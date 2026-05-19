//! Core primitives for memoryd.
//!
//! This crate owns the durable engine boundary: object storage, append-only
//! events, and queue storage. The current implementation is in-memory so the
//! API can settle before a persistent storage adapter is introduced.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
mod in_memory;
mod model;
mod store;

pub use error::{MemorydError, MemorydResult};
pub use in_memory::MemoryEngine;
pub use model::{MemoryEvent, MemoryObject, ObjectKey, QueueJob, QueueJobStatus};
pub use store::{EventLog, MemoryStore, ObjectStore, QueueStore};
