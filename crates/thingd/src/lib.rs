//! Core primitives for thingd.
//!
//! This crate owns the durable engine boundary: object storage, append-only
//! events, and queue storage. The default implementation is in-memory, with a
//! feature-gated persistent storage adapter.
//!
//! # Feature Flags
//!
//! | Feature | Default | Description |
#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::option_if_let_else,
    clippy::manual_let_else
)]
//! |---------|---------|-------------|
//! | `persistent` | Yes | Enables [`PersistentEngine`] — durable local storage |
//! | `connectors` | No | Enables CSV/JSON file connectors for data import |
//!
//! # Example (in-memory)
//!
//! ```rust
//! use thingd::{MemoryEngine, ObjectStore, EventLog, MemoryObject, MemoryEvent};
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
//! # Example (persistent storage)
//!
//! ```rust,no_run
//! use thingd::{PersistentEngine, ObjectStore, MemoryObject};
//!
//! let mut db = PersistentEngine::open("/tmp/thingd-data").unwrap();
//! db.put_object(MemoryObject::new("users", "alice", r#"{"name":"Alice"}"#)).unwrap();
//! let user = db.get_object("users", "alice").unwrap();
//! assert_eq!(user.unwrap().body, r#"{"name":"Alice"}"#);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "connectors")]
pub mod connector;
#[cfg(feature = "connectors")]
pub mod connectors;
#[cfg(feature = "persistent")]
mod encryption;
mod error;
mod in_memory;
mod model;
#[cfg(feature = "persistent")]
mod persistent;
mod store;

#[cfg(feature = "connectors")]
pub use connector::{
    Column, ColumnType, Connector, ConnectorAuth, ConnectorConfig, FileConnector, PullStream,
    Schema, SslMode, SyncStrategy,
};
#[cfg(feature = "connectors")]
pub use connectors::{MysqlConnector, PostgresConnector};
#[cfg(feature = "persistent")]
pub use encryption::{EncryptionConfig, KeyProvider, StaticKeyProvider};
pub use error::{ThingdError, ThingdResult};
pub use in_memory::MemoryEngine;
pub use model::{
    AggregateFunction, AggregateGroupResult, AggregateOptions, AggregateResult, CollectionSchema,
    DEFAULT_QUEUE_LEASE_MS, FieldSchema, Link, LinkDirection, LinkQueryOptions, ListEventsOptions,
    ListObjectsOptions, MemoryEvent, MemoryObject, ObjectKey, PutObjectOptions, QueueClaimOptions,
    QueueJob, QueueJobStatus, QueueNackOptions, SchemaOptions, SearchHit, SearchOptions, SortBy,
    SortDirection, TimeBucket, TimeSeriesBucket, TimeSeriesOptions, TimeSeriesResult,
    VectorSearchHit, VectorSearchOptions,
};
#[cfg(feature = "persistent")]
#[cfg_attr(docsrs, doc(cfg(feature = "persistent")))]
pub use persistent::{PersistentEngine, PersistentOpenOptions};
pub use store::{
    AggregateStore, EventLog, LinkStore, ObjectStore, QueueStore, Searcher, ThingStore, VectorStore,
};

/// Shared contract tests for all engine backends.
#[cfg(test)]
pub(crate) mod contract_tests;

pub(crate) fn unix_timestamp_millis() -> i64 {
    let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        #[cfg(debug_assertions)]
        panic!("SystemTime::now is before UNIX epoch — clock is broken");
        #[cfg(not(debug_assertions))]
        {
            return 0;
        }
    };

    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

pub(crate) fn u64_to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

pub(crate) fn now_iso_string() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Compute cosine similarity between two vectors.
/// Returns a value in [-1.0, 1.0], or 0.0 if either vector is zero.
pub(crate) fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    f64::from(dot / (norm_a * norm_b))
}
