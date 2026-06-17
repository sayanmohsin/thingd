//! External database connectors for syncing data into thingd.
//!
//! Connectors pull data from external sources (CSV, JSON, Postgres, `MySQL`)
//! and sync it into thingd collections.

use std::collections::HashMap;
use std::path::Path;

use crate::{ThingdError, ThingdResult};

/// Configuration for a connector instance.
#[derive(Debug, Clone)]
pub struct ConnectorConfig {
    /// Connector type: "csv", "json", "postgres", "mysql"
    pub connector_type: String,

    /// Connection string or file path
    pub source: String,

    /// Collection to sync into
    pub collection: String,

    /// Sync strategy: full or incremental
    pub sync_strategy: SyncStrategy,

    /// Optional: specific table/view/query to pull from
    pub query: Option<String>,

    /// Optional: column mapping (`external_name` → `thingd_field`)
    pub column_mapping: Option<HashMap<String, String>>,
}

/// Sync strategy for pulling data.
#[derive(Debug, Clone)]
pub enum SyncStrategy {
    /// Pull all data every time
    Full,
    /// Only pull new/changed data since last sync
    Incremental {
        /// Column name to use as cursor for incremental sync
        cursor_column: String,
    },
}

/// Schema of an external source.
#[derive(Debug, Clone)]
pub struct Schema {
    /// Table/view/file name
    pub name: String,

    /// Column definitions
    pub columns: Vec<Column>,

    /// Estimated total rows
    pub estimated_rows: Option<u64>,
}

/// Column definition from schema discovery.
#[derive(Debug, Clone)]
pub struct Column {
    /// Column name
    pub name: String,

    /// Inferred data type
    pub data_type: ColumnType,

    /// Whether the column is nullable
    pub nullable: bool,

    /// Sample values for type inference
    pub sample_values: Vec<serde_json::Value>,
}

/// Inferred column data type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnType {
    /// String/text data
    Text,
    /// Integer numbers
    Integer,
    /// Floating point numbers
    Float,
    /// Boolean values
    Boolean,
    /// ISO timestamp strings
    Timestamp,
    /// JSON objects/arrays
    Json,
    /// Unknown type (treated as text)
    Unknown,
}

/// A connector pulls data from an external source into thingd.
pub trait Connector: Send + Sync {
    /// Human-readable name for this connector type.
    fn name(&self) -> &'static str;

    /// Discover the schema of the external source.
    ///
    /// # Errors
    ///
    /// Returns an error when the schema cannot be read from the source.
    fn discover_schema(&self, config: &ConnectorConfig) -> ThingdResult<Schema>;

    /// Pull data from the source, yielding batches of objects.
    ///
    /// # Errors
    ///
    /// Returns an error when data cannot be read from the source.
    fn pull(&self, config: &ConnectorConfig) -> ThingdResult<Vec<serde_json::Value>>;
}

/// CSV/JSON file connector.
pub struct FileConnector;

impl Connector for FileConnector {
    fn name(&self) -> &'static str {
        "file"
    }

    fn discover_schema(&self, config: &ConnectorConfig) -> ThingdResult<Schema> {
        let path = Path::new(&config.source);
        if !path.exists() {
            return Err(ThingdError::Storage(format!(
                "file not found: {}",
                config.source
            )));
        }

        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        match extension {
            "csv" => Self::discover_csv_schema(config),
            "json" | "jsonl" | "ndjson" => Self::discover_json_schema(config),
            _ => Err(ThingdError::Storage(format!(
                "unsupported file type: .{extension}"
            ))),
        }
    }

    fn pull(&self, config: &ConnectorConfig) -> ThingdResult<Vec<serde_json::Value>> {
        let path = Path::new(&config.source);
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        match extension {
            "csv" => Self::pull_csv(config),
            "json" | "jsonl" | "ndjson" => Self::pull_json(config),
            _ => Err(ThingdError::Storage(format!(
                "unsupported file type: .{extension}"
            ))),
        }
    }
}

impl FileConnector {
    fn discover_csv_schema(config: &ConnectorConfig) -> ThingdResult<Schema> {
        let mut reader = csv::Reader::from_path(&config.source)
            .map_err(|e| ThingdError::Storage(format!("failed to read CSV: {e}")))?;

        let headers: Vec<String> = reader
            .headers()
            .map_err(|e| ThingdError::Storage(format!("failed to read CSV headers: {e}")))?
            .iter()
            .map(ToString::to_string)
            .collect();

        let mut columns: Vec<Column> = headers
            .iter()
            .map(|h| Column {
                name: h.clone(),
                data_type: ColumnType::Unknown,
                nullable: false,
                sample_values: Vec::new(),
            })
            .collect();

        // Sample up to 100 rows for type inference
        for (sample_count, result) in reader.records().enumerate() {
            let record =
                result.map_err(|e| ThingdError::Storage(format!("CSV read error: {e}")))?;
            if sample_count >= 100 {
                break;
            }
            for (i, field) in record.iter().enumerate() {
                if i < columns.len() {
                    let value = infer_json_value(field);
                    if columns[i].sample_values.len() < 10 {
                        columns[i].sample_values.push(value);
                    }
                }
            }
        }

        // Infer types from samples
        for column in &mut columns {
            column.data_type = infer_type(&column.sample_values);
        }

        let name = Path::new(&config.source)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(Schema {
            name,
            columns,
            estimated_rows: None,
        })
    }

    fn discover_json_schema(config: &ConnectorConfig) -> ThingdResult<Schema> {
        let content = std::fs::read_to_string(&config.source)
            .map_err(|e| ThingdError::Storage(format!("failed to read JSON file: {e}")))?;

        let mut columns: HashMap<String, Column> = HashMap::new();

        for (sample_count, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || sample_count >= 100 {
                break;
            }

            let value: serde_json::Value = serde_json::from_str(line)
                .map_err(|e| ThingdError::Storage(format!("JSON parse error: {e}")))?;

            if let Some(obj) = value.as_object() {
                for (key, val) in obj {
                    let column = columns.entry(key.clone()).or_insert_with(|| Column {
                        name: key.clone(),
                        data_type: ColumnType::Unknown,
                        nullable: false,
                        sample_values: Vec::new(),
                    });
                    if column.sample_values.len() < 10 {
                        column.sample_values.push(val.clone());
                    }
                    if val.is_null() {
                        column.nullable = true;
                    }
                }
            }
        }

        // Infer types from samples
        for column in columns.values_mut() {
            column.data_type = infer_type(&column.sample_values);
        }

        let mut columns_vec: Vec<Column> = columns.into_values().collect();
        columns_vec.sort_by(|a, b| a.name.cmp(&b.name));

        let name = Path::new(&config.source)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(Schema {
            name,
            columns: columns_vec,
            estimated_rows: None,
        })
    }

    fn pull_csv(config: &ConnectorConfig) -> ThingdResult<Vec<serde_json::Value>> {
        let mut reader = csv::Reader::from_path(&config.source)
            .map_err(|e| ThingdError::Storage(format!("failed to read CSV: {e}")))?;

        let headers: Vec<String> = reader
            .headers()
            .map_err(|e| ThingdError::Storage(format!("failed to read CSV headers: {e}")))?
            .iter()
            .map(ToString::to_string)
            .collect();

        let mut objects = Vec::new();

        for (index, result) in reader.records().enumerate() {
            let record =
                result.map_err(|e| ThingdError::Storage(format!("CSV read error: {e}")))?;

            let mut obj = serde_json::Map::new();

            // Add row index as ID
            obj.insert(
                "_row_index".to_string(),
                serde_json::Value::Number(index.into()),
            );

            for (i, field) in record.iter().enumerate() {
                if i < headers.len() {
                    let key = &headers[i];
                    let mapped_key = config
                        .column_mapping
                        .as_ref()
                        .and_then(|m| m.get(key))
                        .unwrap_or(key);
                    obj.insert(mapped_key.clone(), infer_json_value(field));
                }
            }

            objects.push(serde_json::Value::Object(obj));
        }

        Ok(objects)
    }

    fn pull_json(config: &ConnectorConfig) -> ThingdResult<Vec<serde_json::Value>> {
        let content = std::fs::read_to_string(&config.source)
            .map_err(|e| ThingdError::Storage(format!("failed to read JSON file: {e}")))?;

        let mut objects = Vec::new();

        for (index, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let value: serde_json::Value = serde_json::from_str(line).map_err(|e| {
                ThingdError::Storage(format!("JSON parse error at line {index}: {e}"))
            })?;

            // For JSONL, each line is an object
            if let Some(obj) = value.as_object() {
                let mut obj = obj.clone();
                // Add line index as ID if not present
                if !obj.contains_key("id") {
                    obj.insert(
                        "_row_index".to_string(),
                        serde_json::Value::Number(index.into()),
                    );
                }
                objects.push(serde_json::Value::Object(obj));
            } else {
                // For single JSON arrays or values
                objects.push(value);
            }
        }

        Ok(objects)
    }
}

/// Infer a JSON value from a string field.
fn infer_json_value(s: &str) -> serde_json::Value {
    if s.is_empty() {
        return serde_json::Value::Null;
    }

    // Try boolean
    if s.eq_ignore_ascii_case("true") {
        return serde_json::Value::Bool(true);
    }
    if s.eq_ignore_ascii_case("false") {
        return serde_json::Value::Bool(false);
    }

    // Try integer
    if let Ok(n) = s.parse::<i64>() {
        return serde_json::Value::Number(n.into());
    }

    // Try float
    if let Ok(f) = s.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return serde_json::Value::Number(n);
        }
    }

    // Try JSON
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
        return v;
    }

    // Default to string
    serde_json::Value::String(s.to_string())
}

/// Infer column type from sample values.
fn infer_type(samples: &[serde_json::Value]) -> ColumnType {
    if samples.is_empty() {
        return ColumnType::Unknown;
    }

    // Filter out null values for type inference
    let non_null: Vec<&serde_json::Value> = samples.iter().filter(|s| !s.is_null()).collect();

    if non_null.is_empty() {
        return ColumnType::Unknown;
    }

    let mut has_integer = true;
    let mut has_float = true;
    let mut has_boolean = true;
    let mut has_timestamp = true;

    for sample in &non_null {
        match sample {
            serde_json::Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    has_float = false;
                } else {
                    has_integer = false;
                }
                has_boolean = false;
                has_timestamp = false;
            },
            serde_json::Value::Bool(_) => {
                has_integer = false;
                has_float = false;
                has_timestamp = false;
            },
            serde_json::Value::String(s) => {
                has_integer = false;
                has_float = false;
                has_boolean = false;
                // Check if it looks like a timestamp
                if !s.ends_with('Z') && !s.contains('+') && !s.contains('T') {
                    has_timestamp = false;
                }
            },
            _ => {
                has_integer = false;
                has_float = false;
                has_boolean = false;
                has_timestamp = false;
            },
        }
    }

    if has_integer {
        ColumnType::Integer
    } else if has_float {
        ColumnType::Float
    } else if has_boolean {
        ColumnType::Boolean
    } else if has_timestamp {
        ColumnType::Timestamp
    } else {
        ColumnType::Text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn discovers_csv_schema() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.csv");
        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(
            file,
            "name,age,active\nAlice,30,true\nBob,25,false\nCharlie,35,true"
        )
        .unwrap();

        let connector = FileConnector;
        let config = ConnectorConfig {
            connector_type: "csv".to_string(),
            source: file_path.to_str().unwrap().to_string(),
            collection: "users".to_string(),
            sync_strategy: SyncStrategy::Full,
            query: None,
            column_mapping: None,
        };

        let schema = connector.discover_schema(&config).unwrap();
        assert_eq!(schema.columns.len(), 3);
        assert_eq!(schema.columns[0].name, "name");
        assert_eq!(schema.columns[0].data_type, ColumnType::Text);
        assert_eq!(schema.columns[1].name, "age");
        assert_eq!(schema.columns[1].data_type, ColumnType::Integer);
        assert_eq!(schema.columns[2].name, "active");
        assert_eq!(schema.columns[2].data_type, ColumnType::Boolean);
    }

    #[test]
    fn pulls_csv_data() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.csv");
        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(file, "name,age\nAlice,30\nBob,25").unwrap();

        let connector = FileConnector;
        let config = ConnectorConfig {
            connector_type: "csv".to_string(),
            source: file_path.to_str().unwrap().to_string(),
            collection: "users".to_string(),
            sync_strategy: SyncStrategy::Full,
            query: None,
            column_mapping: None,
        };

        let objects = connector.pull(&config).unwrap();
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0]["name"], "Alice");
        assert_eq!(objects[0]["age"], 30);
        assert_eq!(objects[1]["name"], "Bob");
        assert_eq!(objects[1]["age"], 25);
    }

    #[test]
    fn pulls_jsonl_data() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.jsonl");
        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(
            file,
            "{{\"name\":\"Alice\",\"age\":30}}\n{{\"name\":\"Bob\",\"age\":25}}"
        )
        .unwrap();

        let connector = FileConnector;
        let config = ConnectorConfig {
            connector_type: "json".to_string(),
            source: file_path.to_str().unwrap().to_string(),
            collection: "users".to_string(),
            sync_strategy: SyncStrategy::Full,
            query: None,
            column_mapping: None,
        };

        let objects = connector.pull(&config).unwrap();
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0]["name"], "Alice");
        assert_eq!(objects[1]["name"], "Bob");
    }

    #[test]
    fn infer_json_value_empty_string() {
        assert_eq!(infer_json_value(""), serde_json::Value::Null);
    }

    #[test]
    fn infer_json_value_boolean() {
        assert_eq!(infer_json_value("true"), serde_json::Value::Bool(true));
        assert_eq!(infer_json_value("false"), serde_json::Value::Bool(false));
    }

    #[test]
    fn infer_json_value_integer() {
        let v = infer_json_value("42");
        assert_eq!(v, serde_json::json!(42));
    }

    #[test]
    fn infer_json_value_float() {
        let v = infer_json_value("3.14");
        assert!(v.is_number());
    }

    #[test]
    fn infer_json_value_string() {
        let v = infer_json_value("hello world");
        assert_eq!(v, serde_json::Value::String("hello world".to_string()));
    }

    #[test]
    fn infer_type_integer() {
        let samples = vec![
            serde_json::json!(1),
            serde_json::json!(2),
            serde_json::json!(3),
        ];
        assert_eq!(infer_type(&samples), ColumnType::Integer);
    }

    #[test]
    fn infer_type_mixed_with_nulls() {
        let samples = vec![
            serde_json::json!(1),
            serde_json::Value::Null,
            serde_json::json!(3),
        ];
        assert_eq!(infer_type(&samples), ColumnType::Integer);
    }
}
