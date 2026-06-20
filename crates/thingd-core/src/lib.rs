//! Core primitives for thingd.
//!
//! This crate owns the durable engine boundary: object storage, append-only
//! events, and queue storage. The default implementation is in-memory, with a
//! feature-gated SQLite adapter available for durable object, event, and
//! queue storage.
//!
//! # Feature Flags
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `sqlite` | No | Enables [`SqliteThingStore`] with FTS5 search, WAL mode, and auto-migration |
//! | `connectors` | No | Enables CSV/JSON file connectors for data import |
//!
//! # Example (in-memory)
//!
//! ```rust
//! use thingd_core::{MemoryEngine, ObjectStore, EventLog, MemoryObject, MemoryEvent};
//!
//! let mut engine = MemoryEngine::new();
//!
//! let obj = MemoryObject::new("users", "alice", r#"{"name":"Alice"}"#);
//! engine.put_object(obj).unwrap();
//!
//! let user = engine.get_object("users", "alice").unwrap();
//! assert_eq!(user.unwrap().body, r#"{"name":"Alice"}"#);
//!
//! let event = MemoryEvent::new("audit", "user.created", r#"{"user":"alice"}"#);
//! engine.append_event(event).unwrap();
//! ```
//!
//! # Example (SQLite — requires `sqlite` feature)
//!
//! ```rust,no_run
//! #[cfg(feature = "sqlite")]
//! {
//!     use thingd_core::{SqliteThingStore, ObjectStore, MemoryObject};
//!
//!     let mut db = SqliteThingStore::open_in_memory().unwrap();
//!     db.put_object(MemoryObject::new("users", "alice", r#"{"name":"Alice"}"#)).unwrap();
//!     let user = db.get_object("users", "alice").unwrap();
//!     assert_eq!(user.unwrap().body, r#"{"name":"Alice"}"#);
//! }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "connectors")]
pub mod connector;
mod error;
mod in_memory;
mod model;
#[cfg(feature = "sqlite")]
mod sqlite;
mod store;

#[cfg(feature = "connectors")]
pub use connector::{
    Column, ColumnType, Connector, ConnectorConfig, FileConnector, Schema, SyncStrategy,
};
pub use error::{ThingdError, ThingdResult};
pub use in_memory::MemoryEngine;
pub use model::{
    DEFAULT_QUEUE_LEASE_MS, Link, LinkDirection, LinkQueryOptions, ListEventsOptions,
    ListObjectsOptions, MemoryEvent, MemoryObject, ObjectKey, PutObjectOptions, QueueClaimOptions,
    QueueJob, QueueJobStatus, QueueNackOptions, SearchHit, SearchOptions, SortBy, SortDirection,
};
#[cfg(feature = "sqlite")]
#[cfg_attr(docsrs, doc(cfg(feature = "sqlite")))]
pub use sqlite::{SQLITE_SCHEMA_VERSION, SqliteThingStore};
pub use store::{EventLog, LinkStore, ObjectStore, QueueStore, Searcher, ThingStore};

pub(crate) fn unix_timestamp_millis() -> i64 {
    let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return 0;
    };

    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

pub(crate) fn u64_to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

pub(crate) fn now_iso_string() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
