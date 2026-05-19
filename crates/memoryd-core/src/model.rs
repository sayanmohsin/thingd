//! Data model types shared by storage adapters.

/// Stable object key inside a collection.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
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

/// An object stored in a memoryd collection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryObject {
    /// Stable object key.
    pub key: ObjectKey,
    /// Serialized object body.
    pub body: String,
    /// Monotonic object version assigned by the store.
    pub version: u64,
}

impl MemoryObject {
    /// Create a new object record.
    pub fn new(collection: impl Into<String>, id: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            key: ObjectKey::new(collection, id),
            body: body.into(),
            version: 0,
        }
    }
}

/// An append-only event stored in a memoryd stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryEvent {
    /// Stream name, such as `project:memoryd` or `customer:cus_123`.
    pub stream: String,
    /// Event kind, such as `decision.made`.
    pub event_type: String,
    /// Serialized event body.
    pub body: String,
    /// Monotonic sequence assigned by the event log.
    pub sequence: u64,
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
        }
    }
}

/// Queue job lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
#[derive(Clone, Debug, Eq, PartialEq)]
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
        }
    }
}
