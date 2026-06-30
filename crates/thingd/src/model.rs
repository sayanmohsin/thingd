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
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
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
        }
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
        }
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
