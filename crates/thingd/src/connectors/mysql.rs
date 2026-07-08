//! `MySQL` connector — pulls data from MySQL/MariaDB databases via `sqlx`.
//!
//! Uses an internal `tokio::runtime::Runtime` to run async `sqlx` queries
//! synchronously, matching the `Connector` trait's sync `PullStream` interface.

use crate::connector::{
    Column as ConnColumn, ColumnType, Connector, ConnectorConfig, PullStream, Schema,
};
use crate::{ThingdError, ThingdResult};
use sqlx::{Column, Row};

/// Connector that pulls data from a MySQL/MariaDB database.
pub struct MysqlConnector {
    runtime: tokio::runtime::Runtime,
}

impl Default for MysqlConnector {
    fn default() -> Self {
        Self {
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime for MysqlConnector"),
        }
    }
}

impl MysqlConnector {
    /// Create a new `MySQL` connector.
    pub fn new() -> Self {
        Self::default()
    }

    fn pool(&self, config: &ConnectorConfig) -> ThingdResult<sqlx::MySqlPool> {
        let auth = config
            .auth
            .as_ref()
            .ok_or_else(|| ThingdError::Storage("MySQL connector requires auth config".into()))?;

        let uri = auth.mysql_uri();
        self.runtime
            .block_on(sqlx::MySqlPool::connect(&uri))
            .map_err(|e| ThingdError::Storage(format!("failed to connect to MySQL: {e}")))
    }
}

impl Connector for MysqlConnector {
    fn name(&self) -> &'static str {
        "mysql"
    }

    fn list_tables(&self, config: &ConnectorConfig) -> ThingdResult<Vec<String>> {
        let pool = self.pool(config)?;
        let rows = self
            .runtime
            .block_on(
                sqlx::query_as::<_, (String,)>(
                    "SELECT table_name FROM information_schema.tables \
                 WHERE table_schema = DATABASE() AND table_type = 'BASE TABLE' \
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
                "MySQL connector requires a query or table name".into(),
            ));
        }

        // Query information_schema for column metadata
        let rows = self
            .runtime
            .block_on(
                sqlx::query_as::<_, (String, String, String)>(
                    "SELECT column_name, data_type, is_nullable \
                 FROM information_schema.columns \
                 WHERE table_name = ? \
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
                data_type: mysql_type_to_column_type(&data_type),
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
            .ok_or_else(|| ThingdError::Storage("MySQL connector requires a query".into()))?
            .to_string();
        let batch_size = config.batch_size.max(1);

        let rows = self
            .runtime
            .block_on(sqlx::query(&query).fetch_all(&pool))
            .map_err(|e| ThingdError::Storage(format!("MySQL query failed: {e}")))?;

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
                            let value = mysql_row_to_json_value(row, i);
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

fn mysql_type_to_column_type(mysql_type: &str) -> ColumnType {
    match mysql_type {
        t if t.contains("int")
            || t.contains("tinyint")
            || t.contains("smallint")
            || t.contains("mediumint")
            || t == "bigint"
            || t == "year" =>
        {
            ColumnType::Integer
        },
        t if t.contains("float")
            || t.contains("double")
            || t.contains("decimal")
            || t.contains("numeric")
            || t.contains("real") =>
        {
            ColumnType::Float
        },
        t if t == "boolean" || t == "bool" || t == "bit" => ColumnType::Boolean,
        t if t.contains("timestamp") || t.contains("date") || t == "time" || t == "datetime" => {
            ColumnType::Timestamp
        },
        "json" => ColumnType::Json,
        _ => ColumnType::Text,
    }
}

fn mysql_row_to_json_value(row: &sqlx::mysql::MySqlRow, index: usize) -> serde_json::Value {
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
    fn maps_mysql_types() {
        assert_eq!(mysql_type_to_column_type("int"), ColumnType::Integer);
        assert_eq!(mysql_type_to_column_type("tinyint"), ColumnType::Integer);
        assert_eq!(mysql_type_to_column_type("smallint"), ColumnType::Integer);
        assert_eq!(mysql_type_to_column_type("mediumint"), ColumnType::Integer);
        assert_eq!(mysql_type_to_column_type("bigint"), ColumnType::Integer);
        assert_eq!(mysql_type_to_column_type("year"), ColumnType::Integer);
        assert_eq!(mysql_type_to_column_type("float"), ColumnType::Float);
        assert_eq!(mysql_type_to_column_type("double"), ColumnType::Float);
        assert_eq!(mysql_type_to_column_type("decimal"), ColumnType::Float);
        assert_eq!(mysql_type_to_column_type("numeric"), ColumnType::Float);
        assert_eq!(mysql_type_to_column_type("boolean"), ColumnType::Boolean);
        assert_eq!(mysql_type_to_column_type("bool"), ColumnType::Boolean);
        assert_eq!(mysql_type_to_column_type("bit"), ColumnType::Boolean);
        assert_eq!(
            mysql_type_to_column_type("timestamp"),
            ColumnType::Timestamp
        );
        assert_eq!(mysql_type_to_column_type("datetime"), ColumnType::Timestamp);
        assert_eq!(mysql_type_to_column_type("date"), ColumnType::Timestamp);
        assert_eq!(mysql_type_to_column_type("json"), ColumnType::Json);
        assert_eq!(mysql_type_to_column_type("varchar"), ColumnType::Text);
        assert_eq!(mysql_type_to_column_type("text"), ColumnType::Text);
        assert_eq!(mysql_type_to_column_type("char"), ColumnType::Text);
    }

    #[test]
    fn mysql_type_longtext() {
        assert_eq!(mysql_type_to_column_type("longtext"), ColumnType::Text);
    }

    #[test]
    fn mysql_type_mediumtext() {
        assert_eq!(mysql_type_to_column_type("mediumtext"), ColumnType::Text);
    }

    #[test]
    fn mysql_type_tinytext() {
        assert_eq!(mysql_type_to_column_type("tinytext"), ColumnType::Text);
    }

    #[test]
    fn mysql_type_binary() {
        assert_eq!(mysql_type_to_column_type("binary"), ColumnType::Text);
    }

    #[test]
    fn mysql_type_varbinary() {
        assert_eq!(mysql_type_to_column_type("varbinary"), ColumnType::Text);
    }

    #[test]
    fn mysql_type_blob() {
        assert_eq!(mysql_type_to_column_type("blob"), ColumnType::Text);
    }

    #[test]
    fn mysql_type_unknown() {
        assert_eq!(mysql_type_to_column_type("set"), ColumnType::Text);
        assert_eq!(mysql_type_to_column_type("enum"), ColumnType::Text);
        assert_eq!(mysql_type_to_column_type("point"), ColumnType::Text);
    }

    #[test]
    fn mysql_type_int_unsigned() {
        assert_eq!(mysql_type_to_column_type("int unsigned"), ColumnType::Integer);
    }

    #[test]
    fn mysql_type_double_precision() {
        assert_eq!(mysql_type_to_column_type("double precision"), ColumnType::Float);
    }

    #[test]
    fn mysql_type_real() {
        assert_eq!(mysql_type_to_column_type("real"), ColumnType::Float);
    }

    #[test]
    fn mysql_connector_name() {
        let connector = MysqlConnector;
        assert_eq!(connector.name(), "mysql");
    }
}
