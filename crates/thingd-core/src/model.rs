//! Data model types shared by storage adapters.

use crate::{u64_to_i64, unix_timestamp_millis};

/// Default queue lease duration in milliseconds.
pub const DEFAULT_QUEUE_LEASE_MS: u64 = 30_000;

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

/// An object stored in a thingd collection.
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
    pub fn new(
        collection: impl Into<String>,
        id: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            key: ObjectKey::new(collection, id),
            body: body.into(),
            version: 0,
        }
    }
}

/// An append-only event stored in a thingd stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryEvent {
    /// Stream name, such as `project:thingd` or `customer:cus_123`.
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub struct QueueNackOptions {
    /// Delay before a retry can be claimed.
    pub delay_ms: u64,
}

impl QueueNackOptions {
    /// Create queue nack options with the given retry delay.
    #[must_use]
    pub const fn new(delay_ms: u64) -> Self {
        Self { delay_ms }
    }
}
