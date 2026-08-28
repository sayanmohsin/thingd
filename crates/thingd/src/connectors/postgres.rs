//! Postgres connector — pulls data from `PostgreSQL` databases via `sqlx`.
//!
//! Uses an internal `tokio::runtime::Runtime` to run async `sqlx` queries
//! synchronously, matching the `Connector` trait's sync `PullStream` interface.

use crate::connector::{
    Column as ConnColumn, ColumnType, Connector, ConnectorConfig, PullStream, Schema,
};
use crate::{ThingdError, ThingdResult};
use sqlx::{Column, Row};

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
        let pool = self.pool(config)?;
        let query = config
            .query
            .as_deref()
            .ok_or_else(|| ThingdError::Storage("Postgres connector requires a query".into()))?
            .to_string();
        let batch_size = config.batch_size.max(1);

        // We fetch all rows upfront via sqlx, then stream them.
        // For truly large datasets, this should use an async channel-based
        // approach — but for the common case (tables under 100K rows),
        // loading into memory is fast enough.
        let rows = self
            .runtime
            .as_ref()
            .expect("PostgresConnector runtime already shut down")
            .block_on(sqlx::query(sqlx::AssertSqlSafe(query)).fetch_all(&pool))
            .map_err(|e| ThingdError::Storage(format!("Postgres query failed: {e}")))?;

        let total = rows.len();
        let mut cursor = 0usize;

        // Build column name list from the first row
        let columns: Vec<String> = if let Some(first) = rows.first() {
            first
                .columns()
                .iter()
                .map(|c| c.name().to_string())
                .collect()
        } else {
            return Ok(Box::new(std::iter::empty()));
        };

        let stream = std::iter::from_fn(move || {
            let remaining = total.saturating_sub(cursor);
            if remaining == 0 {
                return None;
            }

            let end = (cursor + batch_size).min(total);
            let batch = &rows[cursor..end];
            cursor = end;

            Some(
                batch
                    .iter()
                    .map(|row| {
                        let mut obj = serde_json::Map::new();
                        for (i, col) in columns.iter().enumerate() {
                            let value = pg_row_to_json_value(row, i);
                            obj.insert(col.clone(), value);
                        }
                        Ok(serde_json::Value::Object(obj))
                    })
                    .collect::<Vec<ThingdResult<serde_json::Value>>>()
                    .into_iter(),
            )
        })
        .flatten();

        Ok(Box::new(stream))
    }
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
