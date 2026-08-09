//! Data model types shared by storage adapters.

use crate::{u64_to_i64, unix_timestamp_millis};

/// Default queue lease duration in milliseconds.
pub const DEFAULT_QUEUE_LEASE_MS: u64 = 30_000;

/// Stable object key inside a collection.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub struct ObjectKey {
    /// Collection name, such as `decisions`, `documents`, or `customers`.
    pub collection: String,
    /// Stable object identifier inside the collection.
    pub id: String,
}

impl ObjectKey {
    /// Create a new object key.
    pub fn new(collection: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            collection: collection.into(),
            id: id.into(),
        }
    }
}

/// An object stored in a thingd collection.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryObject {
    /// Stable object key.
    pub key: ObjectKey,
    /// Serialized object body.
    pub body: String,
    /// Monotonic object version assigned by the store.
    pub version: u64,
    /// ISO 8601 creation timestamp, e.g. "2026-06-01T12:00:00.000Z". Empty if not set.
    pub created_at: String,
    /// ISO 8601 last-update timestamp. Empty if not set.
    pub updated_at: String,
    /// Optional vector embedding for vector search (e.g., for ANN search).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector: Option<Vec<f32>>,
}

impl MemoryObject {
    /// Create a new object record.
    pub fn new(
        collection: impl Into<String>,
        id: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            key: ObjectKey::new(collection, id),
            body: body.into(),
            version: 0,
            created_at: String::new(),
            updated_at: String::new(),
            vector: None,
        }
    }

    /// Attach a vector embedding to this object.
    #[must_use]
    pub fn with_vector(mut self, vector: Vec<f32>) -> Self {
        self.vector = Some(vector);
        self
    }
}

/// An append-only event stored in a thingd stream.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEvent {
    /// Stream name, such as `project:thingd` or `customer:cus_123`.
    pub stream: String,
    /// Event kind, such as `decision.made`.
    pub event_type: String,
    /// Serialized event body.
    pub body: String,
    /// Monotonic sequence assigned by the event log.
    pub sequence: u64,
    /// ISO 8601 creation timestamp. Empty if not set.
    pub created_at: String,
    /// Optional idempotency key for deduplication on retry.
    /// When set, appending an event with the same (stream, `idempotency_key`)
    /// pair returns the existing event instead of creating a duplicate.
    pub idempotency_key: String,
}

impl MemoryEvent {
    /// Create a new event record.
    pub fn new(
        stream: impl Into<String>,
        event_type: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            stream: stream.into(),
            event_type: event_type.into(),
            body: body.into(),
            sequence: 0,
            created_at: String::new(),
            idempotency_key: String::new(),
        }
    }
}

/// Queue job lifecycle state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QueueJobStatus {
    /// Ready to be claimed by a worker.
    Ready,
    /// Claimed by a worker and awaiting ack/nack.
    Leased,
    /// Completed successfully.
    Completed,
    /// Exhausted retries and moved to the dead-letter set.
    Dead,
}

/// A queued unit of work.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueJob {
    /// Queue name.
    pub queue: String,
    /// Stable job identifier.
    pub id: String,
    /// Serialized job payload.
    pub body: String,
    /// Number of attempts already made.
    pub attempts: u32,
    /// Maximum attempts before the job should be considered dead.
    pub max_attempts: u32,
    /// Current job status.
    pub status: QueueJobStatus,
    /// Unix timestamp in milliseconds when this job becomes claimable.
    pub available_at_ms: i64,
    /// Unix timestamp in milliseconds when this job was leased.
    pub leased_at_ms: Option<i64>,
    /// Unix timestamp in milliseconds when this job lease expires.
    pub lease_expires_at_ms: Option<i64>,
    /// Unix timestamp in milliseconds when this job completed.
    pub completed_at_ms: Option<i64>,
    /// Unix timestamp in milliseconds when this job moved to dead-letter state.
    pub dead_at_ms: Option<i64>,
    /// ISO 8601 creation timestamp. Empty if not set.
    pub created_at: String,
    /// Error message from last nack. Empty if not set.
    pub last_error: String,
    /// Priority for claim ordering (higher = claimed sooner). Default: 0.
    pub priority: i32,
}

impl QueueJob {
    /// Create a new ready job.
    pub fn new(
        queue: impl Into<String>,
        id: impl Into<String>,
        body: impl Into<String>,
        max_attempts: u32,
    ) -> Self {
        Self {
            queue: queue.into(),
            id: id.into(),
            body: body.into(),
            attempts: 0,
            max_attempts,
            status: QueueJobStatus::Ready,
            available_at_ms: 0,
            leased_at_ms: None,
            lease_expires_at_ms: None,
            completed_at_ms: None,
            dead_at_ms: None,
            created_at: String::new(),
            last_error: String::new(),
            priority: 0,
        }
    }

    /// Set the priority for this job (higher = claimed sooner).
    #[must_use]
    pub const fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Make this job available after a delay.
    #[must_use]
    pub fn delay_by_ms(mut self, delay_ms: u64) -> Self {
        self.available_at_ms = unix_timestamp_millis().saturating_add(u64_to_i64(delay_ms));
        self
    }

    /// Set the exact Unix timestamp in milliseconds when this job is claimable.
    #[must_use]
    pub const fn available_at_ms(mut self, available_at_ms: i64) -> Self {
        self.available_at_ms = available_at_ms;
        self
    }
}

/// Options for listing events.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListEventsOptions {
    /// Only return events with sequence greater than this value.
    pub from_sequence: Option<u64>,
    /// Maximum number of events to return.
    pub limit: Option<u64>,
    /// Only return events created at or after this ISO 8601 timestamp.
    pub since: Option<String>,
}

/// Sort direction for list queries.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortDirection {
    /// Ascending order (A→Z, oldest→newest, smallest→largest).
    #[default]
    Asc,
    /// Descending order (Z→A, newest→oldest, largest→smallest).
    Desc,
}

/// Sort specification for list queries.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortBy {
    /// Field name: `id`, `collection`, `created_at`, `updated_at`, `version`.
    pub field: String,
    /// Sort direction.
    pub direction: SortDirection,
}

impl SortBy {
    /// Create ascending sort by field name.
    pub fn asc(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            direction: SortDirection::Asc,
        }
    }

    /// Create descending sort by field name.
    pub fn desc(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            direction: SortDirection::Desc,
        }
    }
}

/// Options for listing objects in a collection.
#[derive(Clone, Debug, Default)]
pub struct ListObjectsOptions {
    /// Filter key-value pairs serialised as JSON pairs: only objects whose body
    /// contains every listed top-level key with the exact JSON value are returned.
    /// Each string is `"key":<json-value>` without surrounding braces.
    pub filter: Vec<(String, serde_json::Value)>,
    /// Sort specification. Default is insertion order.
    pub sort_by: Option<SortBy>,
    /// Maximum number of objects to return.
    pub limit: Option<u64>,
    /// Number of objects to skip before returning results.
    pub offset: Option<u64>,
}

/// Options for putting an object.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutObjectOptions {
    /// Whether to update the FTS search index. Default: `true`.
    /// Set to `false` when only metadata changes (e.g. timestamp dedup)
    /// and the body text is identical — skips FTS DELETE + INSERT.
    pub index: bool,
    /// Optional expected version for optimistic locking (CAS).
    /// When `Some(v)`, the put succeeds only if the current version
    /// equals `v`. If the object does not exist, returns `Conflict`.
    /// When `None`, no version check is performed (default).
    pub expected_version: Option<u64>,
}

impl Default for PutObjectOptions {
    fn default() -> Self {
        Self {
            index: true,
            expected_version: None,
        }
    }
}

/// Options used when claiming a queue job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueClaimOptions {
    /// Lease duration in milliseconds.
    pub lease_ms: u64,
}

impl Default for QueueClaimOptions {
    fn default() -> Self {
        Self {
            lease_ms: DEFAULT_QUEUE_LEASE_MS,
        }
    }
}

impl QueueClaimOptions {
    /// Create queue claim options with the given lease duration.
    #[must_use]
    pub const fn new(lease_ms: u64) -> Self {
        Self { lease_ms }
    }
}

/// Options used when rejecting a leased queue job.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct QueueNackOptions {
    /// Delay before a retry can be claimed.
    pub delay_ms: u64,
    /// Error message from the worker, stored as `last_error` on the job.
    pub error: String,
}

impl QueueNackOptions {
    /// Create queue nack options with the given retry delay.
    #[must_use]
    pub const fn new(delay_ms: u64) -> Self {
        Self {
            delay_ms,
            error: String::new(),
        }
    }

    /// Create queue nack options with retry delay and an error message.
    #[must_use]
    pub fn with_error(delay_ms: u64, error: impl Into<String>) -> Self {
        Self {
            delay_ms,
            error: error.into(),
        }
    }
}

/// Options used when performing a search.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct SearchOptions {
    /// Limit search to these collection or stream names.
    pub collections: Option<Vec<String>>,
    /// Maximum number of hits to return.
    pub limit: Option<usize>,
    /// Metadata filters to match custom fields in the JSON body.
    pub filter: Option<serde_json::Value>,
}

/// A single match returned by a search query.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    /// Result kind: "object" or "event".
    pub kind: String,
    /// Collection or stream name.
    pub collection: String,
    /// Object id or event sequence number.
    pub id: String,
    /// The indexed text that matched.
    pub text: String,
    /// Relevancy score.
    pub score: f64,
    /// The serialized body.
    pub body: String,
    /// Object version (only populated for objects).
    pub version: Option<u64>,
    /// Created timestamp.
    pub created_at: String,
    /// Updated timestamp (only populated for objects).
    pub updated_at: Option<String>,
    /// Event type (only populated for events).
    pub event_type: Option<String>,
}

/// A single match returned by a vector search query.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorSearchHit {
    /// Object id.
    pub id: String,
    /// Cosine similarity score (-1.0 to 1.0).
    pub score: f64,
    /// The full stored object.
    pub value: MemoryObject,
}

/// Options for vector search.
#[derive(Clone, Debug, Default)]
pub struct VectorSearchOptions {
    /// Maximum number of results to return (default: all matching).
    pub top_k: Option<usize>,
    /// Metadata filter: only objects whose body matches these fields are returned.
    pub filter: Option<serde_json::Value>,
}

/// A graph link connecting two references.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    /// Unique link identifier.
    pub id: String,
    /// Source reference (e.g. "collection/id" or "stream/sequence").
    pub from_ref: String,
    /// Relationship type (e.g. "supports", "`depends_on`", "`chunk_of`").
    pub link_type: String,
    /// Target reference.
    pub to_ref: String,
    /// Optional weight for ranking (0.0 to 1.0).
    pub weight: Option<f64>,
    /// Optional metadata as JSON string.
    pub metadata_json: String,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
}

impl Link {
    /// Create a new graph link.
    pub fn new(
        from_ref: impl Into<String>,
        link_type: impl Into<String>,
        to_ref: impl Into<String>,
    ) -> Self {
        Self {
            id: String::new(),
            from_ref: from_ref.into(),
            link_type: link_type.into(),
            to_ref: to_ref.into(),
            weight: None,
            metadata_json: "{}".to_string(),
            created_at: String::new(),
        }
    }

    /// Set the link weight.
    #[must_use]
    pub const fn with_weight(mut self, weight: f64) -> Self {
        self.weight = Some(weight);
        self
    }

    /// Set the metadata JSON.
    #[must_use]
    pub fn with_metadata(mut self, metadata: impl Into<String>) -> Self {
        self.metadata_json = metadata.into();
        self
    }
}

/// Options for querying graph links.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkQueryOptions {
    /// Filter by relationship type.
    pub link_type: Option<String>,
    /// Maximum number of results.
    pub limit: Option<usize>,
}

/// Direction for neighbor queries.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkDirection {
    /// Only outgoing links (`from_ref` matches).
    Outgoing,
    /// Only incoming links (`to_ref` matches).
    Incoming,
    /// Both directions.
    #[default]
    Both,
}

/// Aggregation function to apply.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AggregateFunction {
    /// Count objects (field ignored).
    #[default]
    Count,
    /// Sum of numeric field values.
    Sum,
    /// Average of numeric field values.
    Avg,
    /// Minimum of field values.
    Min,
    /// Maximum of field values.
    Max,
}

impl AggregateFunction {
    /// Return the SQL function name for this aggregate.
    pub const fn sql_func(&self) -> &str {
        match self {
            Self::Count => "COUNT(*)",
            Self::Sum => "SUM",
            Self::Avg => "AVG",
            Self::Min => "MIN",
            Self::Max => "MAX",
        }
    }
}

/// Options for a general aggregation query.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateOptions {
    /// Filter key-value pairs: only matching objects are aggregated.
    pub filter: Vec<(String, serde_json::Value)>,
    /// Group results by this top-level body field.
    pub group_by: Option<String>,
    /// Aggregation function to apply.
    pub function: AggregateFunction,
    /// Field to aggregate (required for sum/avg/min/max, ignored for count).
    pub field: Option<String>,
}

/// Result of an aggregation query.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateResult {
    /// Total across all groups (or the single result if no `group_by`).
    pub total: f64,
    /// Per-group results (empty if no `group_by`).
    pub groups: Vec<AggregateGroupResult>,
}

/// A single group result from aggregation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateGroupResult {
    /// Group key (the field value).
    pub key: String,
    /// Aggregated value for this group.
    pub value: f64,
}

/// Time bucket size for time-series aggregation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimeBucket {
    /// Group by hour.
    Hour,
    /// Group by day.
    #[default]
    Day,
    /// Group by week.
    Week,
    /// Group by month.
    Month,
}

impl TimeBucket {
    /// Return the `SQLite` strftime format for this bucket.
    pub const fn strftime_format(&self) -> &str {
        match self {
            Self::Hour => "%Y-%m-%dT%H:00:00Z",
            Self::Day => "%Y-%m-%d",
            Self::Week => "%Y-W%W",
            Self::Month => "%Y-%m",
        }
    }
}

/// Options for a time-series aggregation query.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeSeriesOptions {
    /// Filter key-value pairs: only matching objects are aggregated.
    pub filter: Vec<(String, serde_json::Value)>,
    /// Time bucket size.
    pub bucket: TimeBucket,
    /// Aggregation function to apply.
    pub function: AggregateFunction,
    /// Field to aggregate (ignored for count).
    pub field: Option<String>,
    /// Start of time range (ISO 8601).
    pub from: Option<String>,
    /// End of time range (ISO 8601).
    pub to: Option<String>,
}

/// Result of a time-series aggregation query.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeSeriesResult {
    /// Ordered time buckets.
    pub buckets: Vec<TimeSeriesBucket>,
}

/// A single time bucket from time-series aggregation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeSeriesBucket {
    /// Bucket label (ISO 8601 truncated to bucket granularity).
    pub label: String,
    /// Aggregated value for this bucket.
    pub value: f64,
}

/// Inferred field metadata for a collection.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldSchema {
    /// Field name.
    pub name: String,
    /// Inferred data type: `"string"`, `"number"`, `"boolean"`, `"date"`, `"null"`, or `"unknown"`.
    pub field_type: String,
    /// Whether the field is absent or null in sampled objects.
    pub nullable: bool,
    /// Example values from sampled objects (may be empty).
    pub sample_values: Vec<serde_json::Value>,
}

/// Reflected schema for a collection.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSchema {
    /// Collection name.
    pub name: String,
    /// Total number of objects in the collection.
    pub object_count: u64,
    /// Inferred fields from sampled objects.
    pub fields: Vec<FieldSchema>,
}

/// Options for schema reflection.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaOptions {
    /// Number of objects to sample for type inference (default 50).
    pub sample_size: Option<usize>,
}

/// Persisted canonical schema metadata.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredSchema {
    /// Canonical schema JSON.
    pub schema_json: String,
    /// Stable schema hash.
    pub hash: String,
    /// Last update timestamp.
    pub updated_at: String,
}

/// A durable record of an applied schema migration.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationRecord {
    /// Migration identifier, normally the numbered filename.
    pub id: String,
    /// Schema hash applied by this migration.
    pub hash: String,
    /// Application timestamp.
    pub applied_at: String,
}

/// A functional index definition for a top-level JSON field.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDefinition {
    /// Collection containing the indexed objects.
    pub collection: String,
    /// Top-level JSON field covered by the index.
    pub field: String,
    /// Whether duplicate non-null values are rejected on writes.
    #[serde(default)]
    pub unique: bool,
}

impl Default for SchemaOptions {
    fn default() -> Self {
        Self {
            sample_size: Some(50),
        }
    }
}
