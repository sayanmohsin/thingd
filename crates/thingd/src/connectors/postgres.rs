//! Postgres connector — pulls data from `PostgreSQL` databases via `sqlx`.
//!
//! Uses an internal `tokio::runtime::Runtime` to run async `sqlx` queries
//! synchronously, matching the `Connector` trait's sync `PullStream` interface.

use crate::connector::{
    Column as ConnColumn, ColumnType, Connector, ConnectorConfig, PullStream, Schema,
};
use crate::{ThingdError, ThingdResult};
use futures_util::StreamExt;
use sqlx::{Column, Row};
use tokio::sync::mpsc;

/// Connector that pulls data from a `PostgreSQL` database.
pub struct PostgresConnector {
    runtime: Option<tokio::runtime::Runtime>,
}

impl Default for PostgresConnector {
    fn default() -> Self {
        Self {
            runtime: Some(
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build tokio runtime for PostgresConnector"),
            ),
        }
    }
}

impl Drop for PostgresConnector {
    fn drop(&mut self) {
        // The connector is owned by async sidecar request handlers. Tokio's
        // normal Runtime::drop blocks while shutting down, which panics when
        // called from an async context. Background shutdown avoids blocking
        // the request executor and lets in-flight database work finish.
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl PostgresConnector {
    /// Create a new Postgres connector.
    pub fn new() -> Self {
        Self::default()
    }

    fn pool(&self, config: &ConnectorConfig) -> ThingdResult<sqlx::PgPool> {
        let auth = config.auth.as_ref().ok_or_else(|| {
            ThingdError::Storage("Postgres connector requires auth config".into())
        })?;

        let uri = auth.postgres_uri();
        self.runtime
            .as_ref()
            .expect("PostgresConnector runtime already shut down")
            .block_on(sqlx::PgPool::connect(&uri))
            .map_err(|e| ThingdError::Storage(format!("failed to connect to Postgres: {e}")))
    }
}

impl Connector for PostgresConnector {
    fn name(&self) -> &'static str {
        "postgres"
    }

    fn list_tables(&self, config: &ConnectorConfig) -> ThingdResult<Vec<String>> {
        let pool = self.pool(config)?;
        let rows = self
            .runtime
            .as_ref()
            .expect("PostgresConnector runtime already shut down")
            .block_on(
                sqlx::query_as::<_, (String,)>(
                    "SELECT table_name FROM information_schema.tables \
                 WHERE table_schema = 'public' AND table_type = 'BASE TABLE' \
                 ORDER BY table_name",
                )
                .fetch_all(&pool),
            )
            .map_err(|e| ThingdError::Storage(format!("failed to list tables: {e}")))?;
        Ok(rows.into_iter().map(|(name,)| name).collect())
    }

    fn discover_schema(&self, config: &ConnectorConfig) -> ThingdResult<Schema> {
        let pool = self.pool(config)?;
        let table_name = config.query.as_deref().unwrap_or("");

        if table_name.is_empty() {
            return Err(ThingdError::Storage(
                "Postgres connector requires a query or table name".into(),
            ));
        }

        // Query information_schema for column metadata
        let rows = self
            .runtime
            .as_ref()
            .expect("PostgresConnector runtime already shut down")
            .block_on(
                sqlx::query_as::<_, (String, String, String)>(
                    "SELECT column_name, data_type, is_nullable::text \
                 FROM information_schema.columns \
                 WHERE table_name = $1 \
                 ORDER BY ordinal_position",
                )
                .bind(table_name)
                .fetch_all(&pool),
            )
            .map_err(|e| ThingdError::Storage(format!("schema discovery failed: {e}")))?;

        let columns: Vec<ConnColumn> = rows
            .into_iter()
            .map(|(name, data_type, nullable)| ConnColumn {
                name,
                data_type: pg_type_to_column_type(&data_type),
                nullable: nullable == "YES",
                sample_values: Vec::new(),
            })
            .collect();

        Ok(Schema {
            name: table_name.to_string(),
            columns,
            estimated_rows: None,
        })
    }

    fn pull(&self, config: &ConnectorConfig) -> ThingdResult<PullStream> {
        let auth = config.auth.as_ref().ok_or_else(|| {
            ThingdError::Storage("Postgres connector requires auth config".into())
        })?;
        let uri = auth.postgres_uri();
        let query = config
            .query
            .as_deref()
            .ok_or_else(|| ThingdError::Storage("Postgres connector requires a query".into()))?
            .to_string();
        let capacity = config.batch_size.clamp(1, 1_000);
        let (sender, mut receiver) = mpsc::channel(capacity);
        let error_sender = sender.clone();
        std::thread::Builder::new()
            .name("thingd-postgres-pull".to_string())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = error_sender.blocking_send(Err(ThingdError::Storage(format!(
                            "failed to build Postgres pull runtime: {error}"
                        ))));
                        return;
                    },
                };
                let result = runtime.block_on(async move {
                    let pool = sqlx::PgPool::connect(&uri).await.map_err(|error| {
                        ThingdError::Storage(format!("Postgres query failed: {error}"))
                    })?;
                    let mut rows = sqlx::query(sqlx::AssertSqlSafe(query)).fetch(&pool);
                    while let Some(row) = rows.next().await {
                        let row = row.map_err(|error| {
                            ThingdError::Storage(format!("Postgres query failed: {error}"))
                        })?;
                        let value = postgres_row_to_json_value(&row);
                        if sender.send(Ok(value)).await.is_err() {
                            return Ok::<_, ThingdError>(());
                        }
                    }
                    Ok(())
                });
                if let Err(error) = result {
                    let _ = error_sender.blocking_send(Err(error));
                }
            })
            .map_err(|error| {
                ThingdError::Storage(format!("failed to start Postgres pull: {error}"))
            })?;

        Ok(Box::new(std::iter::from_fn(move || {
            receiver.blocking_recv()
        })))
    }
}

fn postgres_row_to_json_value(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    for (index, column) in row.columns().iter().enumerate() {
        object.insert(column.name().to_string(), pg_row_to_json_value(row, index));
    }
    serde_json::Value::Object(object)
}

fn pg_type_to_column_type(pg_type: &str) -> ColumnType {
    match pg_type {
        t if t == "bigint" || t == "smallint" => ColumnType::Integer,
        t if t.contains("int") || t.contains("serial") => ColumnType::Integer,
        t if t.contains("float")
            || t.contains("double")
            || t.contains("numeric")
            || t.contains("real") =>
        {
            ColumnType::Float
        },
        "boolean" => ColumnType::Boolean,
        t if t.contains("timestamp") || t.contains("date") || t == "time" => ColumnType::Timestamp,
        t if t == "json" || t == "jsonb" => ColumnType::Json,
        _ => ColumnType::Text,
    }
}

fn pg_row_to_json_value(row: &sqlx::postgres::PgRow, index: usize) -> serde_json::Value {
    if let Ok(v) = row.try_get::<i64, _>(index) {
        return serde_json::json!(v);
    }
    if let Ok(v) = row.try_get::<f64, _>(index)
        && let Some(n) = serde_json::Number::from_f64(v)
    {
        return serde_json::Value::Number(n);
    }
    if let Ok(v) = row.try_get::<bool, _>(index) {
        return serde_json::json!(v);
    }
    if let Ok(v) = row.try_get::<String, _>(index) {
        return serde_json::json!(v);
    }
    serde_json::Value::Null
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_pg_types() {
        assert_eq!(pg_type_to_column_type("integer"), ColumnType::Integer);
        assert_eq!(pg_type_to_column_type("bigint"), ColumnType::Integer);
        assert_eq!(pg_type_to_column_type("smallint"), ColumnType::Integer);
        assert_eq!(pg_type_to_column_type("serial"), ColumnType::Integer);
        assert_eq!(pg_type_to_column_type("bigserial"), ColumnType::Integer);
        assert_eq!(
            pg_type_to_column_type("double precision"),
            ColumnType::Float
        );
        assert_eq!(pg_type_to_column_type("numeric"), ColumnType::Float);
        assert_eq!(pg_type_to_column_type("real"), ColumnType::Float);
        assert_eq!(pg_type_to_column_type("boolean"), ColumnType::Boolean);
        assert_eq!(
            pg_type_to_column_type("timestamp without time zone"),
            ColumnType::Timestamp
        );
        assert_eq!(pg_type_to_column_type("date"), ColumnType::Timestamp);
        assert_eq!(pg_type_to_column_type("jsonb"), ColumnType::Json);
        assert_eq!(pg_type_to_column_type("json"), ColumnType::Json);
        assert_eq!(pg_type_to_column_type("text"), ColumnType::Text);
        assert_eq!(
            pg_type_to_column_type("character varying"),
            ColumnType::Text
        );
        assert_eq!(pg_type_to_column_type("uuid"), ColumnType::Text);
    }

    #[test]
    fn pg_type_money_maps_to_text() {
        assert_eq!(pg_type_to_column_type("money"), ColumnType::Text);
    }

    #[test]
    fn pg_type_unknown_maps_to_text() {
        assert_eq!(pg_type_to_column_type("xml"), ColumnType::Text);
        assert_eq!(pg_type_to_column_type("bytea"), ColumnType::Text);
        assert_eq!(pg_type_to_column_type("citext"), ColumnType::Text);
    }

    #[test]
    fn pg_type_varchar_without_length() {
        assert_eq!(
            pg_type_to_column_type("character varying"),
            ColumnType::Text
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn can_drop_inside_async_context() {
        drop(PostgresConnector::new());
    }

    #[test]
    fn pg_type_char() {
        assert_eq!(pg_type_to_column_type("char"), ColumnType::Text);
        assert_eq!(pg_type_to_column_type("character"), ColumnType::Text);
    }

    #[test]
    fn pg_type_timestamp_with_time_zone() {
        assert_eq!(
            pg_type_to_column_type("timestamp with time zone"),
            ColumnType::Timestamp
        );
    }

    #[test]
    fn pg_type_time() {
        assert_eq!(pg_type_to_column_type("time"), ColumnType::Timestamp);
    }
}
