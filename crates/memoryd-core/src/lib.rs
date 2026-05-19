//! Core primitives for memoryd.
//!
//! This crate owns the durable engine boundary: object storage, append-only
//! events, and queue storage. The default implementation is in-memory, with a
//! feature-gated SQLite adapter available for durable object and event storage.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
mod in_memory;
mod model;
#[cfg(feature = "sqlite")]
mod sqlite;
mod store;

pub use error::{MemorydError, MemorydResult};
pub use in_memory::MemoryEngine;
pub use model::{MemoryEvent, MemoryObject, ObjectKey, QueueJob, QueueJobStatus};
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteMemoryStore;
pub use store::{EventLog, MemoryStore, ObjectStore, QueueStore};
