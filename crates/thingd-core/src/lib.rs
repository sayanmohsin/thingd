//! Core primitives for thingd.
//!
//! This crate owns the durable engine boundary: object storage, append-only
//! events, and queue storage. The default implementation is in-memory, with a
//! feature-gated `SQLite` adapter available for durable object, event, and
//! queue storage.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::time::{SystemTime, UNIX_EPOCH};

mod error;
mod in_memory;
mod model;
#[cfg(feature = "sqlite")]
mod sqlite;
mod store;

pub use error::{ThingdError, ThingdResult};
pub use in_memory::MemoryEngine;
pub use model::{
    MemoryEvent, MemoryObject, ObjectKey, QueueClaimOptions, QueueJob, QueueJobStatus,
    QueueNackOptions, SearchHit, SearchOptions, DEFAULT_QUEUE_LEASE_MS,
};
#[cfg(feature = "sqlite")]
pub use sqlite::{SqliteThingStore, SQLITE_SCHEMA_VERSION};
pub use store::{EventLog, ObjectStore, QueueStore, ThingStore, Searcher};

pub(crate) fn unix_timestamp_millis() -> i64 {
    let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return 0;
    };

    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

pub(crate) fn u64_to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}
