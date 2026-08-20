//! External database connectors for syncing data into thingd.
//!
//! Connectors pull data from external sources (CSV, JSON, `Postgres`, `MySQL`)
//! and sync it into thingd collections via a streaming `PullStream` interface.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{ThingdError, ThingdResult};

/// A streaming iterator of rows returned by a connector's `pull()` method.
/// Each item is either a JSON value or an error from the underlying source.
pub type PullStream = Box<dyn Iterator<Item = ThingdResult<serde_json::Value>> + Send>;

/// SSL/TLS mode for database connections.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SslMode {
    /// No encryption
    Disable,
    /// Prefer TLS if available
    #[default]
    Prefer,
    /// Require TLS
    Require,
}

/// Authentication details for database connectors.
#[derive(Debug, Clone)]
pub struct ConnectorAuth {
    /// Database username
    pub username: String,
    /// Database password
    pub password: String,
    /// Database host
    pub host: String,
    /// Database port
    pub port: u16,
    /// Database name
    pub database: String,
    /// SSL/TLS mode
    pub ssl_mode: SslMode,
}

impl ConnectorAuth {
    /// Build a Postgres connection string.
    pub fn postgres_uri(&self) -> String {
        let ssl_mode = match self.ssl_mode {
            SslMode::Disable => "disable",
            SslMode::Prefer => "prefer",
            SslMode::Require => "require",
        };
        format!(
            "postgres://{}:{}@{}:{}/{}?sslmode={}",
            self.username, self.password, self.host, self.port, self.database, ssl_mode
        )
    }

    /// Build a `MySQL` connection string.
    pub fn mysql_uri(&self) -> String {
        format!(
            "mysql://{}:{}@{}:{}/{}",
            self.username, self.password, self.host, self.port, self.database
        )
    }
}

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

    /// Optional: authentication for database connectors
    pub auth: Option<ConnectorAuth>,

    /// Number of rows to fetch per batch when streaming (DB connectors only).
    /// Defaults to 1000.
    pub batch_size: usize,

    /// Connector-specific source options, preserved by generic clients.
    ///
    /// Cloud integrations should store this value opaquely and render the
    /// connector's [`ConnectorDescriptor::config_schema`] rather than
    /// branching on connector names.
    pub source_options: Option<serde_json::Value>,
}

impl Default for ConnectorConfig {
    fn default() -> Self {
        Self {
            connector_type: String::new(),
            source: String::new(),
            collection: String::new(),
            sync_strategy: SyncStrategy::Full,
            query: None,
            column_mapping: None,
            auth: None,
            batch_size: 1000,
            source_options: None,
        }
    }
}

/// A capability advertised by a connector to generic clients.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectorDescriptor {
    /// Stable connector identifier used in API paths and persisted configs.
    pub id: String,
    /// Human-readable connector name.
    pub display_name: String,
    /// Operations supported by this connector.
    pub operations: Vec<ConnectorOperation>,
    /// Accepted source kinds, such as `file`, `url`, or `public_export`.
    pub source_kinds: Vec<String>,
    /// JSON Schema-like configuration metadata for generic clients.
    pub config_schema: serde_json::Value,
    /// Connector-specific limits and feature flags.
    pub metadata: serde_json::Value,
}

/// Operation supported by a connector.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorOperation {
    /// Test whether the source can be reached and read.
    Validate,
    /// Discover sheets, tables, columns, or other source structure.
    Discover,
    /// Return a bounded sample without importing it.
    Preview,
    /// Import source rows into Thingd.
    Pull,
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

    /// Describe this connector for generic API clients.
    ///
    /// A default descriptor keeps existing third-party connectors compatible;
    /// built-in connectors should override it with their complete source and
    /// configuration metadata.
    fn descriptor(&self) -> ConnectorDescriptor {
        ConnectorDescriptor {
            id: self.name().to_string(),
            display_name: self.name().to_string(),
            operations: vec![
                ConnectorOperation::Validate,
                ConnectorOperation::Discover,
                ConnectorOperation::Pull,
            ],
            source_kinds: vec!["source".to_string()],
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string" },
                    "query": { "type": "string" }
                },
                "required": ["source"]
            }),
            metadata: serde_json::json!({}),
        }
    }

    /// Discover the schema of the external source.
    ///
    /// # Errors
    ///
    /// Returns an error when the schema cannot be read from the source.
    fn discover_schema(&self, config: &ConnectorConfig) -> ThingdResult<Schema>;

    /// List available tables/views in the external source.
    ///
    /// Used by the UI to let users pick a table instead of writing raw SQL.
    /// The default implementation returns an empty list (connectors that don't
    /// support table listing should keep the default).
    ///
    /// # Errors
    ///
    /// Returns an error when the table list cannot be fetched.
    fn list_tables(&self, config: &ConnectorConfig) -> ThingdResult<Vec<String>> {
        let _ = config;
        Ok(Vec::new())
    }

    /// Pull data from the source, yielding a stream of objects.
    ///
    /// The returned `PullStream` is an iterator — rows are fetched lazily,
    /// avoiding loading the entire dataset into memory.
    ///
    /// # Errors
    ///
    /// Each item in the stream may return an error from the underlying source.
    fn pull(&self, config: &ConnectorConfig) -> ThingdResult<PullStream>;
}

/// CSV/JSON file connector.
pub struct FileConnector;

/// Excel workbook connector for `.xlsx` and related spreadsheet formats.
pub struct ExcelConnector;

/// Google Sheets connector for a public CSV export.
///
/// The sidecar resolves the public export URL to a bounded local file before
/// invoking this connector. Keeping URL fetching outside the engine keeps the
/// core connector contract usable by native and embedded clients.
pub struct GoogleSheetsConnector;

impl Connector for FileConnector {
    fn name(&self) -> &'static str {
        "file"
    }

    fn descriptor(&self) -> ConnectorDescriptor {
        ConnectorDescriptor {
            id: "file".to_string(),
            display_name: "CSV or JSON file".to_string(),
            operations: vec![
                ConnectorOperation::Validate,
                ConnectorOperation::Discover,
                ConnectorOperation::Preview,
                ConnectorOperation::Pull,
            ],
            source_kinds: vec!["file".to_string()],
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "Path to a CSV or JSONL file" },
                    "columnMapping": { "type": "object", "additionalProperties": { "type": "string" } }
                },
                "required": ["source"]
            }),
            metadata: serde_json::json!({ "formats": ["csv", "json", "jsonl", "ndjson"] }),
        }
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

    fn pull(&self, config: &ConnectorConfig) -> ThingdResult<PullStream> {
        let path = Path::new(&config.source);
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let rows: Vec<serde_json::Value> = match extension {
            "csv" => Self::pull_csv(config)?,
            "json" | "jsonl" | "ndjson" => Self::pull_json(config)?,
            _ => {
                return Err(ThingdError::Storage(format!(
                    "unsupported file type: .{extension}"
                )));
            },
        };

        Ok(Box::new(rows.into_iter().map(Ok)))
    }
}

impl Connector for ExcelConnector {
    fn name(&self) -> &'static str {
        "excel"
    }

    fn descriptor(&self) -> ConnectorDescriptor {
        ConnectorDescriptor {
            id: "excel".to_string(),
            display_name: "Excel workbook".to_string(),
            operations: vec![
                ConnectorOperation::Validate,
                ConnectorOperation::Discover,
                ConnectorOperation::Preview,
                ConnectorOperation::Pull,
            ],
            source_kinds: vec!["file".to_string()],
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "Path to an Excel workbook" },
                    "query": { "type": "string", "description": "Worksheet name" },
                    "columnMapping": { "type": "object", "additionalProperties": { "type": "string" } }
                },
                "required": ["source"]
            }),
            metadata: serde_json::json!({ "formats": ["xlsx", "xlsm", "xlsb", "ods"] }),
        }
    }

    fn list_tables(&self, config: &ConnectorConfig) -> ThingdResult<Vec<String>> {
        use calamine::{Reader, open_workbook_auto};

        let workbook = open_workbook_auto(&config.source)
            .map_err(|e| ThingdError::Storage(format!("failed to read Excel workbook: {e}")))?;
        Ok(workbook.sheet_names())
    }

    fn discover_schema(&self, config: &ConnectorConfig) -> ThingdResult<Schema> {
        let rows = Self::read_rows(config)?;
        let Some(headers) = rows.first() else {
            return Err(ThingdError::Storage("Excel worksheet is empty".into()));
        };

        let headers = headers
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let name = value
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .map_or_else(|| format!("column_{}", index + 1), ToString::to_string);
                (index, name)
            })
            .collect::<Vec<_>>();
        let mut columns = headers
            .iter()
            .map(|(_, name)| Column {
                name: name.clone(),
                data_type: ColumnType::Unknown,
                nullable: false,
                sample_values: Vec::new(),
            })
            .collect::<Vec<_>>();

        for row in rows.iter().skip(1).take(100) {
            for (index, value) in row.iter().enumerate().take(columns.len()) {
                if columns[index].sample_values.len() < 10 {
                    columns[index].sample_values.push(value.clone());
                }
                columns[index].nullable |= value.is_null();
            }
        }
        for column in &mut columns {
            column.data_type = infer_type(&column.sample_values);
        }

        Ok(Schema {
            name: config.query.clone().unwrap_or_else(|| "worksheet".into()),
            columns,
            estimated_rows: Some(rows.len().saturating_sub(1) as u64),
        })
    }

    fn pull(&self, config: &ConnectorConfig) -> ThingdResult<PullStream> {
        let rows = Self::read_rows(config)?;
        let Some(headers) = rows.first() else {
            return Ok(Box::new(std::iter::empty()));
        };
        let headers = headers
            .iter()
            .enumerate()
            .map(|(index, value)| {
                value
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .map_or_else(|| format!("column_{}", index + 1), ToString::to_string)
            })
            .collect::<Vec<_>>();
        let mapping = config.column_mapping.clone();
        let rows = rows
            .into_iter()
            .skip(1)
            .enumerate()
            .map(move |(index, row)| {
                let mut object = serde_json::Map::new();
                object.insert("_row_index".into(), serde_json::json!(index));
                for (column, value) in headers.iter().zip(row.iter()) {
                    let mapped = mapping
                        .as_ref()
                        .and_then(|mapping| mapping.get(column))
                        .unwrap_or(column);
                    object.insert(mapped.clone(), value.clone());
                }
                Ok(serde_json::Value::Object(object))
            });
        Ok(Box::new(rows))
    }
}

impl ExcelConnector {
    fn read_rows(config: &ConnectorConfig) -> ThingdResult<Vec<Vec<serde_json::Value>>> {
        use calamine::{Reader, open_workbook_auto};

        let mut workbook = open_workbook_auto(&config.source)
            .map_err(|e| ThingdError::Storage(format!("failed to read Excel workbook: {e}")))?;
        let sheet = config
            .query
            .clone()
            .or_else(|| workbook.sheet_names().first().cloned())
            .ok_or_else(|| ThingdError::Storage("Excel workbook has no worksheets".into()))?;
        let range = workbook
            .worksheet_range(&sheet)
            .map_err(|e| ThingdError::Storage(format!("failed to read worksheet {sheet}: {e}")))?;
        Ok(range
            .rows()
            .map(|row| row.iter().map(excel_value_to_json).collect())
            .collect())
    }
}

impl Connector for GoogleSheetsConnector {
    fn name(&self) -> &'static str {
        "google-sheets"
    }

    fn descriptor(&self) -> ConnectorDescriptor {
        ConnectorDescriptor {
            id: "google-sheets".to_string(),
            display_name: "Google Sheets".to_string(),
            operations: vec![
                ConnectorOperation::Validate,
                ConnectorOperation::Discover,
                ConnectorOperation::Preview,
                ConnectorOperation::Pull,
            ],
            source_kinds: vec!["public_export_url".to_string()],
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string", "format": "uri", "description": "Public Google Sheets CSV export URL" },
                    "columnMapping": { "type": "object", "additionalProperties": { "type": "string" } }
                },
                "required": ["source"]
            }),
            metadata: serde_json::json!({
                "format": "csv_export",
                "authentication": "public_url",
                "oauth": false
            }),
        }
    }

    fn discover_schema(&self, config: &ConnectorConfig) -> ThingdResult<Schema> {
        FileConnector.discover_schema(config)
    }

    fn pull(&self, config: &ConnectorConfig) -> ThingdResult<PullStream> {
        FileConnector.pull(config)
    }
}

fn excel_value_to_json(value: &calamine::Data) -> serde_json::Value {
    use calamine::Data;

    match value {
        Data::Empty => serde_json::Value::Null,
        Data::String(value) => serde_json::Value::String(value.clone()),
        Data::Float(value) => serde_json::Number::from_f64(*value)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        Data::Int(value) => serde_json::json!(value),
        Data::Bool(value) => serde_json::json!(value),
        Data::DateTime(value) => serde_json::Value::String(value.to_string()),
        Data::DateTimeIso(value) | Data::DurationIso(value) => {
            serde_json::Value::String(value.clone())
        },
        Data::Error(value) => serde_json::Value::String(value.to_string()),
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
    if let Ok(f) = s.parse::<f64>()
        && let Some(n) = serde_json::Number::from_f64(f)
    {
        return serde_json::Value::Number(n);
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
            ..Default::default()
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
            ..Default::default()
        };

        let stream = connector.pull(&config).unwrap();
        let objects: Vec<serde_json::Value> = stream.collect::<ThingdResult<Vec<_>>>().unwrap();
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
            ..Default::default()
        };

        let stream = connector.pull(&config).unwrap();
        let objects: Vec<serde_json::Value> = stream.collect::<ThingdResult<Vec<_>>>().unwrap();
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

    #[test]
    fn connector_auth_postgres_uri() {
        let auth = ConnectorAuth {
            username: "user".to_string(),
            password: "pass".to_string(),
            host: "localhost".to_string(),
            port: 5432,
            database: "mydb".to_string(),
            ssl_mode: SslMode::Disable,
        };
        assert_eq!(
            auth.postgres_uri(),
            "postgres://user:pass@localhost:5432/mydb?sslmode=disable"
        );
    }

    #[test]
    fn connector_auth_mysql_uri() {
        let auth = ConnectorAuth {
            username: "root".to_string(),
            password: "secret".to_string(),
            host: "db.example.com".to_string(),
            port: 3306,
            database: "analytics".to_string(),
            ssl_mode: SslMode::Prefer,
        };
        assert_eq!(
            auth.mysql_uri(),
            "mysql://root:secret@db.example.com:3306/analytics"
        );
    }

    #[test]
    fn pull_stream_from_vec() {
        let data = vec![
            serde_json::json!({"id": 1}),
            serde_json::json!({"id": 2}),
            serde_json::json!({"id": 3}),
        ];
        let stream: PullStream = Box::new(data.into_iter().map(Ok));
        let results: Vec<serde_json::Value> = stream.collect::<ThingdResult<Vec<_>>>().unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["id"], 1);
        assert_eq!(results[2]["id"], 3);
    }

    #[test]
    fn pull_stream_empty() {
        let stream: PullStream = Box::new(std::iter::empty());
        let results: Vec<serde_json::Value> = stream.collect::<ThingdResult<Vec<_>>>().unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn file_connector_implements_connector() {
        let connector = FileConnector;
        assert_eq!(connector.name(), "file");
        assert_ne!(
            std::any::TypeId::of::<FileConnector>(),
            std::any::TypeId::of::<()>()
        );
    }

    #[test]
    fn built_in_descriptors_are_generic_client_ready() {
        let excel = ExcelConnector.descriptor();
        assert_eq!(excel.id, "excel");
        assert!(excel.source_kinds.contains(&"file".to_string()));
        assert!(excel.operations.contains(&ConnectorOperation::Preview));

        let sheets = GoogleSheetsConnector.descriptor();
        assert_eq!(sheets.id, "google-sheets");
        assert_eq!(sheets.source_kinds, vec!["public_export_url"]);
        assert_eq!(sheets.metadata["oauth"], false);
    }

    #[test]
    fn ssl_mode_default() {
        assert_eq!(SslMode::default(), SslMode::Prefer);
    }

    #[test]
    fn ssl_mode_variants() {
        assert_eq!(SslMode::Disable as u8, 0);
        assert_eq!(SslMode::Prefer as u8, 1);
        assert_eq!(SslMode::Require as u8, 2);
    }

    #[test]
    fn connector_auth_postgres_uri_special_chars() {
        let auth = ConnectorAuth {
            username: "user@host".to_string(),
            password: "p@ss:word!".to_string(),
            host: "localhost".to_string(),
            port: 5432,
            database: "mydb".to_string(),
            ssl_mode: SslMode::Disable,
        };
        let uri = auth.postgres_uri();
        assert!(uri.contains("user@host"));
        assert!(uri.contains("mydb"));
        assert!(uri.contains("sslmode=disable"));
    }

    #[test]
    fn connector_auth_mysql_uri_special_chars() {
        let auth = ConnectorAuth {
            username: "root".to_string(),
            password: "p@ss:word!".to_string(),
            host: "db.example.com".to_string(),
            port: 3306,
            database: "analytics".to_string(),
            ssl_mode: SslMode::Require,
        };
        let uri = auth.mysql_uri();
        assert!(uri.starts_with("mysql://"));
        assert!(uri.contains("analytics"));
    }

    #[test]
    fn connector_config_defaults() {
        let config = ConnectorConfig::default();
        assert!(config.connector_type.is_empty());
        assert!(config.source.is_empty());
        assert!(config.collection.is_empty());
        assert!(config.query.is_none());
        assert_eq!(config.batch_size, 1000);
    }

    #[test]
    fn infer_type_all_nulls() {
        let samples = vec![serde_json::Value::Null, serde_json::Value::Null];
        assert_eq!(infer_type(&samples), ColumnType::Unknown);
    }

    #[test]
    fn infer_type_empty() {
        let samples: Vec<serde_json::Value> = vec![];
        assert_eq!(infer_type(&samples), ColumnType::Unknown);
    }

    #[test]
    fn infer_type_float() {
        let samples = vec![serde_json::json!(1.5), serde_json::json!(2.7)];
        assert_eq!(infer_type(&samples), ColumnType::Float);
    }

    #[test]
    fn infer_type_boolean() {
        let samples = vec![serde_json::json!(true), serde_json::json!(false)];
        assert_eq!(infer_type(&samples), ColumnType::Boolean);
    }

    #[test]
    fn infer_type_string() {
        let samples = vec![serde_json::json!("hello"), serde_json::json!("world")];
        assert_eq!(infer_type(&samples), ColumnType::Text);
    }

    #[test]
    fn infer_json_value_negative() {
        let v = infer_json_value("-42");
        assert_eq!(v, serde_json::json!(-42));
    }

    #[test]
    fn infer_json_value_large_number() {
        let v = infer_json_value("9999999999999");
        assert!(v.is_number());
    }

    #[test]
    fn csv_empty_rows() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("empty.csv");
        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(file, "name,age").unwrap();

        let connector = FileConnector;
        let config = ConnectorConfig {
            connector_type: "csv".to_string(),
            source: file_path.to_str().unwrap().to_string(),
            collection: "test".to_string(),
            ..Default::default()
        };

        let stream = connector.pull(&config).unwrap();
        let objects: Vec<serde_json::Value> = stream.collect::<ThingdResult<Vec<_>>>().unwrap();
        assert!(objects.is_empty());
    }

    #[test]
    fn csv_single_column() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("single.csv");
        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(file, "value\nhello\nworld").unwrap();

        let connector = FileConnector;
        let config = ConnectorConfig {
            connector_type: "csv".to_string(),
            source: file_path.to_str().unwrap().to_string(),
            collection: "test".to_string(),
            ..Default::default()
        };

        let schema = connector.discover_schema(&config).unwrap();
        assert_eq!(schema.columns.len(), 1);
        assert_eq!(schema.columns[0].name, "value");

        let stream = connector.pull(&config).unwrap();
        let objects: Vec<serde_json::Value> = stream.collect::<ThingdResult<Vec<_>>>().unwrap();
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0]["value"], "hello");
    }

    #[test]
    fn jsonl_single_object() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("single.jsonl");
        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(file, "{{\"id\":1,\"name\":\"only\"}}").unwrap();

        let connector = FileConnector;
        let config = ConnectorConfig {
            connector_type: "json".to_string(),
            source: file_path.to_str().unwrap().to_string(),
            collection: "test".to_string(),
            ..Default::default()
        };

        let stream = connector.pull(&config).unwrap();
        let objects: Vec<serde_json::Value> = stream.collect::<ThingdResult<Vec<_>>>().unwrap();
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0]["name"], "only");
    }

    #[test]
    fn list_tables_returns_empty_for_file_connector() {
        let connector = FileConnector;
        let config = ConnectorConfig::default();
        let tables = connector.list_tables(&config).unwrap();
        assert!(tables.is_empty());
    }

    #[test]
    fn connector_name_constants() {
        assert_eq!(FileConnector.name(), "file");
    }
}
