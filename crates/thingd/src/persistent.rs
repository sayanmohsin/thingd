#![allow(
    clippy::branches_sharing_code,
    clippy::missing_errors_doc,
    clippy::match_wildcard_for_single_variants,
    clippy::assigning_clones,
    clippy::needless_pass_by_value,
    clippy::doc_markdown
)]

use std::collections::HashMap;
use std::ops::Bound::{Excluded, Unbounded};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::encryption::{EncryptionConfig, StorageCodec, StorageCrypto, make_codec};
use crate::error::ThingdResult;
use crate::model::{
    AggregateFunction, AggregateGroupResult, AggregateOptions, AggregateResult, IndexDefinition,
    LinkDirection, LinkQueryOptions, ListEventsOptions, ListObjectsOptions, MigrationRecord,
    ObjectKey, PutObjectOptions, SchemaOptions, SearchOptions, SortDirection, StoredSchema,
    TimeBucket, TimeSeriesBucket, TimeSeriesOptions, TimeSeriesResult,
};
use crate::replication::{REPLICATION_STATE_COLLECTION, REPLICATION_STREAM};
use crate::storage_backend::{
    Database, Keyspace, KeyspaceCreateOptions, PersistMode, StorageBackend,
};
use crate::storage_backend::{Guard, Slice};
use crate::store::{
    AggregateStore, EventLog, LinkStore, ObjectStore, QueueStore, RetentionOptions,
    RetentionReport, Searcher, StorageMaintenanceStatus,
};
use crate::{
    CollectionSchema, FieldSchema, Link, MemoryEvent, MemoryObject, QueueClaimOptions, QueueJob,
    QueueJobStatus, QueueNackOptions, SearchHit, ThingdError, VectorSearchHit, VectorSearchOptions,
};
use crate::{now_iso_string, unix_timestamp_millis};

/// Persistent storage engine implementing all 6 storage traits.
///
/// Data directory layout:
/// - `objects`: `{collection}\0{id}` → serialized `MemoryObject`
/// - `events`: `{stream}\0{seq:8BE}` → serialized `MemoryEvent`
/// - `queue_jobs`: `{queue}\0{id}` → serialized `QueueJob`
/// - `links_by_id`: `{link_id}` → serialized `Link`
/// - `links_from`: `{from_ref}\0{type}\0{link_id}` → `()`
/// - `links_to`: `{to_ref}\0{type}\0{link_id}` → `()`
pub struct PersistentEngine {
    #[allow(dead_code)]
    db: Database,
    path: PathBuf,
    objects: Keyspace,
    events: Keyspace,
    event_meta: Keyspace,
    queue_jobs: Keyspace,
    ready_jobs: Keyspace,
    lease_jobs: Keyspace,
    links_by_id: Keyspace,
    links_from: Keyspace,
    links_to: Keyspace,
    schemas: Keyspace,
    migrations: Keyspace,
    indexes: Keyspace,
    next_link_id: AtomicU64,
    event_seq_counters: HashMap<String, u64>,
    event_idempotency_keys: HashMap<(String, String), u64>,
    unique_index_values: HashMap<(String, String, String), ObjectKey>,
    unique_index_cache_complete: bool,
    #[cfg(feature = "search")]
    search_index: Option<tantivy::Index>,
    #[cfg(feature = "search")]
    search_writer: Option<Arc<Mutex<tantivy::IndexWriter<tantivy::TantivyDocument>>>>,
    #[cfg(feature = "search")]
    search_reader: Option<Arc<Mutex<tantivy::IndexReader>>>,
    #[cfg(feature = "search")]
    search_mode: PersistentSearchMode,
    #[cfg(feature = "search")]
    search_queue: Option<Arc<SearchMutationQueue>>,
    #[cfg(feature = "search")]
    search_commit_interval_ms: u64,
    #[cfg(feature = "search")]
    search_commit_batch_size: usize,
    #[cfg(feature = "search")]
    search_queue_max_keys: usize,
    #[cfg(feature = "search")]
    search_worker_started: bool,
    #[cfg(feature = "search")]
    search_worker: Option<thread::JoinHandle<()>>,
    #[cfg(feature = "search")]
    search_rebuild: Option<SearchRebuildProgress>,
    #[cfg(feature = "search")]
    search_rebuild_required: bool,
    #[cfg(feature = "search")]
    search_rebuild_path: Option<PathBuf>,
    maintenance: StorageMaintenanceStatus,
    queue_diagnostics: QueueDiagnostics,
    recovery_batch_size: usize,
    recovery_pause_ms: u64,
    recovery_max_retries: u64,
    recovery_memory_limit_bytes: Option<u64>,
    max_journal_bytes: u64,
    #[cfg(feature = "vectors")]
    vectors: Keyspace,
    codec: Box<dyn StorageCodec>,
}

impl Drop for PersistentEngine {
    fn drop(&mut self) {
        #[cfg(feature = "search")]
        self.stop_search_worker();
    }
}

const STORAGE_FORMAT_VERSION: u32 = 1;
const STORAGE_CONTRACT: &str = "rocksdb-tantivy-v1";
const STORAGE_MANIFEST_FILE: &str = ".thingd-storage.json";
const STORAGE_LOCK_FILE: &str = "lock";
const STORAGE_KEYSPACES_DIR: &str = "keyspaces";
// Tantivy requires a minimum memory budget per writer thread; keep this below
// the previous allocation while remaining valid for the current release.
const SEARCH_WRITER_MEMORY_BYTES: usize = 15_000_000;
const DEFAULT_MAX_JOURNAL_BYTES: u64 = 32 * 1024 * 1024;
const MAX_SEARCH_REBUILD_RETRIES: u64 = 3;
const DEFAULT_SEARCH_COMMIT_INTERVAL_MS: u64 = 250;
const DEFAULT_SEARCH_COMMIT_BATCH_SIZE: usize = 32;
const DEFAULT_SEARCH_QUEUE_MAX_KEYS: usize = 10_000;
const LEASE_KEYSPACE: &str = "lease_jobs";
type UniqueIndexCache = HashMap<(String, String, String), ObjectKey>;

const REQUIRED_KEYSPACES: &[&str] = &[
    "objects",
    "events",
    "queue_jobs",
    "ready_jobs",
    "links_by_id",
    "links_from",
    "links_to",
    "schemas",
    "migrations",
    "indexes",
];

#[cfg(feature = "vectors")]
const REQUIRED_VECTOR_KEYSPACE: &str = "vectors";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StorageManifest {
    format_version: u32,
    contract: String,
    keyspaces: Vec<String>,
    search_schema_version: u32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct EventMetadata {
    stream: String,
    max_sequence: u64,
    idempotency_keys: HashMap<String, u64>,
}

#[cfg(feature = "search")]
#[derive(Debug, Clone)]
enum SearchIndexMutation {
    UpsertObject(MemoryObject),
    DeleteObject { collection: String, id: String },
    UpsertEvent(MemoryEvent),
    DeleteEvent { stream: String, sequence: u64 },
}

#[cfg(feature = "search")]
#[derive(Debug, Default)]
struct SearchQueueState {
    pending: HashMap<String, SearchIndexMutation>,
    in_flight: bool,
    queued: u64,
    coalesced: u64,
    committed: u64,
    last_commit_unix_ms: Option<i64>,
    last_commit_duration_ms: Option<u64>,
    last_error: Option<String>,
    retry_count: u64,
    stale: bool,
}

#[cfg(feature = "search")]
struct SearchMutationQueue {
    state: Mutex<SearchQueueState>,
    wake: Condvar,
    max_keys: usize,
    shutdown: AtomicBool,
}

#[cfg(feature = "search")]
impl SearchMutationQueue {
    fn new(max_keys: usize) -> Self {
        Self {
            state: Mutex::new(SearchQueueState::default()),
            wake: Condvar::new(),
            max_keys: max_keys.max(1),
            shutdown: AtomicBool::new(false),
        }
    }

    fn enqueue(&self, key: String, mutation: SearchIndexMutation) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        state.queued = state.queued.saturating_add(1);
        let accepted = if state.pending.contains_key(&key) {
            state.coalesced = state.coalesced.saturating_add(1);
            state.pending.insert(key, mutation);
            true
        } else if state.pending.len() >= self.max_keys {
            // The primary write has already committed. Mark the derived index
            // stale and rebuild it from primary storage instead of growing the
            // queue without bound or rejecting durable data.
            state.pending.clear();
            state.stale = true;
            state.last_error = Some("search mutation queue capacity reached".to_string());
            false
        } else {
            state.pending.insert(key, mutation);
            true
        };
        self.wake.notify_one();
        accepted
    }

    fn take_batch(&self, batch_size: usize, interval: Duration) -> Vec<SearchIndexMutation> {
        let Ok(mut state) = self.state.lock() else {
            return Vec::new();
        };
        if state.stale {
            return Vec::new();
        }
        while state.pending.is_empty() {
            if self.shutdown.load(Ordering::Acquire) {
                return Vec::new();
            }
            let Ok((next, _)) = self.wake.wait_timeout(state, interval) else {
                return Vec::new();
            };
            state = next;
            if self.shutdown.load(Ordering::Acquire) {
                return Vec::new();
            }
            if state.stale {
                return Vec::new();
            }
            if state.pending.is_empty() {
                return Vec::new();
            }
        }
        if state.pending.len() < batch_size.max(1) {
            if self.shutdown.load(Ordering::Acquire) {
                let keys = state
                    .pending
                    .keys()
                    .take(batch_size.max(1))
                    .cloned()
                    .collect::<Vec<_>>();
                state.in_flight = !keys.is_empty();
                return keys
                    .into_iter()
                    .filter_map(|key| state.pending.remove(&key))
                    .collect();
            }
            let Ok((next, _)) = self.wake.wait_timeout(state, interval) else {
                return Vec::new();
            };
            state = next;
        }
        let keys = state
            .pending
            .keys()
            .take(batch_size.max(1))
            .cloned()
            .collect::<Vec<_>>();
        state.in_flight = !keys.is_empty();
        keys.into_iter()
            .filter_map(|key| state.pending.remove(&key))
            .collect()
    }

    fn record_commit(&self, duration: Duration, count: usize) {
        if let Ok(mut state) = self.state.lock() {
            state.in_flight = false;
            state.committed = state.committed.saturating_add(count as u64);
            state.last_commit_unix_ms = Some(unix_timestamp_millis());
            state.last_commit_duration_ms =
                Some(u64::try_from(duration.as_millis()).unwrap_or(u64::MAX));
            state.last_error = None;
            state.retry_count = 0;
        }
    }

    fn record_error(&self, error: String) {
        if let Ok(mut state) = self.state.lock() {
            state.in_flight = false;
            state.last_error = Some(error);
            state.retry_count = state.retry_count.saturating_add(1);
            state.stale = true;
        }
    }

    fn snapshot(&self) -> SearchQueueState {
        self.state
            .lock()
            .map(|state| SearchQueueState {
                pending: HashMap::new(),
                in_flight: state.in_flight,
                queued: state.queued,
                coalesced: state.coalesced,
                committed: state.committed,
                last_commit_unix_ms: state.last_commit_unix_ms,
                last_commit_duration_ms: state.last_commit_duration_ms,
                last_error: state.last_error.clone(),
                retry_count: state.retry_count,
                stale: state.stale,
            })
            .unwrap_or_default()
    }

    fn depth(&self) -> usize {
        self.state
            .lock()
            .map(|state| state.pending.len())
            .unwrap_or_default()
    }

    fn needs_fallback(&self) -> bool {
        self.state.lock().map_or(true, |state| {
            state.in_flight || !state.pending.is_empty() || state.stale
        })
    }

    fn is_stale(&self) -> bool {
        self.state.lock().map_or(true, |state| state.stale)
    }

    fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        self.wake.notify_all();
    }

    fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }
}

/// Search index behavior for a persistent engine.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub enum PersistentSearchMode {
    /// Open the persistent Tantivy index and rebuild it when necessary.
    #[default]
    Persistent,
    /// Open the primary store immediately and rebuild a missing or incompatible
    /// search index incrementally through the server's background maintenance loop.
    PersistentAsync,
    /// Open only primary storage, then rebuild search after startup maintenance.
    PersistentRecovery,
    /// Open a compatible Tantivy index but do not create or rebuild one.
    PersistentNoRebuild,
    /// Do not open or rebuild Tantivy. Search uses the bounded fallback scan.
    Disabled,
}

/// Current state of an asynchronous persistent search-index rebuild.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRebuildStatus {
    /// `rebuilding`, `ready`, or `failed`.
    pub state: String,
    /// Number of primary records incorporated into the rebuild.
    pub processed: u64,
    /// Number of primary records observed when the rebuild started.
    pub total: u64,
    /// Rebuild generation identifier.
    pub generation: u64,
    /// Last rebuild error, when state is `failed`.
    pub error: Option<String>,
}

#[cfg(feature = "search")]
#[derive(Debug)]
enum SearchRebuildPhase {
    Objects,
    Events,
    Replay,
}

#[cfg(feature = "search")]
#[derive(Debug)]
struct SearchRebuildProgress {
    generation: u64,
    phase: SearchRebuildPhase,
    object_cursor: Option<Vec<u8>>,
    event_cursor: Option<Vec<u8>>,
    processed: u64,
    total: u64,
    replay: HashMap<String, SearchReplayMutation>,
    replay_overflow: bool,
    error: Option<String>,
}

#[cfg(feature = "search")]
#[derive(Debug, Clone, Copy)]
enum SearchReplayMutation {
    UpsertObject,
    DeleteObject,
    UpsertEvent,
    DeleteEvent,
}

#[cfg(feature = "search")]
const SEARCH_REBUILD_REPLAY_LIMIT: usize = 100_000;

/// Result of validating a native RocksDB storage directory without opening it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageValidationReport {
    /// Current Thingd storage format version.
    pub format_version: u32,
    /// Whether the directory predates the Thingd manifest and can be upgraded on open.
    pub legacy_manifest: bool,
    /// Whether the expected lock file is present.
    pub lock_present: bool,
    /// Whether the expected keyspace directory is present.
    pub keyspaces_present: bool,
    /// Whether the existing Tantivy schema is compatible, when an index exists.
    pub search_index_compatible: Option<bool>,
}

/// Bounded queue-index diagnostics for a persistent engine.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueDiagnostics {
    /// Number of lease-index entries examined for expiration.
    pub lease_entries_examined: u64,
    /// Number of stale ready or lease entries removed.
    pub stale_index_repairs: u64,
    /// Number of queue transitions committed atomically.
    pub transition_count: u64,
    /// Number of keyspace operations included in queue transition batches.
    pub transition_operations: u64,
}

/// Selects the durable backend used by a persistent Thingd engine.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PersistentBackend {
    /// The mature C++ RocksDB backend. This remains the default.
    #[default]
    RocksDb,
    /// The experimental Rust-native ThingDB backend.
    ThingDb,
}

impl From<PersistentBackend> for StorageBackend {
    fn from(backend: PersistentBackend) -> Self {
        match backend {
            PersistentBackend::RocksDb => Self::RocksDb,
            PersistentBackend::ThingDb => Self::ThingDb,
        }
    }
}

fn manifest_keyspaces() -> Vec<String> {
    #[cfg(feature = "vectors")]
    let extra = std::iter::once(REQUIRED_VECTOR_KEYSPACE);
    #[cfg(not(feature = "vectors"))]
    let extra = std::iter::empty();

    REQUIRED_KEYSPACES
        .iter()
        .copied()
        .chain(extra)
        .map(str::to_string)
        .collect()
}

fn search_index_compatible(path: &Path) -> Option<bool> {
    let search_dir = path.join("search");
    if !search_dir.exists() {
        return None;
    }
    #[cfg(feature = "search")]
    {
        let Ok(index) = tantivy::Index::open_in_dir(search_dir) else {
            return Some(false);
        };
        let schema = index.schema();
        Some(
            ["doc_key", "collection", "id", "body", "kind"]
                .iter()
                .all(|field| schema.get_field(field).is_ok()),
        )
    }
    #[cfg(not(feature = "search"))]
    {
        Some(false)
    }
}

fn validate_existing_directory(path: &Path) -> ThingdResult<Option<StorageValidationReport>> {
    if !path.exists() {
        return Ok(None);
    }
    if !path.is_dir() {
        return Err(ThingdError::StorageValidation(format!(
            "database path is not a directory: {}",
            path.display()
        )));
    }

    let has_entries = std::fs::read_dir(path)
        .map_err(|error| ThingdError::StorageValidation(error.to_string()))?
        .next()
        .transpose()
        .map_err(|error| ThingdError::StorageValidation(error.to_string()))?
        .is_some();
    if !has_entries {
        return Ok(None);
    }

    let lock_present = path.join(STORAGE_LOCK_FILE).is_file() || path.join("LOCK").is_file();
    let rocksdb_layout = path.join("CURRENT").is_file();
    if rocksdb_layout && !manifest_path_exists(path) {
        return Err(ThingdError::UnsupportedStorageFormat(
            "RocksDB directory is missing the Thingd storage manifest".to_string(),
        ));
    }
    if !lock_present && !rocksdb_layout {
        return Err(ThingdError::StorageValidation(format!(
            "missing required lock file: {}",
            path.join(STORAGE_LOCK_FILE).display()
        )));
    }

    let keyspaces_present = path.join(STORAGE_KEYSPACES_DIR).is_dir() || rocksdb_layout;
    if !keyspaces_present {
        return Err(ThingdError::UnsupportedStorageFormat(format!(
            "missing keyspaces directory: {}",
            path.join(STORAGE_KEYSPACES_DIR).display()
        )));
    }

    let manifest_path = path.join(STORAGE_MANIFEST_FILE);
    if manifest_path.exists() {
        let bytes = std::fs::read(&manifest_path)
            .map_err(|error| ThingdError::StorageValidation(error.to_string()))?;
        let manifest: StorageManifest = serde_json::from_slice(&bytes).map_err(|error| {
            ThingdError::UnsupportedStorageFormat(format!(
                "invalid {}: {error}",
                manifest_path.display()
            ))
        })?;
        if manifest.format_version != STORAGE_FORMAT_VERSION {
            return Err(ThingdError::UnsupportedStorageFormat(format!(
                "expected format version {}, found {}",
                STORAGE_FORMAT_VERSION, manifest.format_version
            )));
        }
        if manifest.contract != STORAGE_CONTRACT {
            return Err(ThingdError::UnsupportedStorageFormat(format!(
                "expected contract {STORAGE_CONTRACT}, found {}",
                manifest.contract
            )));
        }
        for required in manifest_keyspaces() {
            if !manifest.keyspaces.iter().any(|found| found == &required) {
                return Err(ThingdError::UnsupportedStorageFormat(format!(
                    "manifest does not declare keyspace {required}"
                )));
            }
        }
        return Ok(Some(StorageValidationReport {
            format_version: manifest.format_version,
            legacy_manifest: false,
            lock_present,
            keyspaces_present,
            search_index_compatible: search_index_compatible(path),
        }));
    }

    if path.join(STORAGE_KEYSPACES_DIR).is_dir()
        || std::fs::read_dir(path)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .any(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "jnl")
            })
    {
        return Err(ThingdError::UnsupportedStorageFormat(
            "legacy storage format is not supported by this runtime; restore it with the archived compatibility release or import a logical export".to_string(),
        ));
    }

    // An otherwise empty directory is a valid new RocksDB destination and
    // receives a manifest during the normal open.
    Ok(Some(StorageValidationReport {
        format_version: STORAGE_FORMAT_VERSION,
        legacy_manifest: true,
        lock_present,
        keyspaces_present,
        search_index_compatible: search_index_compatible(path),
    }))
}

fn validate_thingdb_directory(path: &Path) -> ThingdResult<StorageValidationReport> {
    if !path.is_dir() {
        return Err(ThingdError::StorageValidation(format!(
            "ThingDB path is not a directory: {}",
            path.display()
        )));
    }
    let manifest_path = path.join("MANIFEST.json");
    let bytes = std::fs::read(&manifest_path).map_err(|error| {
        ThingdError::StorageValidation(format!("read {}: {error}", manifest_path.display()))
    })?;
    let manifest: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        ThingdError::UnsupportedStorageFormat(format!(
            "invalid ThingDB manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    let format_version = manifest
        .get("format_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            ThingdError::UnsupportedStorageFormat(
                "ThingDB manifest has no format_version".to_string(),
            )
        })?;
    if format_version != 1 {
        return Err(ThingdError::UnsupportedStorageFormat(format!(
            "unsupported ThingDB format version {format_version}"
        )));
    }
    Ok(StorageValidationReport {
        format_version: u32::try_from(format_version).unwrap_or(u32::MAX),
        legacy_manifest: false,
        lock_present: path.join("LOCK").is_file(),
        keyspaces_present: true,
        search_index_compatible: search_index_compatible(path),
    })
}

fn manifest_path_exists(path: &Path) -> bool {
    path.join(STORAGE_MANIFEST_FILE).is_file()
}

fn write_or_validate_manifest(
    path: &Path,
    existing: Option<&StorageValidationReport>,
    backend: PersistentBackend,
) -> ThingdResult<()> {
    let manifest_path = path.join(STORAGE_MANIFEST_FILE);
    if manifest_path.exists() {
        return Ok(());
    }
    if let Some(report) = existing
        && !report.legacy_manifest
    {
        return Ok(());
    }
    let manifest = StorageManifest {
        format_version: STORAGE_FORMAT_VERSION,
        contract: match backend {
            PersistentBackend::RocksDb => STORAGE_CONTRACT.to_string(),
            PersistentBackend::ThingDb => "thingdb-tantivy-v1".to_string(),
        },
        keyspaces: manifest_keyspaces(),
        search_schema_version: 1,
    };
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| ThingdError::StorageValidation(error.to_string()))?;
    std::fs::write(manifest_path, bytes)
        .map_err(|error| ThingdError::StorageValidation(error.to_string()))
}

/// Options used when opening a persistent database.
#[derive(Clone)]
pub struct PersistentOpenOptions {
    /// Durable backend selection. RocksDB remains the default.
    pub backend: PersistentBackend,
    /// Optional authenticated-encryption configuration.
    pub encryption: Option<EncryptionConfig>,
    /// Permit an explicit encrypted-to-plaintext migration when used by
    /// `PersistentEngine::reencrypt_to`. Ignored during normal open.
    pub allow_plaintext_output: bool,
    /// Controls whether the persistent Tantivy index is opened.
    pub search_mode: PersistentSearchMode,
    /// Maximum RocksDB WAL budget before maintenance backpressure applies.
    pub max_journal_bytes: u64,
    /// Maximum records processed by one search-rebuild step.
    pub recovery_batch_size: usize,
    /// Milliseconds to yield between recovery batches.
    pub recovery_pause_ms: u64,
    /// Maximum number of automatic recovery retries.
    pub recovery_max_retries: u64,
    /// Optional resident-memory ceiling for recovery, in bytes.
    pub recovery_memory_limit_bytes: Option<u64>,
    /// Maximum delay before the asynchronous search worker commits pending mutations.
    pub search_commit_interval_ms: u64,
    /// Maximum mutations included in one asynchronous search commit.
    pub search_commit_batch_size: usize,
    /// Maximum distinct document keys retained by the asynchronous search queue.
    pub search_queue_max_keys: usize,
}

impl Default for PersistentOpenOptions {
    fn default() -> Self {
        Self {
            backend: PersistentBackend::default(),
            encryption: None,
            allow_plaintext_output: false,
            search_mode: PersistentSearchMode::Persistent,
            max_journal_bytes: DEFAULT_MAX_JOURNAL_BYTES,
            recovery_batch_size: 32,
            recovery_pause_ms: 50,
            recovery_max_retries: MAX_SEARCH_REBUILD_RETRIES,
            recovery_memory_limit_bytes: None,
            search_commit_interval_ms: DEFAULT_SEARCH_COMMIT_INTERVAL_MS,
            search_commit_batch_size: DEFAULT_SEARCH_COMMIT_BATCH_SIZE,
            search_queue_max_keys: DEFAULT_SEARCH_QUEUE_MAX_KEYS,
        }
    }
}

#[cfg(feature = "vectors")]
#[derive(serde::Serialize, serde::Deserialize)]
struct StoredVector {
    collection: String,
    id: String,
    vector: Vec<f32>,
}

fn value_to_vec(v: Option<Slice>) -> Option<Vec<u8>> {
    v.map(|c| c.to_vec())
}

impl PersistentEngine {
    #[cfg(feature = "search")]
    const fn should_run_async_search_worker(&self) -> bool {
        self.search_index.is_some()
            && self.search_writer.is_some()
            && self.search_reader.is_some()
            && !matches!(self.search_mode, PersistentSearchMode::PersistentNoRebuild)
            && !self.search_rebuild_required
    }

    #[cfg(feature = "search")]
    fn start_search_worker(&mut self) {
        if self.search_worker_started || !self.should_run_async_search_worker() {
            return;
        }
        let (Some(index), Some(writer), Some(reader)) = (
            self.search_index.clone(),
            self.search_writer.clone(),
            self.search_reader.clone(),
        ) else {
            return;
        };
        let queue = Arc::new(SearchMutationQueue::new(self.search_queue_max_keys));
        self.search_queue = Some(queue.clone());
        self.search_worker_started = true;
        let interval = Duration::from_millis(self.search_commit_interval_ms);
        let batch_size = self.search_commit_batch_size;
        self.search_worker = thread::Builder::new()
            .name("thingd-search-index".to_string())
            .spawn(move || {
                loop {
                    let mutations = queue.take_batch(batch_size, interval);
                    if mutations.is_empty() {
                        if queue.is_shutdown() {
                            break;
                        }
                        thread::sleep(Duration::from_millis(50));
                        continue;
                    }
                    let started = Instant::now();
                    let result = writer
                        .lock()
                        .map_err(|_| {
                            ThingdError::Storage("search writer lock poisoned".to_string())
                        })
                        .and_then(|mut writer| {
                            for mutation in &mutations {
                                Self::apply_search_mutation(&index, &writer, mutation)?;
                            }
                            writer
                                .commit()
                                .map_err(|error| ThingdError::Storage(error.to_string()))
                        });
                    match result {
                        Ok(_) => {
                            if let Ok(reader) = reader.lock() {
                                let _ = reader.reload();
                            }
                            queue.record_commit(started.elapsed(), mutations.len());
                        },
                        Err(error) => {
                            queue.record_error(error.to_string());
                            thread::sleep(Duration::from_millis(100));
                        },
                    }
                }
            })
            .ok();
    }

    #[cfg(feature = "search")]
    fn stop_search_worker(&mut self) {
        if let Some(queue) = self.search_queue.take() {
            queue.shutdown();
        }
        if let Some(worker) = self.search_worker.take() {
            let _ = worker.join();
        }
        self.search_worker_started = false;
    }

    #[cfg(feature = "search")]
    fn apply_search_mutation(
        index: &tantivy::Index,
        writer: &tantivy::IndexWriter<tantivy::TantivyDocument>,
        mutation: &SearchIndexMutation,
    ) -> ThingdResult<()> {
        let schema = index.schema();
        let doc_key_field = schema
            .get_field("doc_key")
            .map_err(|error| ThingdError::Storage(error.to_string()))?;
        let body_field = schema
            .get_field("body")
            .map_err(|error| ThingdError::Storage(error.to_string()))?;
        let collection_field = schema
            .get_field("collection")
            .map_err(|error| ThingdError::Storage(error.to_string()))?;
        let id_field = schema
            .get_field("id")
            .map_err(|error| ThingdError::Storage(error.to_string()))?;
        let kind_field = schema
            .get_field("kind")
            .map_err(|error| ThingdError::Storage(error.to_string()))?;

        let (doc_key, collection, id, body, kind) = match mutation {
            SearchIndexMutation::UpsertObject(object) => (
                format!("{}/{}", object.key.collection, object.key.id),
                object.key.collection.clone(),
                object.key.id.clone(),
                object.body.clone(),
                "object",
            ),
            SearchIndexMutation::UpsertEvent(event) => (
                format!("event:{}/{}", event.stream, event.sequence),
                event.stream.clone(),
                event.sequence.to_string(),
                event.body.clone(),
                "event",
            ),
            SearchIndexMutation::DeleteObject { collection, id } => {
                let doc_key = format!("{collection}/{id}");
                let term = tantivy::Term::from_field_text(doc_key_field, &doc_key);
                writer.delete_term(term);
                return Ok(());
            },
            SearchIndexMutation::DeleteEvent { stream, sequence } => {
                let doc_key = format!("event:{stream}/{sequence}");
                let term = tantivy::Term::from_field_text(doc_key_field, &doc_key);
                writer.delete_term(term);
                return Ok(());
            },
        };

        writer.delete_term(tantivy::Term::from_field_text(doc_key_field, &doc_key));
        let mut doc = tantivy::TantivyDocument::new();
        doc.add_text(doc_key_field, doc_key);
        doc.add_text(collection_field, collection);
        doc.add_text(id_field, id);
        doc.add_text(body_field, body);
        doc.add_text(kind_field, kind);
        writer
            .add_document(doc)
            .map_err(|error| ThingdError::Storage(error.to_string()))?;
        Ok(())
    }

    /// Open or create a Persistent database at the given path.
    /// Creates all required keyspaces (partitions) on first open.
    pub fn open(path: impl AsRef<Path>) -> ThingdResult<Self> {
        Self::open_with_options(path, PersistentOpenOptions::default())
    }

    /// Create an in-memory ThingDB engine without touching the filesystem.
    ///
    /// This reuses the persistent semantic storage implementation, but all
    /// primary and derived state lives in process memory and is lost when the
    /// engine is dropped.
    pub fn open_in_memory_with_backend(backend: PersistentBackend) -> ThingdResult<Self> {
        if backend != PersistentBackend::ThingDb {
            return Err(ThingdError::Storage(
                "in-memory PersistentEngine requires the ThingDB backend".to_string(),
            ));
        }
        let options = PersistentOpenOptions {
            backend,
            ..PersistentOpenOptions::default()
        };
        let db = Database::in_memory_thingdb()?;
        Self::open_from_database(PathBuf::new(), options, db, make_codec(None), false, None)
    }

    /// Open a persistent database with explicit options.
    pub fn open_with_options(
        path: impl AsRef<Path>,
        options: PersistentOpenOptions,
    ) -> ThingdResult<Self> {
        let path = path.as_ref();
        if options.backend == PersistentBackend::ThingDb
            && (path.join("CURRENT").is_file()
                || path.join(STORAGE_MANIFEST_FILE).is_file()
                    && std::fs::read(path.join(STORAGE_MANIFEST_FILE))
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<StorageManifest>(&bytes).ok())
                        .is_some_and(|manifest| manifest.contract == STORAGE_CONTRACT))
        {
            return Err(ThingdError::UnsupportedStorageFormat(
                "RocksDB directory cannot be opened as ThingDB; use logical repack".to_string(),
            ));
        }
        let existing = if options.backend == PersistentBackend::RocksDb {
            validate_existing_directory(path)?
        } else {
            None
        };
        let crypto = StorageCrypto::open(path, options.encryption.as_ref())?;
        let codec = make_codec(crypto);
        let encrypted = codec.encrypted();
        let db = Database::builder_with_backend(path, options.backend.into())
            .max_journaling_size(options.max_journal_bytes)
            .open()?;

        Self::open_from_database(path, options, db, codec, encrypted, existing)
    }

    fn open_from_database(
        path: impl AsRef<Path>,
        options: PersistentOpenOptions,
        db: Database,
        codec: Box<dyn StorageCodec>,
        encrypted: bool,
        existing: Option<StorageValidationReport>,
    ) -> ThingdResult<Self> {
        let path = path.as_ref();
        let in_memory = db.is_in_memory();

        let objects = db.keyspace("objects", KeyspaceCreateOptions::default)?;
        let events = db.keyspace("events", KeyspaceCreateOptions::default)?;
        let event_meta = db.keyspace("event_meta", KeyspaceCreateOptions::default)?;
        let queue_jobs = db.keyspace("queue_jobs", KeyspaceCreateOptions::default)?;
        let ready_jobs = db.keyspace("ready_jobs", KeyspaceCreateOptions::default)?;
        let lease_jobs = db.keyspace(LEASE_KEYSPACE, KeyspaceCreateOptions::default)?;
        let links_by_id = db.keyspace("links_by_id", KeyspaceCreateOptions::default)?;
        let links_from = db.keyspace("links_from", KeyspaceCreateOptions::default)?;
        let links_to = db.keyspace("links_to", KeyspaceCreateOptions::default)?;
        let schemas = db.keyspace("schemas", KeyspaceCreateOptions::default)?;
        let migrations = db.keyspace("migrations", KeyspaceCreateOptions::default)?;
        let indexes = db.keyspace("indexes", KeyspaceCreateOptions::default)?;

        #[cfg(feature = "vectors")]
        let vectors = db.keyspace("vectors", KeyspaceCreateOptions::default)?;

        if !in_memory {
            write_or_validate_manifest(path, existing.as_ref(), options.backend)?;
        }

        let mut next_link_id = 0u64;
        for kv in links_by_id.iter() {
            let (_, value) = guard_data(kv)?;
            let link: Link = {
                let decoded = codec.decode_value("record", &value)?;
                serde_json::from_slice(&decoded).map_err(|e| ThingdError::Storage(e.to_string()))?
            };
            if let Some(id_str) = link.id.strip_prefix("link-")
                && let Ok(id) = id_str.parse::<u64>()
                && id > next_link_id
            {
                next_link_id = id;
            }
        }

        let mut event_seq_counters: HashMap<String, u64> = HashMap::new();
        let mut event_idempotency_keys: HashMap<(String, String), u64> = HashMap::new();
        let mut metadata_loaded = false;
        for kv in event_meta.iter() {
            let (_, value) = guard_data(kv)?;
            let decoded = codec.decode_value("event_metadata", &value)?;
            let metadata: EventMetadata = serde_json::from_slice(&decoded)
                .map_err(|e| ThingdError::Storage(e.to_string()))?;
            metadata_loaded = true;
            let stream = metadata.stream.clone();
            event_seq_counters.insert(stream.clone(), metadata.max_sequence);
            for (idempotency_key, sequence) in metadata.idempotency_keys {
                event_idempotency_keys.insert((stream.clone(), idempotency_key), sequence);
            }
        }

        // Legacy stores do not have event metadata yet. Scan them once and persist
        // the derived counters so later opens do not repeat the full reconstruction.
        if !metadata_loaded {
            for kv in events.iter() {
                let (_, value) = guard_data(kv)?;
                let decoded = codec.decode_value("record", &value)?;
                let event: MemoryEvent = serde_json::from_slice(&decoded)
                    .map_err(|e| ThingdError::Storage(e.to_string()))?;
                let stream = event.stream.clone();
                let seq = event.sequence;
                event_seq_counters
                    .entry(stream.clone())
                    .and_modify(|max| *max = (*max).max(seq))
                    .or_insert(seq);
                if !event.idempotency_key.is_empty() {
                    event_idempotency_keys.insert((stream, event.idempotency_key), seq);
                }
            }
        }

        let (unique_index_values, unique_index_cache_complete) =
            Self::build_unique_index_cache(&objects, &indexes, codec.as_ref())?;

        #[cfg(feature = "search")]
        let (search_index, rebuild_search_index) = if in_memory {
            match options.search_mode {
                PersistentSearchMode::Disabled | PersistentSearchMode::PersistentNoRebuild => {
                    (None, false)
                },
                _ => (Self::create_search_index(true), true),
            }
        } else {
            match options.search_mode {
                PersistentSearchMode::Disabled => (None, false),
                PersistentSearchMode::Persistent | PersistentSearchMode::PersistentAsync
                    if encrypted =>
                {
                    (Self::create_search_index(true), true)
                },
                PersistentSearchMode::PersistentNoRebuild if encrypted => (None, false),
                PersistentSearchMode::Persistent => Self::init_search_index(path, true)?,
                PersistentSearchMode::PersistentRecovery if encrypted => (None, true),
                PersistentSearchMode::PersistentAsync
                | PersistentSearchMode::PersistentRecovery => {
                    let (index, _) = Self::init_search_index(path, false)?;
                    let rebuild = index.is_none();
                    (index, rebuild)
                },
                PersistentSearchMode::PersistentNoRebuild => Self::init_search_index(path, false)?,
            }
        };

        #[cfg(feature = "search")]
        let search_writer = if options.search_mode == PersistentSearchMode::PersistentNoRebuild {
            None
        } else {
            search_index
                .as_ref()
                .map(|index| {
                    index
                        .writer(SEARCH_WRITER_MEMORY_BYTES)
                        .map(|writer| Arc::new(Mutex::new(writer)))
                        .map_err(|error| {
                            ThingdError::Storage(format!("create search writer: {error}"))
                        })
                })
                .transpose()?
        };

        #[cfg(feature = "search")]
        let search_reader = search_index
            .as_ref()
            .map(|index| {
                index
                    .reader()
                    .map(|reader| Arc::new(Mutex::new(reader)))
                    .map_err(|error| ThingdError::Storage(format!("create search reader: {error}")))
            })
            .transpose()?;

        let initial_rebuild_required = {
            #[cfg(feature = "search")]
            {
                rebuild_search_index
            }
            #[cfg(not(feature = "search"))]
            {
                false
            }
        };
        let initial_compaction_required =
            db.journal_disk_space().unwrap_or_default() > options.max_journal_bytes;
        let initial_journal_bytes = db.journal_disk_space().unwrap_or_default();
        let initial_journal_count = db.journal_count() as u64;

        let mut engine = Self {
            db,
            path: path.to_path_buf(),
            objects,
            events,
            event_meta,
            queue_jobs,
            ready_jobs,
            lease_jobs,
            links_by_id,
            links_from,
            links_to,
            schemas,
            migrations,
            indexes,
            next_link_id: AtomicU64::new(next_link_id + 1),
            event_seq_counters,
            event_idempotency_keys,
            unique_index_values,
            unique_index_cache_complete,
            #[cfg(feature = "search")]
            search_index,
            #[cfg(feature = "search")]
            search_writer,
            #[cfg(feature = "search")]
            search_reader,
            #[cfg(feature = "search")]
            search_mode: options.search_mode,
            #[cfg(feature = "search")]
            search_queue: None,
            #[cfg(feature = "search")]
            search_commit_interval_ms: options.search_commit_interval_ms,
            #[cfg(feature = "search")]
            search_commit_batch_size: options.search_commit_batch_size.max(1),
            #[cfg(feature = "search")]
            search_queue_max_keys: options.search_queue_max_keys.max(1),
            #[cfg(feature = "search")]
            search_worker_started: false,
            #[cfg(feature = "search")]
            search_worker: None,
            #[cfg(feature = "search")]
            search_rebuild: None,
            #[cfg(feature = "search")]
            search_rebuild_required: rebuild_search_index,
            #[cfg(feature = "search")]
            search_rebuild_path: None,
            maintenance: StorageMaintenanceStatus {
                state: if initial_rebuild_required {
                    "rebuilding_search".to_string()
                } else if initial_compaction_required {
                    "compacting".to_string()
                } else {
                    "idle".to_string()
                },
                generation: u64::from(initial_rebuild_required),
                retry_count: 0,
                error: None,
                phase: if initial_rebuild_required || initial_compaction_required {
                    "primary"
                } else {
                    "complete"
                }
                .to_string(),
                processed: 0,
                total: 0,
                journal_bytes: initial_journal_bytes,
                journal_count: initial_journal_count,
                journal_limit_bytes: options.max_journal_bytes,
                search_queue_depth: 0,
                search_queue_capacity: options.search_queue_max_keys as u64,
                search_mutations_queued: 0,
                search_mutations_coalesced: 0,
                search_mutations_committed: 0,
                search_last_commit_unix_ms: None,
                search_last_commit_duration_ms: None,
                search_last_error: None,
                search_retry_count: 0,
                search_stale: false,
            },
            queue_diagnostics: QueueDiagnostics::default(),
            recovery_batch_size: options.recovery_batch_size.max(1),
            recovery_pause_ms: options.recovery_pause_ms,
            recovery_max_retries: options.recovery_max_retries,
            recovery_memory_limit_bytes: options.recovery_memory_limit_bytes,
            max_journal_bytes: options.max_journal_bytes,
            #[cfg(feature = "vectors")]
            vectors,
            codec,
        };

        if !in_memory {
            engine.rebuild_lease_index()?;
        }

        #[cfg(feature = "search")]
        if engine.should_run_async_search_worker() {
            engine.start_search_worker();
        }

        if !metadata_loaded {
            engine.persist_all_event_metadata()?;
        }

        #[cfg(feature = "search")]
        if rebuild_search_index {
            if matches!(
                options.search_mode,
                PersistentSearchMode::PersistentAsync | PersistentSearchMode::PersistentRecovery
            ) {
                return Ok(engine);
            }
            for entry in engine.objects.iter() {
                let (_, value) = guard_data(entry)?;
                let object: MemoryObject = engine.deserialize(&value)?;
                engine.index_object_for_search_with_commit(&object, false);
            }
            for entry in engine.events.iter() {
                let (_, value) = guard_data(entry)?;
                let event: MemoryEvent = engine.deserialize(&value)?;
                engine.index_event_for_search_with_commit(&event, false);
            }
            engine.commit_search_index();
            engine.search_rebuild_required = false;
            engine.maintenance.state = "idle".to_string();
        }

        #[cfg(feature = "search")]
        engine.start_search_worker();

        Ok(engine)
    }

    fn rebuild_lease_index(&self) -> ThingdResult<()> {
        let mut batch = self.db.batch();
        for entry in self.lease_jobs.iter() {
            let (key, _) = guard_data(entry)?;
            batch.remove(&self.lease_jobs, key);
        }
        for entry in self.queue_jobs.iter() {
            let (_, value) = guard_data(entry)?;
            let job: QueueJob = self.deserialize(&value)?;
            if job.status == QueueJobStatus::Leased
                && let Some(expires_at_ms) = job.lease_expires_at_ms
            {
                let key = self.make_lease_key(&job.queue, expires_at_ms, &job.id);
                let data = self.serialize(&job.id)?;
                batch.insert(&self.lease_jobs, key, data);
            }
        }
        batch
            .commit()
            .map_err(|error| ThingdError::Storage(error.to_string()))
    }

    /// Return whether this engine needs an asynchronous derived-index rebuild.
    #[cfg(feature = "search")]
    pub fn search_rebuild_required(&self) -> bool {
        self.search_rebuild_required
            || self
                .search_queue
                .as_ref()
                .is_some_and(|queue| queue.is_stale())
    }

    /// Return the current storage maintenance state.
    pub fn storage_maintenance_status(&self) -> StorageMaintenanceStatus {
        let mut status = self.maintenance.clone();
        status.journal_bytes = self.journal_bytes();
        status.journal_count = self.journal_count();
        status.journal_limit_bytes = self.max_journal_bytes;
        #[cfg(feature = "search")]
        if let Some(queue) = &self.search_queue {
            let snapshot = queue.snapshot();
            status.search_queue_depth = queue.depth() as u64;
            status.search_queue_capacity = queue.max_keys as u64;
            status.search_mutations_queued = snapshot.queued;
            status.search_mutations_coalesced = snapshot.coalesced;
            status.search_mutations_committed = snapshot.committed;
            status.search_last_commit_unix_ms = snapshot.last_commit_unix_ms;
            status.search_last_commit_duration_ms = snapshot.last_commit_duration_ms;
            status.search_last_error = snapshot.last_error;
            status.search_retry_count = snapshot.retry_count;
            status.search_stale = snapshot.stale;
        }
        #[cfg(feature = "search")]
        if let Some(progress) = &self.search_rebuild {
            status.phase = match &progress.phase {
                SearchRebuildPhase::Objects | SearchRebuildPhase::Events => "search",
                SearchRebuildPhase::Replay => "search-replay",
            }
            .to_string();
            status.processed = progress.processed;
            status.total = progress.total;
        }
        status
    }

    /// Return bounded queue-index diagnostics.
    pub fn queue_diagnostics(&self) -> QueueDiagnostics {
        self.queue_diagnostics.clone()
    }

    /// Return the bounded recovery work budget.
    pub const fn recovery_budget(&self) -> crate::RecoveryBudget {
        crate::RecoveryBudget {
            batch_size: self.recovery_batch_size,
            pause_ms: self.recovery_pause_ms,
            max_retries: self.recovery_max_retries,
            memory_limit_bytes: self.recovery_memory_limit_bytes,
        }
    }

    /// Fail recovery closed with an operator-visible error.
    pub fn fail_storage_recovery(&mut self, message: String) {
        self.maintenance.state = "failed".to_string();
        self.maintenance.error = Some(message);
    }

    /// Return current RocksDB WAL bytes.
    pub fn journal_bytes(&self) -> u64 {
        self.db.journal_disk_space().unwrap_or_default()
    }

    /// Return the number of RocksDB WAL files retained by the database.
    pub fn journal_count(&self) -> u64 {
        self.db.journal_count() as u64
    }

    /// Return whether this engine stores all state in process memory.
    pub fn is_in_memory(&self) -> bool {
        self.db.is_in_memory()
    }

    /// Return ThingDB WAL timings and recovery diagnostics when ThingDB is active.
    pub fn wal_diagnostics(&self) -> ThingdResult<Option<thingdb::WalDiagnostics>> {
        self.db
            .wal_diagnostics()
            .map_err(|error| ThingdError::Storage(error.to_string()))
    }

    /// Return RAM-only ThingDB lookup and pipeline diagnostics.
    pub fn ram_diagnostics(&self) -> ThingdResult<Option<thingdb::RamDiagnostics>> {
        if !self.is_in_memory() {
            return Ok(None);
        }
        self.db
            .ram_diagnostics()
            .map(Some)
            .map_err(|error| ThingdError::Storage(error.to_string()))
    }

    /// Persist and compact every primary keyspace.
    pub fn compact_storage(&mut self) -> ThingdResult<()> {
        if self.db.is_in_memory() {
            return Err(ThingdError::Storage(
                "in-memory ThingDB does not support persistence or compaction".to_string(),
            ));
        }
        self.maintenance.state = "compacting".to_string();
        self.maintenance.phase = "primary".to_string();
        self.maintenance.processed = 0;
        self.maintenance.total = 0;
        self.maintenance.error = None;
        let result: ThingdResult<()> = (|| {
            self.db
                .persist(PersistMode::SyncAll)
                .map_err(|error| ThingdError::Storage(error.to_string()))?;
            for keyspace in [
                &self.objects,
                &self.events,
                &self.event_meta,
                &self.queue_jobs,
                &self.ready_jobs,
                &self.lease_jobs,
                &self.links_by_id,
                &self.links_from,
                &self.links_to,
                &self.schemas,
                &self.migrations,
                &self.indexes,
            ] {
                keyspace
                    .major_compact()
                    .map_err(|error| ThingdError::Storage(error.to_string()))?;
            }
            #[cfg(feature = "vectors")]
            self.vectors
                .major_compact()
                .map_err(|error| ThingdError::Storage(error.to_string()))?;
            self.db
                .persist(PersistMode::SyncAll)
                .map_err(|error| ThingdError::Storage(error.to_string()))?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                if self.needs_search_rebuild() {
                    self.maintenance.state = "rebuilding_search".to_string();
                    self.maintenance.phase = "search".to_string();
                } else {
                    self.maintenance.state = "idle".to_string();
                    self.maintenance.phase = "complete".to_string();
                }
                Ok(())
            },
            Err(error) => {
                self.maintenance.state = "failed".to_string();
                self.maintenance.phase = "failed".to_string();
                self.maintenance.error = Some(error.to_string());
                Err(error)
            },
        }
    }

    #[cfg(feature = "search")]
    const fn needs_search_rebuild(&self) -> bool {
        self.search_rebuild_required
    }

    #[cfg(not(feature = "search"))]
    const fn needs_search_rebuild(&self) -> bool {
        false
    }

    /// Return the current asynchronous search rebuild status, if one is active
    /// or has failed.
    #[cfg(feature = "search")]
    pub fn search_rebuild_status(&self) -> Option<SearchRebuildStatus> {
        let progress = self.search_rebuild.as_ref()?;
        Some(SearchRebuildStatus {
            state: if progress.error.is_some() || self.maintenance.state == "degraded" {
                "degraded".to_string()
            } else {
                "rebuilding".to_string()
            },
            processed: progress.processed,
            total: progress.total,
            generation: progress.generation,
            error: progress.error.clone(),
        })
    }

    /// Process one bounded batch of an asynchronous search rebuild.
    ///
    /// # Errors
    ///
    /// Returns a storage error when the derived index cannot be created,
    /// opened, read, or committed.
    #[cfg(feature = "search")]
    pub fn search_rebuild_step(&mut self, batch_size: usize) -> ThingdResult<bool> {
        if self
            .search_queue
            .as_ref()
            .is_some_and(|queue| queue.is_stale())
            && self.search_rebuild.is_none()
        {
            self.search_rebuild_required = true;
            self.stop_search_worker();
            self.search_writer = None;
            self.search_reader = None;
            self.search_index = None;
        }
        if !self.search_rebuild_required {
            return Ok(true);
        }
        if let Some(progress) = self.search_rebuild.as_ref()
            && (progress.error.is_some() || progress.replay_overflow)
        {
            if progress.replay_overflow {
                self.maintenance.state = "degraded".to_string();
            }
            return Ok(false);
        }

        if self.search_rebuild.is_none() {
            if self.search_index.is_none() {
                let in_memory = self.is_in_memory();
                let (index, rebuild_path) = if in_memory {
                    (Self::create_search_index(true), None)
                } else {
                    let generation = self.maintenance.generation.max(1);
                    let rebuild_path = if self.codec.encrypted() {
                        None
                    } else {
                        let path = self.path.join(format!(".search-rebuild-{generation}"));
                        if path.exists() {
                            std::fs::remove_dir_all(&path).map_err(|error| {
                                ThingdError::Storage(format!(
                                    "remove stale search generation: {error}"
                                ))
                            })?;
                        }
                        Some(path)
                    };
                    let index = match rebuild_path.as_ref() {
                        Some(path) => Some(Self::create_search_index_at(path)?),
                        None => Self::init_search_index(&self.path, true)?.0,
                    };
                    (index, rebuild_path)
                };
                self.search_index = index;
                self.search_rebuild_path = rebuild_path;
                if !in_memory {
                    std::fs::write(
                        self.path.join(".thingd-search-rebuild"),
                        self.maintenance.generation.max(1).to_string(),
                    )
                    .map_err(|error| {
                        ThingdError::Storage(format!("write search rebuild marker: {error}"))
                    })?;
                }
                self.search_writer = self
                    .search_index
                    .as_ref()
                    .map(|index| {
                        index
                            .writer(SEARCH_WRITER_MEMORY_BYTES)
                            .map(|writer| Arc::new(Mutex::new(writer)))
                            .map_err(|error| {
                                ThingdError::Storage(format!("create search writer: {error}"))
                            })
                    })
                    .transpose()?;
                self.search_reader = self
                    .search_index
                    .as_ref()
                    .map(|index| {
                        index
                            .reader()
                            .map(|reader| Arc::new(Mutex::new(reader)))
                            .map_err(|error| {
                                ThingdError::Storage(format!("create search reader: {error}"))
                            })
                    })
                    .transpose()?;
            }
            self.search_rebuild = Some(SearchRebuildProgress {
                generation: self.maintenance.generation.max(1),
                phase: SearchRebuildPhase::Objects,
                object_cursor: None,
                event_cursor: None,
                processed: 0,
                total: self.objects.iter().count() as u64 + self.events.iter().count() as u64,
                replay: HashMap::new(),
                replay_overflow: false,
                error: None,
            });
        }

        let Some(mut progress) = self.search_rebuild.take() else {
            return Err(ThingdError::Storage(
                "search rebuild state was not initialized".to_string(),
            ));
        };
        let batch_size = batch_size.max(1);
        let result: ThingdResult<bool> = (|| {
            match progress.phase {
                SearchRebuildPhase::Objects => {
                    let items = match progress.object_cursor.as_ref() {
                        Some(cursor) => self
                            .objects
                            .range::<&[u8], _>((Excluded(cursor.as_slice()), Unbounded::<&[u8]>))
                            .take(batch_size)
                            .collect::<Vec<_>>(),
                        None => self.objects.iter().take(batch_size).collect::<Vec<_>>(),
                    };
                    let mut last_key = None;
                    for entry in items {
                        let (key, value) = guard_data(entry)?;
                        let object: MemoryObject = self.deserialize(&value)?;
                        self.index_object_for_search_with_commit(&object, false);
                        last_key = Some(key);
                        progress.processed += 1;
                    }
                    progress.object_cursor = last_key;
                    let objects_exhausted = progress.object_cursor.as_ref().is_none_or(|cursor| {
                        self.objects
                            .range::<&[u8], _>((Excluded(cursor.as_slice()), Unbounded::<&[u8]>))
                            .next()
                            .is_none()
                    });
                    if objects_exhausted {
                        progress.phase = SearchRebuildPhase::Events;
                    }
                },
                SearchRebuildPhase::Events => {
                    let items = match progress.event_cursor.as_ref() {
                        Some(cursor) => self
                            .events
                            .range::<&[u8], _>((Excluded(cursor.as_slice()), Unbounded::<&[u8]>))
                            .take(batch_size)
                            .collect::<Vec<_>>(),
                        None => self.events.iter().take(batch_size).collect::<Vec<_>>(),
                    };
                    let mut last_key = None;
                    for entry in items {
                        let (key, value) = guard_data(entry)?;
                        let event: MemoryEvent = self.deserialize(&value)?;
                        self.index_event_for_search_with_commit(&event, false);
                        last_key = Some(key);
                        progress.processed += 1;
                    }
                    progress.event_cursor = last_key;
                    let events_exhausted = progress.event_cursor.as_ref().is_none_or(|cursor| {
                        self.events
                            .range::<&[u8], _>((Excluded(cursor.as_slice()), Unbounded::<&[u8]>))
                            .next()
                            .is_none()
                    });
                    if events_exhausted {
                        progress.phase = SearchRebuildPhase::Replay;
                    }
                },
                SearchRebuildPhase::Replay => {
                    if progress.replay_overflow {
                        progress.error = Some(format!(
                            "search rebuild mutation replay exceeded {SEARCH_REBUILD_REPLAY_LIMIT} keys"
                        ));
                        self.maintenance.state = "degraded".to_string();
                        self.maintenance.phase = "search-replay".to_string();
                        self.maintenance.retry_count =
                            self.maintenance.retry_count.saturating_add(1);
                        return Ok(false);
                    }
                    let replay = std::mem::take(&mut progress.replay);
                    for (key, mutation) in replay {
                        self.replay_search_mutation(&key, mutation)?;
                    }
                    self.commit_search_index();
                    if self.is_in_memory() {
                        self.search_rebuild_path = None;
                    } else {
                        self.promote_search_generation()?;
                    }
                    self.search_rebuild_required = false;
                    self.search_rebuild = None;
                    self.start_search_worker();
                    if !self.is_in_memory() {
                        let _ = std::fs::remove_file(self.path.join(".thingd-search-rebuild"));
                    }
                    self.maintenance.generation = self.maintenance.generation.saturating_add(1);
                    self.maintenance.state = "idle".to_string();
                    self.maintenance.phase = "complete".to_string();
                    return Ok(true);
                },
            }
            self.commit_search_index();
            Ok(false)
        })();

        match result {
            Ok(done) => {
                if !done {
                    self.search_rebuild = Some(progress);
                }
                Ok(done)
            },
            Err(error) => {
                progress.error = Some(error.to_string());
                self.maintenance.state = "failed".to_string();
                self.maintenance.phase = "failed".to_string();
                self.search_rebuild = Some(progress);
                Err(error)
            },
        }
    }

    /// Reset a degraded rebuild for one bounded retry generation.
    #[cfg(feature = "search")]
    pub fn retry_search_rebuild(&mut self) -> bool {
        if self.maintenance.state != "degraded"
            || self.maintenance.retry_count >= self.recovery_max_retries
        {
            return false;
        }
        self.stop_search_worker();
        self.search_writer = None;
        self.search_reader = None;
        self.search_index = None;
        let _ = std::fs::remove_dir_all(self.path.join("search"));
        if let Some(path) = self.search_rebuild_path.take() {
            let _ = std::fs::remove_dir_all(path);
        }
        let _ = std::fs::remove_file(self.path.join(".thingd-search-rebuild"));
        self.search_rebuild = None;
        self.search_rebuild_required = true;
        self.maintenance.generation = self.maintenance.generation.saturating_add(1);
        self.maintenance.state = "rebuilding_search".to_string();
        self.maintenance.phase = "search".to_string();
        self.maintenance.error = None;
        true
    }

    #[cfg(not(feature = "search"))]
    pub fn retry_search_rebuild(&mut self) -> bool {
        false
    }

    /// Validate a native storage directory without opening RocksDB or mutating files.
    pub fn validate_path(path: impl AsRef<Path>) -> ThingdResult<StorageValidationReport> {
        Self::validate_path_with_backend(path, PersistentBackend::RocksDb)
    }

    /// Validate a durable directory using an explicit backend format.
    pub fn validate_path_with_backend(
        path: impl AsRef<Path>,
        backend: PersistentBackend,
    ) -> ThingdResult<StorageValidationReport> {
        let path = path.as_ref();
        if backend == PersistentBackend::ThingDb {
            return validate_thingdb_directory(path);
        }
        validate_existing_directory(path)?.ok_or_else(|| {
            ThingdError::StorageValidation("database directory does not exist yet".to_string())
        })
    }

    /// Persist all pending RocksDB WAL state before an offline directory copy.
    pub fn checkpoint(&self) -> ThingdResult<()> {
        self.db
            .persist(PersistMode::SyncAll)
            .map_err(|error| ThingdError::Storage(error.to_string()))
    }

    /// Re-encrypt a database into a new destination without modifying the source.
    ///
    /// The destination must not already exist. This operation is intended for
    /// offline migration and key rotation; it never changes a key implicitly.
    #[allow(clippy::items_after_statements)]
    pub fn reencrypt_to(
        source_path: impl AsRef<Path>,
        destination_path: impl AsRef<Path>,
        source_options: PersistentOpenOptions,
        destination_options: PersistentOpenOptions,
    ) -> ThingdResult<()> {
        let source_path = source_path.as_ref();
        let destination_path = destination_path.as_ref();
        if source_path == destination_path {
            return Err(ThingdError::EncryptionMigration(
                "source and destination paths must differ".to_string(),
            ));
        }
        if destination_path.exists() {
            return Err(ThingdError::EncryptionMigration(
                "destination already exists".to_string(),
            ));
        }

        if source_options.encryption.is_some()
            && destination_options.encryption.is_none()
            && !destination_options.allow_plaintext_output
        {
            return Err(ThingdError::EncryptionMigration(
                "encrypted-to-plaintext migration requires explicit opt-in".to_string(),
            ));
        }

        let result = (|| {
            let source = Self::open_with_options(source_path, source_options)?;
            let destination = Self::open_with_options(destination_path, destination_options)?;

            for entry in source.objects.iter() {
                let (_, value) = guard_data(entry)?;
                let object: MemoryObject = source.deserialize(&value)?;
                let key = destination.make_object_key(&object.key.collection, &object.key.id);
                let data = destination.serialize(&object)?;
                destination.objects.insert(&key, &data)?;
                #[cfg(feature = "vectors")]
                if let Some(vector) = object.vector {
                    let vkey = destination.make_vector_key(&object.key.collection, &object.key.id);
                    let vdata = destination.serialize(&StoredVector {
                        collection: object.key.collection.clone(),
                        id: object.key.id.clone(),
                        vector,
                    })?;
                    destination.vectors.insert(&vkey, &vdata)?;
                }
            }

            for entry in source.events.iter() {
                let (_, value) = guard_data(entry)?;
                let event: MemoryEvent = source.deserialize(&value)?;
                let key = destination.make_event_key(&event.stream, event.sequence);
                let data = destination.serialize(&event)?;
                destination.events.insert(&key, &data)?;
            }

            for entry in source.schemas.iter() {
                let (_, value) = guard_data(entry)?;
                let schema: StoredSchema = source.deserialize(&value)?;
                let key = destination.make_schema_key();
                destination
                    .schemas
                    .insert(key, destination.serialize(&schema)?)?;
            }

            for entry in source.migrations.iter() {
                let (_, value) = guard_data(entry)?;
                let migration: MigrationRecord = source.deserialize(&value)?;
                destination.migrations.insert(
                    destination.make_migration_key(&migration.id),
                    destination.serialize(&migration)?,
                )?;
            }

            for entry in source.indexes.iter() {
                let (_, value) = guard_data(entry)?;
                let index: IndexDefinition = source.deserialize(&value)?;
                destination.indexes.insert(
                    destination.make_index_key(&index.collection, &index.field),
                    destination.serialize(&index)?,
                )?;
            }

            for entry in source.queue_jobs.iter() {
                let (_, value) = guard_data(entry)?;
                let job: QueueJob = source.deserialize(&value)?;
                let key = destination.make_queue_key(&job.queue, &job.id);
                let data = destination.serialize(&job)?;
                destination.queue_jobs.insert(&key, &data)?;
                if job.status == QueueJobStatus::Ready {
                    let ready_key = destination.make_ready_key(
                        &job.queue,
                        job.priority,
                        &job.created_at,
                        &job.id,
                    );
                    let ready_data = destination.serialize(&job.id)?;
                    destination.ready_jobs.insert(&ready_key, &ready_data)?;
                } else if job.status == QueueJobStatus::Leased
                    && let Some(expires_at_ms) = job.lease_expires_at_ms
                {
                    let lease_key = destination.make_lease_key(&job.queue, expires_at_ms, &job.id);
                    let lease_data = destination.serialize(&job.id)?;
                    destination.lease_jobs.insert(&lease_key, &lease_data)?;
                }
            }

            #[cfg(feature = "vectors")]
            for entry in source.vectors.iter() {
                let (physical_key, value) = guard_data(entry)?;
                let vector = if let Ok(vector) = source.deserialize::<StoredVector>(&value) {
                    vector
                } else {
                    let Some(separator) = physical_key.iter().rposition(|byte| *byte == 0) else {
                        return Err(ThingdError::Storage(
                            "legacy vector record has no collection/id separator".to_string(),
                        ));
                    };
                    let collection = String::from_utf8_lossy(&physical_key[..separator]);
                    let id = String::from_utf8_lossy(&physical_key[separator + 1..]);
                    StoredVector {
                        collection: collection.into_owned(),
                        id: id.into_owned(),
                        vector: source.deserialize(&value)?,
                    }
                };
                let key = destination.make_vector_key(&vector.collection, &vector.id);
                let data = destination.serialize(&vector)?;
                destination.vectors.insert(&key, &data)?;
            }

            for entry in source.links_by_id.iter() {
                let (_, value) = guard_data(entry)?;
                let link: Link = source.deserialize(&value)?;
                let data = destination.serialize(&link)?;
                let index_data = destination.serialize(&link.id)?;
                destination
                    .links_by_id
                    .insert(destination.make_link_id_key(&link.id), &data)?;
                destination.links_from.insert(
                    destination.make_link_from_key(&link.from_ref, &link.link_type, &link.id),
                    &index_data,
                )?;
                destination.links_to.insert(
                    destination.make_link_to_key(&link.to_ref, &link.link_type, &link.id),
                    &index_data,
                )?;
            }

            destination
                .db
                .persist(PersistMode::SyncAll)
                .map_err(ThingdError::from)
        })();

        if result.is_err() {
            let _ = std::fs::remove_dir_all(destination_path);
        }
        result.map_err(|error: ThingdError| {
            ThingdError::EncryptionMigration(format!("logical database copy failed: {error}"))
        })
    }

    /// Repack a persistent database into a fresh native directory.
    ///
    /// Repacking is an explicit logical migration. It preserves primary
    /// records and their assigned identifiers, versions, sequences, queue
    /// state, links, schemas, migrations, and vectors while discarding journal
    /// history and derived search state.
    pub fn repack_to(
        source_path: impl AsRef<Path>,
        destination_path: impl AsRef<Path>,
        encryption: Option<EncryptionConfig>,
    ) -> ThingdResult<()> {
        Self::repack_to_with_backends(
            source_path,
            destination_path,
            PersistentBackend::RocksDb,
            PersistentBackend::RocksDb,
            encryption,
        )
    }

    /// Repack between explicit durable backends without modifying the source.
    pub fn repack_to_with_backends(
        source_path: impl AsRef<Path>,
        destination_path: impl AsRef<Path>,
        source_backend: PersistentBackend,
        destination_backend: PersistentBackend,
        encryption: Option<EncryptionConfig>,
    ) -> ThingdResult<()> {
        let source_path = source_path.as_ref();
        let destination_path = destination_path.as_ref();
        if source_path == destination_path {
            return Err(ThingdError::InvalidInput(
                "source and destination paths must differ".to_string(),
            ));
        }
        if destination_path.exists() {
            return Err(ThingdError::Conflict(
                "repack destination already exists".to_string(),
            ));
        }
        let parent = destination_path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)
            .map_err(|error| ThingdError::Storage(format!("create repack parent: {error}")))?;
        let temp_path = parent.join(format!(
            ".{}.repack-{}",
            destination_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("thingd"),
            std::process::id()
        ));
        if temp_path.exists() {
            return Err(ThingdError::Conflict(
                "repack temporary destination already exists".to_string(),
            ));
        }

        let source_options = PersistentOpenOptions {
            backend: source_backend,
            encryption,
            search_mode: PersistentSearchMode::Disabled,
            ..PersistentOpenOptions::default()
        };
        let destination_options = PersistentOpenOptions {
            backend: destination_backend,
            encryption: source_options.encryption.clone(),
            search_mode: PersistentSearchMode::Disabled,
            ..PersistentOpenOptions::default()
        };
        let result =
            Self::reencrypt_to(source_path, &temp_path, source_options, destination_options)
                .and_then(|()| {
                    Self::validate_path_with_backend(&temp_path, destination_backend)?;
                    std::fs::rename(&temp_path, destination_path).map_err(|error| {
                        ThingdError::Storage(format!("promote repacked database: {error}"))
                    })
                });
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&temp_path);
        }
        result
    }

    #[cfg(feature = "search")]
    fn search_schema() -> tantivy::schema::Schema {
        let mut schema_builder = tantivy::schema::Schema::builder();
        schema_builder.add_text_field("doc_key", tantivy::schema::STRING | tantivy::schema::STORED);
        schema_builder.add_text_field(
            "collection",
            tantivy::schema::STRING | tantivy::schema::STORED,
        );
        schema_builder.add_text_field("id", tantivy::schema::STRING | tantivy::schema::STORED);
        schema_builder.add_text_field("body", tantivy::schema::TEXT | tantivy::schema::STORED);
        schema_builder.add_text_field("kind", tantivy::schema::STRING | tantivy::schema::STORED);
        schema_builder.build()
    }

    #[cfg(feature = "search")]
    fn create_search_index(in_memory: bool) -> Option<tantivy::Index> {
        let schema = Self::search_schema();
        if in_memory {
            Some(tantivy::Index::create_in_ram(schema))
        } else {
            None
        }
    }

    #[cfg(feature = "search")]
    fn init_search_index(
        path: &Path,
        rebuild_incompatible: bool,
    ) -> ThingdResult<(Option<tantivy::Index>, bool)> {
        let search_dir = path.join("search");
        let marker_path = path.join(".thingd-search-rebuild");

        if marker_path.exists() && !rebuild_incompatible {
            return Ok((None, true));
        }
        if rebuild_incompatible {
            let _ = std::fs::remove_file(&marker_path);
        }

        // A Tantivy index is derived state. If an older SDK wrote an
        // incompatible schema (for example without `doc_key`), discard only
        // that derived directory and rebuild it from durable records. Never let
        // a schema mismatch panic or make the primary database unreadable.
        if let Ok(index) = tantivy::Index::open_in_dir(&search_dir)
            && Self::search_schema_is_compatible(&index.schema())
        {
            return Ok((Some(index), false));
        }

        if !rebuild_incompatible {
            return Ok((None, false));
        }

        if search_dir.exists() {
            std::fs::remove_dir_all(&search_dir)
                .map_err(|e| ThingdError::Storage(format!("replace legacy search index: {e}")))?;
        }
        std::fs::create_dir_all(&search_dir)
            .map_err(|e| ThingdError::Storage(format!("recreate search directory: {e}")))?;

        let schema = Self::search_schema();

        let index = tantivy::Index::create_in_dir(&search_dir, schema)
            .map_err(|e| ThingdError::Storage(format!("create search index: {e}")))?;
        Ok((Some(index), true))
    }

    #[cfg(feature = "search")]
    fn create_search_index_at(path: &Path) -> ThingdResult<tantivy::Index> {
        std::fs::create_dir_all(path)
            .map_err(|error| ThingdError::Storage(format!("create search directory: {error}")))?;
        tantivy::Index::create_in_dir(path, Self::search_schema())
            .map_err(|error| ThingdError::Storage(format!("create search index: {error}")))
    }

    #[cfg(feature = "search")]
    fn promote_search_generation(&mut self) -> ThingdResult<()> {
        let Some(rebuild_path) = self.search_rebuild_path.take() else {
            return Ok(());
        };
        self.stop_search_worker();
        self.search_writer = None;
        self.search_reader = None;
        self.search_index = None;

        let target = self.path.join("search");
        let previous = self.path.join(".search-previous");
        if previous.exists() {
            std::fs::remove_dir_all(&previous).map_err(|error| {
                ThingdError::Storage(format!("remove previous search generation: {error}"))
            })?;
        }
        if target.exists() {
            std::fs::rename(&target, &previous).map_err(|error| {
                ThingdError::Storage(format!("stage previous search generation: {error}"))
            })?;
        }
        std::fs::rename(&rebuild_path, &target)
            .map_err(|error| ThingdError::Storage(format!("promote search generation: {error}")))?;
        if previous.exists() {
            std::fs::remove_dir_all(&previous).map_err(|error| {
                ThingdError::Storage(format!("remove previous search generation: {error}"))
            })?;
        }
        let index = tantivy::Index::open_in_dir(&target).map_err(|error| {
            ThingdError::Storage(format!("reopen promoted search index: {error}"))
        })?;
        self.search_index = Some(index);
        self.search_writer = self
            .search_index
            .as_ref()
            .map(|index| {
                index
                    .writer(SEARCH_WRITER_MEMORY_BYTES)
                    .map(|writer| Arc::new(Mutex::new(writer)))
                    .map_err(|error| ThingdError::Storage(format!("reopen search writer: {error}")))
            })
            .transpose()?;
        self.search_reader = self
            .search_index
            .as_ref()
            .map(|index| {
                index
                    .reader()
                    .map(|reader| Arc::new(Mutex::new(reader)))
                    .map_err(|error| ThingdError::Storage(format!("reopen search reader: {error}")))
            })
            .transpose()?;
        let _ = std::fs::remove_file(self.path.join(".thingd-search-rebuild"));
        Ok(())
    }

    #[cfg(feature = "search")]
    fn search_schema_is_compatible(schema: &tantivy::schema::Schema) -> bool {
        ["doc_key", "collection", "id", "body", "kind"]
            .iter()
            .all(|field| schema.get_field(field).is_ok())
    }

    fn serialize<T: serde::Serialize>(&self, value: &T) -> ThingdResult<Vec<u8>> {
        let data = serde_json::to_vec(value).map_err(|e| ThingdError::Storage(e.to_string()))?;
        self.codec.encode_value("record", &data)
    }

    fn deserialize<T: for<'a> serde::Deserialize<'a>>(&self, bytes: &[u8]) -> ThingdResult<T> {
        let data = self.codec.decode_value("record", bytes)?;
        serde_json::from_slice(&data).map_err(|e| ThingdError::Storage(e.to_string()))
    }

    fn persist_event_metadata(&self, stream: &str) -> ThingdResult<()> {
        let max_sequence = self.event_seq_counters.get(stream).copied().unwrap_or(0);
        let idempotency_keys = self
            .event_idempotency_keys
            .iter()
            .filter_map(|((stored_stream, key), sequence)| {
                (stored_stream == stream).then_some((key.clone(), *sequence))
            })
            .collect();
        let metadata = EventMetadata {
            stream: stream.to_string(),
            max_sequence,
            idempotency_keys,
        };
        let raw = serde_json::to_vec(&metadata)
            .map_err(|error| ThingdError::Storage(error.to_string()))?;
        let encoded = self.codec.encode_value("event_metadata", &raw)?;
        let key = self
            .codec
            .encode_key("event_metadata.stream", stream.as_bytes());
        self.event_meta.insert(&key, &encoded)?;
        Ok(())
    }

    fn persist_all_event_metadata(&self) -> ThingdResult<()> {
        let streams: Vec<String> = self.event_seq_counters.keys().cloned().collect();
        for stream in streams {
            self.persist_event_metadata(&stream)?;
        }
        Ok(())
    }

    fn make_object_key(&self, collection: &str, id: &str) -> Vec<u8> {
        self.codec
            .encode_scoped_key("objects", collection.as_bytes(), id.as_bytes())
    }

    fn make_object_prefix(&self, collection: &str) -> Vec<u8> {
        self.codec
            .encode_scoped_prefix("objects", collection.as_bytes())
    }

    fn make_event_key(&self, stream: &str, sequence: u64) -> Vec<u8> {
        let seq_be = sequence.to_be_bytes();
        let mut key = if self.codec.encrypted() {
            self.codec.encode_key("events.stream", stream.as_bytes())
        } else {
            let mut raw = Vec::with_capacity(stream.len() + 1);
            raw.extend_from_slice(stream.as_bytes());
            raw.push(0);
            raw
        };
        key.extend_from_slice(&seq_be);
        key
    }

    fn make_event_prefix(&self, stream: &str) -> Vec<u8> {
        if self.codec.encrypted() {
            self.codec.encode_key("events.stream", stream.as_bytes())
        } else {
            let mut prefix = stream.as_bytes().to_vec();
            prefix.push(0);
            prefix
        }
    }

    fn make_queue_key(&self, queue: &str, id: &str) -> Vec<u8> {
        self.codec
            .encode_scoped_key("queue_jobs", queue.as_bytes(), id.as_bytes())
    }

    fn make_queue_prefix(&self, queue: &str) -> Vec<u8> {
        self.codec
            .encode_scoped_prefix("queue_jobs", queue.as_bytes())
    }

    /// Ready jobs index key: {`queue}\0{priority_rev:8BE}\0{created_at}\0{id`}
    fn make_ready_key(&self, queue: &str, priority: i32, created_at: &str, id: &str) -> Vec<u8> {
        let priority_rev = (i32::MAX - priority).to_be_bytes();
        if self.codec.encrypted() {
            let mut key = self.codec.encode_key("ready_jobs.queue", queue.as_bytes());
            key.extend_from_slice(&priority_rev);
            key.extend_from_slice(created_at.as_bytes());
            key.extend_from_slice(&self.codec.encode_key("ready_jobs.id", id.as_bytes()));
            return key;
        }
        let mut key = Vec::new();
        key.extend_from_slice(queue.as_bytes());
        key.push(b'\0');
        key.extend_from_slice(&priority_rev);
        key.push(b'\0');
        key.extend_from_slice(created_at.as_bytes());
        key.push(b'\0');
        key.extend_from_slice(id.as_bytes());
        key
    }

    fn make_ready_prefix(&self, queue: &str) -> Vec<u8> {
        if self.codec.encrypted() {
            self.codec.encode_key("ready_jobs.queue", queue.as_bytes())
        } else {
            let mut prefix = queue.as_bytes().to_vec();
            prefix.push(0);
            prefix
        }
    }

    /// Lease index key ordered by queue, expiration, and job id.
    fn make_lease_key(&self, queue: &str, expires_at_ms: i64, id: &str) -> Vec<u8> {
        let expiry = u64::try_from(expires_at_ms)
            .unwrap_or_default()
            .to_be_bytes();
        if self.codec.encrypted() {
            let mut key = self.codec.encode_key("lease_jobs.queue", queue.as_bytes());
            key.extend_from_slice(&expiry);
            key.extend_from_slice(&self.codec.encode_key("lease_jobs.id", id.as_bytes()));
            key
        } else {
            let mut key = queue.as_bytes().to_vec();
            key.push(0);
            key.extend_from_slice(&expiry);
            key.push(0);
            key.extend_from_slice(id.as_bytes());
            key
        }
    }

    fn make_lease_prefix(&self, queue: &str) -> Vec<u8> {
        if self.codec.encrypted() {
            self.codec.encode_key("lease_jobs.queue", queue.as_bytes())
        } else {
            let mut prefix = queue.as_bytes().to_vec();
            prefix.push(0);
            prefix
        }
    }

    fn lease_expiration(&self, queue: &str, key: &[u8]) -> ThingdResult<i64> {
        let prefix = self.make_lease_prefix(queue);
        let bytes = key
            .get(prefix.len()..prefix.len().saturating_add(8))
            .ok_or_else(|| ThingdError::Storage("malformed queue lease index key".to_string()))?;
        let millis =
            u64::from_be_bytes(bytes.try_into().map_err(|_| {
                ThingdError::Storage("malformed queue lease expiration".to_string())
            })?);
        i64::try_from(millis)
            .map_err(|_| ThingdError::Storage("queue lease expiration overflows i64".to_string()))
    }

    #[cfg(feature = "vectors")]
    fn make_vector_key(&self, collection: &str, id: &str) -> Vec<u8> {
        self.codec
            .encode_scoped_key("vectors", collection.as_bytes(), id.as_bytes())
    }

    #[cfg(feature = "vectors")]
    fn make_vector_prefix(&self, collection: &str) -> Vec<u8> {
        self.codec
            .encode_scoped_prefix("vectors", collection.as_bytes())
    }

    fn make_link_id_key(&self, link_id: &str) -> Vec<u8> {
        self.codec.encode_key("links_by_id", link_id.as_bytes())
    }

    fn make_link_from_key(&self, from_ref: &str, link_type: &str, link_id: &str) -> Vec<u8> {
        let suffix = format!("{link_type}\0{link_id}");
        self.codec
            .encode_scoped_key("links_from", from_ref.as_bytes(), suffix.as_bytes())
    }

    fn make_link_from_prefix(&self, from_ref: &str) -> Vec<u8> {
        self.codec
            .encode_scoped_prefix("links_from", from_ref.as_bytes())
    }

    fn make_link_to_key(&self, to_ref: &str, link_type: &str, link_id: &str) -> Vec<u8> {
        let suffix = format!("{link_type}\0{link_id}");
        self.codec
            .encode_scoped_key("links_to", to_ref.as_bytes(), suffix.as_bytes())
    }

    fn make_link_to_prefix(&self, to_ref: &str) -> Vec<u8> {
        self.codec
            .encode_scoped_prefix("links_to", to_ref.as_bytes())
    }

    fn make_schema_key(&self) -> Vec<u8> {
        self.codec.encode_key("schemas.current", b"current")
    }

    fn make_migration_key(&self, id: &str) -> Vec<u8> {
        self.codec.encode_key("migrations.id", id.as_bytes())
    }

    fn make_index_key(&self, collection: &str, field: &str) -> Vec<u8> {
        let mut value = collection.as_bytes().to_vec();
        value.push(0);
        value.extend_from_slice(field.as_bytes());
        self.codec.encode_key("indexes.definition", &value)
    }

    fn unique_cache_key(
        collection: &str,
        field: &str,
        value: &serde_json::Value,
    ) -> ThingdResult<(String, String, String)> {
        let value = serde_json::to_string(value)
            .map_err(|error| ThingdError::Storage(error.to_string()))?;
        Ok((collection.to_string(), field.to_string(), value))
    }

    fn build_unique_index_cache(
        objects: &Keyspace,
        indexes: &Keyspace,
        codec: &dyn StorageCodec,
    ) -> ThingdResult<(UniqueIndexCache, bool)> {
        let mut unique_indexes = Vec::new();
        for entry in indexes.iter() {
            let (_, value) = guard_data(entry)?;
            let decoded = codec.decode_value("record", &value)?;
            let index: IndexDefinition = serde_json::from_slice(&decoded)
                .map_err(|error| ThingdError::Storage(error.to_string()))?;
            if index.unique {
                unique_indexes.push(index);
            }
        }
        if unique_indexes.is_empty() {
            return Ok((HashMap::new(), true));
        }

        let mut cache = HashMap::new();
        let mut complete = true;
        for entry in objects.iter() {
            let (_, value) = guard_data(entry)?;
            let decoded = codec.decode_value("record", &value)?;
            let object: MemoryObject = serde_json::from_slice(&decoded)
                .map_err(|error| ThingdError::Storage(error.to_string()))?;
            let Ok(body) = serde_json::from_str::<serde_json::Value>(&object.body) else {
                complete = false;
                continue;
            };
            for index in unique_indexes
                .iter()
                .filter(|index| index.collection == object.key.collection)
            {
                let Some(value) = body.get(&index.field).filter(|value| !value.is_null()) else {
                    continue;
                };
                let key = Self::unique_cache_key(&index.collection, &index.field, value)?;
                if cache.insert(key, object.key.clone()).is_some() {
                    complete = false;
                }
            }
        }
        Ok((cache, complete))
    }

    fn refresh_unique_index_cache(&mut self) -> ThingdResult<()> {
        let (cache, complete) =
            Self::build_unique_index_cache(&self.objects, &self.indexes, self.codec.as_ref())?;
        self.unique_index_values = cache;
        self.unique_index_cache_complete = complete;
        Ok(())
    }

    fn update_unique_index_cache_for_object(
        &mut self,
        object: &MemoryObject,
        previous: Option<&MemoryObject>,
    ) -> ThingdResult<()> {
        if !self.unique_index_cache_complete {
            return Ok(());
        }
        let indexes: Vec<IndexDefinition> = self
            .list_index_definitions()?
            .into_iter()
            .filter(|index| index.unique && index.collection == object.key.collection)
            .collect();
        for index in indexes {
            if let Some(previous) = previous
                && let Ok(body) = serde_json::from_str::<serde_json::Value>(&previous.body)
                && let Some(value) = body.get(&index.field).filter(|value| !value.is_null())
            {
                let key = Self::unique_cache_key(&index.collection, &index.field, value)?;
                self.unique_index_values.remove(&key);
            }
            if let Ok(body) = serde_json::from_str::<serde_json::Value>(&object.body)
                && let Some(value) = body.get(&index.field).filter(|value| !value.is_null())
            {
                let key = Self::unique_cache_key(&index.collection, &index.field, value)?;
                self.unique_index_values.insert(key, object.key.clone());
            }
        }
        Ok(())
    }

    fn remove_unique_index_cache_for_object(&mut self, object: &MemoryObject) -> ThingdResult<()> {
        if !self.unique_index_cache_complete {
            return Ok(());
        }
        let indexes: Vec<IndexDefinition> = self
            .list_index_definitions()?
            .into_iter()
            .filter(|index| index.unique && index.collection == object.key.collection)
            .collect();
        if let Ok(body) = serde_json::from_str::<serde_json::Value>(&object.body) {
            for index in indexes {
                if let Some(value) = body.get(&index.field).filter(|value| !value.is_null()) {
                    let key = Self::unique_cache_key(&index.collection, &index.field, value)?;
                    self.unique_index_values.remove(&key);
                }
            }
        }
        Ok(())
    }

    fn validate_unique_indexes(&self, object: &MemoryObject) -> ThingdResult<()> {
        let indexes: Vec<IndexDefinition> = self
            .list_index_definitions()?
            .into_iter()
            .filter(|index| index.unique && index.collection == object.key.collection)
            .collect();
        if indexes.is_empty() {
            return Ok(());
        }

        let body = serde_json::from_str::<serde_json::Value>(&object.body)
            .map_err(|error| ThingdError::InvalidInput(format!("invalid object JSON: {error}")))?;
        let existing_bodies = if self.unique_index_cache_complete {
            None
        } else {
            let objects = self.list_objects(
                Some(std::slice::from_ref(&object.key.collection)),
                &ListObjectsOptions::default(),
            )?;
            Some(
                objects
                    .into_iter()
                    .filter_map(|existing| {
                        serde_json::from_str::<serde_json::Value>(&existing.body)
                            .ok()
                            .map(|existing_body| (existing.key, existing_body))
                    })
                    .collect::<Vec<_>>(),
            )
        };

        for index in indexes {
            let Some(value) = body.get(&index.field) else {
                continue;
            };
            if value.is_null() {
                continue;
            }
            if self.unique_index_cache_complete {
                let key = Self::unique_cache_key(&index.collection, &index.field, value)?;
                if let Some(existing_key) = self.unique_index_values.get(&key)
                    && existing_key != &object.key
                {
                    return Err(ThingdError::Conflict(format!(
                        "unique index {}.{} rejects duplicate value",
                        index.collection, index.field
                    )));
                }
                continue;
            }
            if existing_bodies.as_ref().is_some_and(|existing_bodies| {
                existing_bodies.iter().any(|(existing_key, existing_body)| {
                    existing_key != &object.key
                        && existing_body
                            .get(&index.field)
                            .is_some_and(|existing_value| existing_value == value)
                })
            }) {
                return Err(ThingdError::Conflict(format!(
                    "unique index {}.{} rejects duplicate value",
                    index.collection, index.field
                )));
            }
        }
        Ok(())
    }
}

fn guard_data(kv: Guard) -> ThingdResult<(Vec<u8>, Vec<u8>)> {
    let kv = kv.into_inner()?;
    let key = kv.0.to_vec();
    let val = kv.1.to_vec();
    Ok((key, val))
}

fn timestamp_before(value: &str, cutoff_ms: i64) -> bool {
    chrono::DateTime::parse_from_rfc3339(value)
        .is_ok_and(|timestamp| timestamp.timestamp_millis() < cutoff_ms)
}

impl crate::SchemaStore for PersistentEngine {
    fn get_schema_document(&self) -> ThingdResult<Option<StoredSchema>> {
        let key = self.make_schema_key();
        value_to_vec(self.schemas.get(&key)?)
            .map(|value| self.deserialize(&value))
            .transpose()
    }

    fn put_schema_document(&mut self, schema: StoredSchema) -> ThingdResult<()> {
        let key = self.make_schema_key();
        let value = self.serialize(&schema)?;
        self.schemas.insert(key, value)?;
        Ok(())
    }

    fn list_migrations(&self) -> ThingdResult<Vec<MigrationRecord>> {
        let mut migrations = Vec::new();
        for entry in self.migrations.iter() {
            let (_, value) = guard_data(entry)?;
            migrations.push(self.deserialize(&value)?);
        }
        migrations.sort_by(|left: &MigrationRecord, right| left.id.cmp(&right.id));
        Ok(migrations)
    }

    fn record_migration(&mut self, migration: MigrationRecord) -> ThingdResult<()> {
        let key = self.make_migration_key(&migration.id);
        let value = self.serialize(&migration)?;
        self.migrations.insert(key, value)?;
        Ok(())
    }
}

impl PersistentEngine {
    fn safe_replication_cursor(&self) -> ThingdResult<Option<u64>> {
        let mut minimum: Option<u64> = None;
        for entry in self.objects.iter() {
            let (_, value) = guard_data(entry)?;
            let object: MemoryObject = self.deserialize(&value)?;
            if object.key.collection != REPLICATION_STATE_COLLECTION {
                continue;
            }
            let Some(cursor) = serde_json::from_str::<serde_json::Value>(&object.body)
                .ok()
                .and_then(|body| body.get("lastAppliedCursor").and_then(Value::as_u64))
            else {
                continue;
            };
            minimum = Some(minimum.map_or(cursor, |current| current.min(cursor)));
        }
        Ok(minimum)
    }
}

// ── ObjectStore ──────────────────────────────────────────────────────────────

impl ObjectStore for PersistentEngine {
    fn put_object(&mut self, mut object: MemoryObject) -> ThingdResult<MemoryObject> {
        self.validate_unique_indexes(&object)?;
        let key = self.make_object_key(&object.key.collection, &object.key.id);
        let previous = value_to_vec(self.objects.get(&key)?)
            .map(|data| self.deserialize::<MemoryObject>(&data))
            .transpose()?;

        if object.created_at.is_empty() {
            object.created_at = now_iso_string();
        }
        object.updated_at = now_iso_string();

        if let Some(existing_obj) = previous.as_ref() {
            object.version = existing_obj.version + 1;
            object.created_at.clone_from(&existing_obj.created_at);
        } else {
            object.version = 1;
        }

        let data = self.serialize(&object)?;

        // Atomic batch: object data + vector state
        let mut batch = self.db.batch();
        batch.insert(&self.objects, &key, &data);
        #[cfg(feature = "vectors")]
        {
            let vkey = self.make_vector_key(&object.key.collection, &object.key.id);
            if let Some(ref vector) = object.vector {
                let vdata = self.serialize(&StoredVector {
                    collection: object.key.collection.clone(),
                    id: object.key.id.clone(),
                    vector: vector.clone(),
                })?;
                batch.insert(&self.vectors, &vkey, vdata);
            } else {
                batch.remove(&self.vectors, vkey);
            }
        }
        batch
            .commit()
            .map_err(|e| ThingdError::Storage(e.to_string()))?;

        #[cfg(feature = "search")]
        self.record_search_mutation(
            format!("object:{}/{}", object.key.collection, object.key.id),
            SearchReplayMutation::UpsertObject,
        );
        #[cfg(feature = "search")]
        self.index_object_for_search(&object);

        self.update_unique_index_cache_for_object(&object, previous.as_ref())?;

        Ok(object)
    }

    fn put_object_with_options(
        &mut self,
        object: MemoryObject,
        options: PutObjectOptions,
    ) -> ThingdResult<MemoryObject> {
        let key = self.make_object_key(&object.key.collection, &object.key.id);

        if let Some(expected_version) = options.expected_version {
            match value_to_vec(self.objects.get(&key)?) {
                Some(existing) => {
                    let existing_obj: MemoryObject = self.deserialize(&existing)?;
                    if existing_obj.version != expected_version {
                        return Err(ThingdError::Conflict(format!(
                            "expected version {} but current version is {}",
                            expected_version, existing_obj.version
                        )));
                    }
                },
                None => {
                    return Err(ThingdError::Conflict(format!(
                        "object '{}/{}' does not exist",
                        object.key.collection, object.key.id
                    )));
                },
            }
        }

        self.put_object(object)
    }

    fn put_object_with_source_metadata(
        &mut self,
        object: MemoryObject,
        options: PutObjectOptions,
    ) -> ThingdResult<MemoryObject> {
        let key = self.make_object_key(&object.key.collection, &object.key.id);

        if let Some(expected_version) = options.expected_version {
            match value_to_vec(self.objects.get(&key)?) {
                Some(existing) => {
                    let existing_obj: MemoryObject = self.deserialize(&existing)?;
                    if existing_obj.version != expected_version {
                        return Err(ThingdError::Conflict(format!(
                            "expected version {} but current version is {}",
                            expected_version, existing_obj.version
                        )));
                    }
                },
                None => {
                    return Err(ThingdError::Conflict(format!(
                        "object '{}/{}' does not exist",
                        object.key.collection, object.key.id
                    )));
                },
            }
        }

        // Use the normal write path for validation, vectors, indexes, and
        // derived search state, then restore the metadata supplied by the
        // replication source without changing local write semantics.
        let stored = self.put_object(object.clone())?;
        let mut replicated = stored;
        if object.version > 0 {
            replicated.version = object.version;
        }
        if !object.created_at.is_empty() {
            replicated.created_at = object.created_at;
        }
        if !object.updated_at.is_empty() {
            replicated.updated_at = object.updated_at;
        }

        let data = self.serialize(&replicated)?;
        let mut batch = self.db.batch();
        batch.insert(&self.objects, &key, &data);
        batch
            .commit()
            .map_err(|error| ThingdError::Storage(error.to_string()))?;

        Ok(replicated)
    }

    fn retain(&mut self, options: RetentionOptions) -> ThingdResult<RetentionReport> {
        let safe_replication_cursor = if options.include_replication {
            self.safe_replication_cursor()?
        } else {
            None
        };
        let mut report = RetentionReport {
            dry_run: options.dry_run,
            safe_replication_cursor,
            ..RetentionReport::default()
        };
        let mut event_deletions = Vec::new();
        for entry in self.events.iter() {
            let (key, value) = guard_data(entry)?;
            let event: MemoryEvent = self.deserialize(&value)?;
            let old = timestamp_before(&event.created_at, options.before_unix_ms);
            let eligible = if event.stream == REPLICATION_STREAM {
                if old
                    && options.include_replication
                    && safe_replication_cursor.is_some_and(|cursor| event.sequence <= cursor)
                {
                    true
                } else if old {
                    report.skipped_replication_events += 1;
                    false
                } else {
                    false
                }
            } else {
                !self.is_protected_stream(&event.stream) && old
            };
            if eligible {
                report.events += 1;
                event_deletions.push((key, event));
            }
        }

        let mut job_deletions = Vec::new();
        for entry in self.queue_jobs.iter() {
            let (key, value) = guard_data(entry)?;
            let job: QueueJob = self.deserialize(&value)?;
            let eligible = match job.status {
                QueueJobStatus::Completed => job
                    .completed_at_ms
                    .is_some_and(|timestamp| timestamp < options.before_unix_ms),
                QueueJobStatus::Dead => job
                    .dead_at_ms
                    .is_some_and(|timestamp| timestamp < options.before_unix_ms),
                _ => false,
            };
            if eligible {
                match job.status {
                    QueueJobStatus::Completed => report.completed_jobs += 1,
                    QueueJobStatus::Dead => report.dead_jobs += 1,
                    _ => unreachable!(),
                }
                job_deletions.push((key, job));
            }
        }

        if options.dry_run {
            return Ok(report);
        }

        let mut batch = self.db.batch();
        for (key, _) in &event_deletions {
            batch.remove(&self.events, key.clone());
        }
        for (key, _) in &job_deletions {
            batch.remove(&self.queue_jobs, key.clone());
        }
        if !event_deletions.is_empty() || !job_deletions.is_empty() {
            batch
                .commit()
                .map_err(|error| ThingdError::Storage(error.to_string()))?;
        }

        let mut affected_streams = std::collections::HashSet::new();
        #[cfg(feature = "search")]
        let mut search_deletions = Vec::new();
        for (_, event) in &event_deletions {
            affected_streams.insert(event.stream.clone());
            self.event_idempotency_keys.retain(|(stream, _), sequence| {
                stream != &event.stream || *sequence != event.sequence
            });
            #[cfg(feature = "search")]
            search_deletions.push((event.stream.clone(), event.sequence));
        }
        #[cfg(feature = "search")]
        self.delete_events_from_search_index(&search_deletions);
        for stream in affected_streams {
            self.persist_event_metadata(&stream)?;
        }
        if options.compact && (!event_deletions.is_empty() || !job_deletions.is_empty()) {
            self.events
                .major_compact()
                .map_err(|error| ThingdError::Storage(error.to_string()))?;
            self.queue_jobs
                .major_compact()
                .map_err(|error| ThingdError::Storage(error.to_string()))?;
            report.compacted = true;
        }
        Ok(report)
    }

    fn get_object(&self, collection: &str, id: &str) -> ThingdResult<Option<MemoryObject>> {
        let key = self.make_object_key(collection, id);
        match value_to_vec(self.objects.get(&key)?) {
            Some(data) => {
                let started = std::time::Instant::now();
                let result = self.deserialize(&data);
                self.db.record_ram_deserialization(
                    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
                );
                Ok(Some(result?))
            },
            None => Ok(None),
        }
    }

    fn get_objects_batch(
        &self,
        collection: &str,
        ids: &[String],
    ) -> ThingdResult<Vec<Option<MemoryObject>>> {
        ids.iter()
            .map(|id| self.get_object(collection, id))
            .collect()
    }

    fn list_objects(
        &self,
        collections: Option<&[String]>,
        options: &ListObjectsOptions,
    ) -> ThingdResult<Vec<MemoryObject>> {
        // Avoid materializing the whole collection when the caller requests
        // the natural key order. Sorting still requires the full candidate set.
        if options.sort_by.is_none() {
            let offset = options.offset.unwrap_or(0);
            let limit = options.limit.unwrap_or(u64::MAX);
            let mut skipped = 0u64;
            let mut results = Vec::new();

            if let Some(collections) = collections
                && collections.len() == 1
            {
                let prefix = self.make_object_prefix(&collections[0]);
                for kv in self.objects.prefix(&prefix) {
                    let (_, value) = guard_data(kv)?;
                    let object: MemoryObject = self.deserialize(&value)?;
                    if !matches_object_filters(&object, &options.filter) {
                        continue;
                    }
                    if skipped < offset {
                        skipped += 1;
                        continue;
                    }
                    results.push(object);
                    if results.len() as u64 >= limit {
                        break;
                    }
                }
            } else {
                for kv in self.objects.iter() {
                    let (_, value) = guard_data(kv)?;
                    let object: MemoryObject = self.deserialize(&value)?;
                    if let Some(collections) = collections
                        && !collections.contains(&object.key.collection)
                    {
                        continue;
                    }
                    if !matches_object_filters(&object, &options.filter) {
                        continue;
                    }
                    if skipped < offset {
                        skipped += 1;
                        continue;
                    }
                    results.push(object);
                    if results.len() as u64 >= limit {
                        break;
                    }
                }
            }
            return Ok(results);
        }

        let prefix = if let Some(collections) = collections
            && collections.len() == 1
        {
            Some(self.make_object_prefix(&collections[0]))
        } else {
            None
        };

        let mut objects: Vec<MemoryObject> = if let Some(ref prefix) = prefix {
            let mut objs = Vec::new();
            for kv in self.objects.prefix(prefix) {
                let (_, value) = guard_data(kv)?;
                objs.push(self.deserialize(&value)?);
            }
            objs
        } else {
            let mut objs = Vec::new();
            for kv in self.objects.iter() {
                let (_, value) = guard_data(kv)?;
                objs.push(self.deserialize(&value)?);
            }
            objs
        };

        if let Some(cols) = collections
            && cols.len() != 1
        {
            objects.retain(|o| cols.contains(&o.key.collection));
        }

        if !options.filter.is_empty() {
            objects.retain(|object| matches_object_filters(object, &options.filter));
        }

        if let Some(ref sort_by) = options.sort_by {
            let asc = sort_by.direction == SortDirection::Asc;
            objects.sort_by(|a, b| {
                let cmp = if sort_by.field.starts_with("$.") {
                    let path = sort_by.field.trim_start_matches('$');
                    let a_val = serde_json::from_str::<serde_json::Value>(&a.body)
                        .ok()
                        .and_then(|v| v.get(path).cloned());
                    let b_val = serde_json::from_str::<serde_json::Value>(&b.body)
                        .ok()
                        .and_then(|v| v.get(path).cloned());
                    match (&a_val, &b_val) {
                        (Some(a), Some(b)) => value_compare(a, b),
                        (Some(_), None) => std::cmp::Ordering::Greater,
                        (None, Some(_)) => std::cmp::Ordering::Less,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                } else {
                    match sort_by.field.as_str() {
                        "id" => a.key.id.cmp(&b.key.id),
                        "collection" => a.key.collection.cmp(&b.key.collection),
                        "created_at" => a.created_at.cmp(&b.created_at),
                        "updated_at" => a.updated_at.cmp(&b.updated_at),
                        "version" => a.version.cmp(&b.version),
                        _ => std::cmp::Ordering::Equal,
                    }
                };
                if asc { cmp } else { cmp.reverse() }
            });
        }

        if let Some(offset) = options.offset {
            let skip = usize::try_from(offset).unwrap_or(usize::MAX);
            objects = objects.into_iter().skip(skip).collect();
        }
        if let Some(limit) = options.limit {
            let take = usize::try_from(limit).unwrap_or(usize::MAX);
            objects.truncate(take);
        }

        Ok(objects)
    }

    fn delete_object(&mut self, collection: &str, id: &str) -> ThingdResult<bool> {
        let key = self.make_object_key(collection, id);
        let existing = value_to_vec(self.objects.get(&key)?)
            .map(|data| self.deserialize::<MemoryObject>(&data))
            .transpose()?;
        let existed = existing.is_some();

        // Atomic batch: object + vector removal
        let mut batch = self.db.batch();
        batch.remove(&self.objects, key);
        #[cfg(feature = "vectors")]
        {
            let vkey = self.make_vector_key(collection, id);
            batch.remove(&self.vectors, vkey);
        }
        batch
            .commit()
            .map_err(|e| ThingdError::Storage(e.to_string()))?;

        #[cfg(feature = "search")]
        self.record_search_mutation(
            format!("object:{collection}/{id}"),
            SearchReplayMutation::DeleteObject,
        );
        #[cfg(feature = "search")]
        self.enqueue_delete_object_for_search(collection, id);
        if let Some(existing) = existing.as_ref() {
            self.remove_unique_index_cache_for_object(existing)?;
        }
        Ok(existed)
    }

    fn delete_objects_batch(&mut self, keys: &[(String, String)]) -> ThingdResult<u64> {
        let mut count = 0u64;
        let mut batch = self.db.batch();
        let mut deleted = Vec::new();

        for (collection, id) in keys {
            let key = self.make_object_key(collection, id);
            if self.objects.get(&key)?.is_some() {
                if let Some(data) = value_to_vec(self.objects.get(&key)?) {
                    deleted.push(self.deserialize::<MemoryObject>(&data)?);
                }
                batch.remove(&self.objects, key);
                count += 1;
            }
            #[cfg(feature = "vectors")]
            {
                let vkey = self.make_vector_key(collection, id);
                batch.remove(&self.vectors, vkey);
            }
        }

        if count > 0 {
            batch
                .commit()
                .map_err(|e| ThingdError::Storage(e.to_string()))?;
        }

        #[cfg(feature = "search")]
        for (collection, id) in keys {
            self.record_search_mutation(
                format!("object:{collection}/{id}"),
                SearchReplayMutation::DeleteObject,
            );
            self.enqueue_delete_object_for_search(collection, id);
        }
        for object in &deleted {
            self.remove_unique_index_cache_for_object(object)?;
        }

        Ok(count)
    }

    fn count_objects(&self) -> ThingdResult<u64> {
        let mut count = 0u64;
        for kv in self.objects.iter() {
            let _ = kv;
            count += 1;
        }
        Ok(count)
    }

    fn count_objects_in_collection(&self, collection: &str) -> ThingdResult<u64> {
        let prefix = self.make_object_prefix(collection);
        let mut count = 0u64;
        for kv in self.objects.prefix(&prefix) {
            let _ = kv;
            count += 1;
        }
        Ok(count)
    }

    fn list_collections(&self) -> ThingdResult<Vec<String>> {
        let mut collections: Vec<String> = Vec::new();
        for kv in self.objects.iter() {
            let (_, value) = guard_data(kv)?;
            let object: MemoryObject = self.deserialize(&value)?;
            if !collections.contains(&object.key.collection) {
                collections.push(object.key.collection);
            }
        }
        Ok(collections)
    }

    fn create_index(&mut self, collection: &str, field: &str) -> ThingdResult<()> {
        self.create_index_definition(IndexDefinition {
            collection: collection.to_string(),
            field: field.to_string(),
            unique: false,
        })
    }

    fn create_index_definition(&mut self, index: IndexDefinition) -> ThingdResult<()> {
        if index.collection.is_empty() || index.field.is_empty() {
            return Err(ThingdError::InvalidInput(
                "index collection and field are required".to_string(),
            ));
        }
        if index.unique {
            let objects = self.list_objects(
                Some(std::slice::from_ref(&index.collection)),
                &ListObjectsOptions::default(),
            )?;
            let mut values = Vec::new();
            for object in objects {
                let body =
                    serde_json::from_str::<serde_json::Value>(&object.body).map_err(|error| {
                        ThingdError::InvalidInput(format!("invalid object JSON: {error}"))
                    })?;
                if let Some(value) = body.get(&index.field).filter(|value| !value.is_null()) {
                    if values.iter().any(|existing| existing == value) {
                        return Err(ThingdError::Conflict(format!(
                            "cannot create unique index {}.{}: existing values are duplicated",
                            index.collection, index.field
                        )));
                    }
                    values.push(value.clone());
                }
            }
        }
        self.indexes.insert(
            self.make_index_key(&index.collection, &index.field),
            self.serialize(&index)?,
        )?;
        self.refresh_unique_index_cache()?;
        Ok(())
    }

    fn list_indexes(&self) -> ThingdResult<Vec<(String, String)>> {
        Ok(self
            .list_index_definitions()?
            .into_iter()
            .map(|index| (index.collection, index.field))
            .collect())
    }

    fn delete_index(&mut self, collection: &str, field: &str) -> ThingdResult<bool> {
        let key = self.make_index_key(collection, field);
        let existed = self.indexes.get(&key)?.is_some();
        if existed {
            self.indexes.remove(key)?;
            self.refresh_unique_index_cache()?;
        }
        Ok(existed)
    }

    fn list_index_definitions(&self) -> ThingdResult<Vec<IndexDefinition>> {
        let mut definitions = Vec::new();
        for entry in self.indexes.iter() {
            let (_, value) = guard_data(entry)?;
            definitions.push(self.deserialize(&value)?);
        }
        definitions.sort_by(|left: &IndexDefinition, right: &IndexDefinition| {
            (&left.collection, &left.field).cmp(&(&right.collection, &right.field))
        });
        Ok(definitions)
    }

    fn schema(
        &self,
        collection: Option<&str>,
        options: &SchemaOptions,
    ) -> ThingdResult<Vec<CollectionSchema>> {
        let sample_size = options.sample_size.unwrap_or(50);
        let mut schemas: Vec<CollectionSchema> = Vec::new();

        let collections: Vec<String> = if let Some(c) = collection {
            vec![c.to_string()]
        } else {
            self.list_collections()?
        };

        for col in collections {
            let prefix = self.make_object_prefix(&col);
            let mut objects: Vec<MemoryObject> = Vec::new();
            let mut count = 0u64;
            for kv in self.objects.prefix(&prefix) {
                count += 1;
                if objects.len() < sample_size {
                    let (_, value) = guard_data(kv)?;
                    objects.push(self.deserialize(&value)?);
                }
            }

            let object_count = count;
            let mut fields: Vec<FieldSchema> = Vec::new();
            let mut field_types: HashMap<String, (String, bool, Vec<serde_json::Value>)> =
                HashMap::new();

            for obj in &objects {
                if let Ok(body) = serde_json::from_str::<serde_json::Value>(&obj.body)
                    && let serde_json::Value::Object(map) = body
                {
                    for (field_name, field_val) in map {
                        let entry = field_types
                            .entry(field_name.clone())
                            .or_insert_with(|| ("unknown".into(), false, Vec::new()));
                        let t = infer_json_type(&field_val);
                        if entry.0 == "unknown" {
                            entry.0 = t.clone();
                        } else if entry.0 != t {
                            entry.0 = "string".into();
                        }
                        if !field_val.is_null() && entry.2.len() < 3 {
                            entry.2.push(field_val.clone());
                        }
                    }
                }
            }

            for (name, (field_type, _, sample_values)) in field_types {
                fields.push(FieldSchema {
                    name,
                    field_type,
                    nullable: false,
                    sample_values,
                });
            }

            fields.sort_by(|a, b| a.name.cmp(&b.name));

            schemas.push(CollectionSchema {
                name: col,
                object_count,
                fields,
            });
        }

        Ok(schemas)
    }
}

// ── EventLog ─────────────────────────────────────────────────────────────────

impl EventLog for PersistentEngine {
    fn is_protected_stream(&self, stream: &str) -> bool {
        stream.starts_with("__thingd:")
    }

    fn append_event(&mut self, mut event: MemoryEvent) -> ThingdResult<MemoryEvent> {
        if !event.idempotency_key.is_empty() {
            let idem_key = (event.stream.clone(), event.idempotency_key.clone());
            if let Some(&existing_seq) = self.event_idempotency_keys.get(&idem_key) {
                let ekey = self.make_event_key(&event.stream, existing_seq);
                if let Some(data) = value_to_vec(self.events.get(&ekey)?) {
                    let existing: MemoryEvent = self.deserialize(&data)?;
                    return Ok(existing);
                }
            }
        }

        let seq = self
            .event_seq_counters
            .entry(event.stream.clone())
            .and_modify(|s| *s += 1)
            .or_insert(1);
        event.sequence = *seq;

        if event.created_at.is_empty() {
            event.created_at = now_iso_string();
        }

        let ekey = self.make_event_key(&event.stream, event.sequence);
        let data = self.serialize(&event)?;
        self.events.insert(&ekey, &data)?;

        #[cfg(feature = "search")]
        self.record_search_mutation(
            format!("event:{}/{}", event.stream, event.sequence),
            SearchReplayMutation::UpsertEvent,
        );
        #[cfg(feature = "search")]
        self.index_event_for_search(&event);

        if !event.idempotency_key.is_empty() {
            self.event_idempotency_keys.insert(
                (event.stream.clone(), event.idempotency_key.clone()),
                event.sequence,
            );
        }

        self.persist_event_metadata(&event.stream)?;

        Ok(event)
    }

    fn append_events_batch(&mut self, events: Vec<MemoryEvent>) -> ThingdResult<Vec<MemoryEvent>> {
        let mut results = Vec::with_capacity(events.len());
        for event in events {
            results.push(self.append_event(event)?);
        }
        Ok(results)
    }

    fn list_events(
        &self,
        stream: Option<&str>,
        options: ListEventsOptions,
    ) -> ThingdResult<Vec<MemoryEvent>> {
        let mut results: Vec<MemoryEvent> = Vec::new();

        if let Some(stream_name) = stream {
            for kv in self.events.prefix(self.make_event_prefix(stream_name)) {
                let (_, value) = guard_data(kv)?;
                let event: MemoryEvent = self.deserialize(&value)?;
                if event.stream != stream_name {
                    continue;
                }
                if let Some(from_seq) = options.from_sequence
                    && event.sequence <= from_seq
                {
                    continue;
                }
                if let Some(ref since) = options.since
                    && event.created_at.as_str() < since.as_str()
                {
                    continue;
                }
                results.push(event);
                if let Some(limit) = options.limit
                    && results.len() as u64 >= limit
                {
                    break;
                }
            }
        } else {
            for kv in self.events.iter() {
                let (_, value) = guard_data(kv)?;
                let event: MemoryEvent = self.deserialize(&value)?;
                if let Some(ref since) = options.since
                    && event.created_at.as_str() < since.as_str()
                {
                    continue;
                }
                results.push(event);
                if let Some(limit) = options.limit
                    && results.len() as u64 >= limit
                {
                    break;
                }
            }
        }

        Ok(results)
    }

    fn delete_last_event(&mut self, stream: &str) -> ThingdResult<Option<MemoryEvent>> {
        if self.is_protected_stream(stream) {
            return Err(ThingdError::Protected(format!(
                "stream '{stream}' is protected and cannot be modified"
            )));
        }

        let mut last_key: Option<Vec<u8>> = None;
        let mut last_event: Option<MemoryEvent> = None;

        for kv in self.events.iter() {
            let (key, value) = guard_data(kv)?;
            let event: MemoryEvent = self.deserialize(&value)?;
            if event.stream == stream
                && last_event
                    .as_ref()
                    .is_none_or(|last| event.sequence > last.sequence)
            {
                last_key = Some(key);
                last_event = Some(event);
            }
        }

        if let Some(key) = last_key {
            self.events.remove(&key)?;
            if let Some(ref ev) = last_event {
                self.event_idempotency_keys
                    .retain(|(stored_stream, _), sequence| {
                        stored_stream != stream || *sequence != ev.sequence
                    });
            }
            #[cfg(feature = "search")]
            if let Some(ref ev) = last_event {
                self.record_search_mutation(
                    format!("event:{}/{}", ev.stream, ev.sequence),
                    SearchReplayMutation::DeleteEvent,
                );
                self.enqueue_delete_event_for_search(&ev.stream, ev.sequence);
            }
            self.persist_event_metadata(stream)?;
            Ok(last_event)
        } else {
            Ok(None)
        }
    }

    fn delete_stream(&mut self, stream: &str) -> ThingdResult<u64> {
        if self.is_protected_stream(stream) {
            return Err(ThingdError::Protected(format!(
                "stream '{stream}' is protected and cannot be modified"
            )));
        }

        let mut entries: Vec<(Vec<u8>, u64)> = Vec::new();
        for kv in self.events.iter() {
            let (key, value) = guard_data(kv)?;
            let event: MemoryEvent = self.deserialize(&value)?;
            if event.stream == stream {
                entries.push((key, event.sequence));
            }
        }

        let count = entries.len() as u64;
        for (key, seq) in &entries {
            self.events.remove(key)?;
            #[cfg(feature = "search")]
            self.record_search_mutation(
                format!("event:{stream}/{seq}"),
                SearchReplayMutation::DeleteEvent,
            );
            #[cfg(feature = "search")]
            self.enqueue_delete_event_for_search(stream, *seq);
        }

        self.event_seq_counters.remove(stream);
        self.event_idempotency_keys.retain(|(s, _), _| s != stream);
        let key = self
            .codec
            .encode_key("event_metadata.stream", stream.as_bytes());
        self.event_meta.remove(&key)?;

        Ok(count)
    }

    fn count_events(&self) -> ThingdResult<u64> {
        let mut count = 0u64;
        for kv in self.events.iter() {
            let _ = kv;
            count += 1;
        }
        Ok(count)
    }

    fn list_streams(&self) -> ThingdResult<Vec<String>> {
        let mut streams: Vec<String> = Vec::new();
        for kv in self.events.iter() {
            let (_, value) = guard_data(kv)?;
            let event: MemoryEvent = self.deserialize(&value)?;
            let stream = event.stream;
            if !streams.contains(&stream) {
                streams.push(stream);
            }
        }
        Ok(streams)
    }
}

// ── QueueStore ───────────────────────────────────────────────────────────────

impl QueueStore for PersistentEngine {
    fn push_job(&mut self, mut job: QueueJob) -> ThingdResult<QueueJob> {
        if job.created_at.is_empty() {
            job.created_at = now_iso_string();
        }
        let key = self.make_queue_key(&job.queue, &job.id);
        let data = self.serialize(&job)?;
        let mut batch = self.db.batch();
        batch.insert(&self.queue_jobs, &key, &data);
        if job.status == QueueJobStatus::Ready {
            let rkey = self.make_ready_key(&job.queue, job.priority, &job.created_at, &job.id);
            let rdata = self.serialize(&job.id)?;
            batch.insert(&self.ready_jobs, &rkey, &rdata);
        }
        batch
            .commit()
            .map_err(|error| ThingdError::Storage(error.to_string()))?;
        self.queue_diagnostics.transition_count =
            self.queue_diagnostics.transition_count.saturating_add(1);
        self.queue_diagnostics.transition_operations = self
            .queue_diagnostics
            .transition_operations
            .saturating_add(if job.status == QueueJobStatus::Ready {
                2
            } else {
                1
            });
        Ok(job)
    }

    fn push_jobs_batch(&mut self, jobs: Vec<QueueJob>) -> ThingdResult<Vec<QueueJob>> {
        let mut results = Vec::with_capacity(jobs.len());
        let mut batch = self.db.batch();
        for job in jobs {
            let mut job = job;
            if job.created_at.is_empty() {
                job.created_at = now_iso_string();
            }
            let key = self.make_queue_key(&job.queue, &job.id);
            let data = self.serialize(&job)?;
            batch.insert(&self.queue_jobs, &key, &data);
            if job.status == QueueJobStatus::Ready {
                let rkey = self.make_ready_key(&job.queue, job.priority, &job.created_at, &job.id);
                let rdata = self.serialize(&job.id)?;
                batch.insert(&self.ready_jobs, &rkey, &rdata);
            }
            results.push(job);
        }
        batch
            .commit()
            .map_err(|error| ThingdError::Storage(error.to_string()))?;
        self.queue_diagnostics.transition_count =
            self.queue_diagnostics.transition_count.saturating_add(1);
        self.queue_diagnostics.transition_operations = self
            .queue_diagnostics
            .transition_operations
            .saturating_add(results.len() as u64 * 2);
        Ok(results)
    }

    fn claim_job_with_options(
        &mut self,
        queue: &str,
        options: QueueClaimOptions,
    ) -> ThingdResult<Option<QueueJob>> {
        let now = unix_timestamp_millis();

        // Reclaim only leases that can have expired. The old implementation
        // scanned every queue job for every claim, which made large queues
        // quadratic even when only one job was active.
        let lease_prefix = self.make_lease_prefix(queue);
        let mut expiry_batch = self.db.batch();
        let mut expiry_writes = false;
        for kv in self.lease_jobs.prefix(&lease_prefix) {
            self.queue_diagnostics.lease_entries_examined = self
                .queue_diagnostics
                .lease_entries_examined
                .saturating_add(1);
            let (lease_key, lease_value) = guard_data(kv)?;
            let expires_at_ms = self.lease_expiration(queue, &lease_key)?;
            if expires_at_ms > now {
                break;
            }
            let job_id: String = self.deserialize(&lease_value)?;
            let qkey = self.make_queue_key(queue, &job_id);
            let Some(job_data) = value_to_vec(self.queue_jobs.get(&qkey)?) else {
                self.queue_diagnostics.stale_index_repairs =
                    self.queue_diagnostics.stale_index_repairs.saturating_add(1);
                expiry_batch.remove(&self.lease_jobs, lease_key);
                expiry_writes = true;
                continue;
            };
            let mut job: QueueJob = self.deserialize(&job_data)?;
            expiry_batch.remove(&self.lease_jobs, lease_key);
            expiry_writes = true;
            if job.status == QueueJobStatus::Leased
                && job.lease_expires_at_ms.is_some_and(|exp| exp <= now)
            {
                job.status = QueueJobStatus::Ready;
                job.leased_at_ms = None;
                job.lease_expires_at_ms = None;
                let data = self.serialize(&job)?;
                let rkey = self.make_ready_key(&job.queue, job.priority, &job.created_at, &job.id);
                let rdata = self.serialize(&job.id)?;
                expiry_batch.insert(&self.queue_jobs, &qkey, &data);
                expiry_batch.insert(&self.ready_jobs, &rkey, &rdata);
            }
        }
        if expiry_writes {
            expiry_batch
                .commit()
                .map_err(|error| ThingdError::Storage(error.to_string()))?;
            self.queue_diagnostics.transition_count =
                self.queue_diagnostics.transition_count.saturating_add(1);
            self.queue_diagnostics.transition_operations = self
                .queue_diagnostics
                .transition_operations
                .saturating_add(3);
        }

        // Read only the first ready index entry instead of materializing every
        // ready job on each claim.
        let prefix = self.make_ready_prefix(queue);
        let mut after_key = None;
        while let Some(kv) = self
            .ready_jobs
            .first_prefix_after(&prefix, after_key.as_deref())?
        {
            let (key, value) = guard_data(kv)?;
            let job_id = if value.is_empty() {
                let key_str = String::from_utf8_lossy(&key);
                let parts: Vec<&str> = key_str.splitn(4, '\0').collect();
                if parts.len() < 4 {
                    continue;
                }
                parts[3].to_string()
            } else {
                self.deserialize::<String>(&value)?
            };
            let rkey = key;

            // Read full job from queue_jobs
            let qkey = self.make_queue_key(queue, &job_id);
            let Some(job_data) = value_to_vec(self.queue_jobs.get(&qkey)?) else {
                // Job record missing — remove stale index entry
                after_key = Some(rkey.clone());
                self.queue_diagnostics.stale_index_repairs =
                    self.queue_diagnostics.stale_index_repairs.saturating_add(1);
                let mut batch = self.db.batch();
                batch.remove(&self.ready_jobs, &rkey);
                batch
                    .commit()
                    .map_err(|error| ThingdError::Storage(error.to_string()))?;
                continue;
            };
            let mut job: QueueJob = self.deserialize(&job_data)?;
            // Release expired lease if this job was previously leased
            if job.status == QueueJobStatus::Leased
                && job.lease_expires_at_ms.is_some_and(|exp| exp <= now)
            {
                job.status = QueueJobStatus::Ready;
                job.leased_at_ms = None;
                job.lease_expires_at_ms = None;
            }

            // Skip (and remove) stale entries for completed or dead jobs
            if job.status != QueueJobStatus::Ready {
                after_key = Some(rkey.clone());
                self.queue_diagnostics.stale_index_repairs =
                    self.queue_diagnostics.stale_index_repairs.saturating_add(1);
                let mut batch = self.db.batch();
                batch.remove(&self.ready_jobs, &rkey);
                batch
                    .commit()
                    .map_err(|error| ThingdError::Storage(error.to_string()))?;
                continue;
            }

            // Job is delayed — skip it but keep the index entry so it can be claimed later
            if job.available_at_ms > now {
                after_key = Some(rkey.clone());
                continue;
            }

            // Claim this job
            job.status = QueueJobStatus::Leased;
            job.attempts = job.attempts.saturating_add(1);
            job.leased_at_ms = Some(now);
            job.lease_expires_at_ms = Some(now + options.lease_ms as i64);
            let data = self.serialize(&job)?;
            let lease_key =
                self.make_lease_key(queue, job.lease_expires_at_ms.unwrap_or_default(), &job.id);
            let lease_value = self.serialize(&job.id)?;
            let mut batch = self.db.batch();
            batch.remove(&self.ready_jobs, &rkey);
            batch.insert(&self.queue_jobs, &qkey, &data);
            batch.insert(&self.lease_jobs, &lease_key, &lease_value);
            batch
                .commit()
                .map_err(|error| ThingdError::Storage(error.to_string()))?;
            self.queue_diagnostics.transition_count =
                self.queue_diagnostics.transition_count.saturating_add(1);
            self.queue_diagnostics.transition_operations = self
                .queue_diagnostics
                .transition_operations
                .saturating_add(3);
            return Ok(Some(job));
        }

        Ok(None)
    }

    fn ack_job(&mut self, queue: &str, id: &str) -> ThingdResult<Option<QueueJob>> {
        let key = self.make_queue_key(queue, id);
        match value_to_vec(self.queue_jobs.get(&key)?) {
            Some(data) => {
                let mut job: QueueJob = self.deserialize(&data)?;
                if job.status != QueueJobStatus::Leased {
                    return Ok(None);
                }
                job.status = QueueJobStatus::Completed;
                job.completed_at_ms = Some(unix_timestamp_millis());
                let new_data = self.serialize(&job)?;
                let mut batch = self.db.batch();
                batch.insert(&self.queue_jobs, &key, &new_data);
                if let Some(expires_at_ms) = job.lease_expires_at_ms {
                    let lease_key = self.make_lease_key(queue, expires_at_ms, id);
                    batch.remove(&self.lease_jobs, &lease_key);
                }
                batch
                    .commit()
                    .map_err(|error| ThingdError::Storage(error.to_string()))?;
                self.queue_diagnostics.transition_count =
                    self.queue_diagnostics.transition_count.saturating_add(1);
                self.queue_diagnostics.transition_operations = self
                    .queue_diagnostics
                    .transition_operations
                    .saturating_add(2);
                Ok(Some(job))
            },
            None => Ok(None),
        }
    }

    fn nack_job_with_options(
        &mut self,
        queue: &str,
        id: &str,
        options: QueueNackOptions,
    ) -> ThingdResult<Option<QueueJob>> {
        let key = self.make_queue_key(queue, id);
        match value_to_vec(self.queue_jobs.get(&key)?) {
            Some(data) => {
                let mut job: QueueJob = self.deserialize(&data)?;
                if job.status != QueueJobStatus::Leased {
                    return Ok(None);
                }
                let previous_lease_expires_at_ms = job.lease_expires_at_ms;
                job.last_error = options.error;
                job.leased_at_ms = None;
                job.lease_expires_at_ms = None;

                let is_dead = job.attempts >= job.max_attempts;
                if is_dead {
                    job.status = QueueJobStatus::Dead;
                    job.dead_at_ms = Some(unix_timestamp_millis());
                } else {
                    job.status = QueueJobStatus::Ready;
                    job.available_at_ms = unix_timestamp_millis() + options.delay_ms as i64;
                }

                let new_data = self.serialize(&job)?;
                let mut batch = self.db.batch();
                batch.insert(&self.queue_jobs, &key, &new_data);
                if let Some(expires_at_ms) = previous_lease_expires_at_ms {
                    let lease_key = self.make_lease_key(queue, expires_at_ms, id);
                    batch.remove(&self.lease_jobs, &lease_key);
                }

                // Re-index if retrying
                if !is_dead {
                    let rkey =
                        self.make_ready_key(&job.queue, job.priority, &job.created_at, &job.id);
                    let rdata = self.serialize(&job.id)?;
                    batch.insert(&self.ready_jobs, &rkey, &rdata);
                }

                batch
                    .commit()
                    .map_err(|error| ThingdError::Storage(error.to_string()))?;
                self.queue_diagnostics.transition_count =
                    self.queue_diagnostics.transition_count.saturating_add(1);
                self.queue_diagnostics.transition_operations = self
                    .queue_diagnostics
                    .transition_operations
                    .saturating_add(if is_dead { 2 } else { 3 });

                Ok(Some(job))
            },
            None => Ok(None),
        }
    }

    fn list_jobs(&self, queue: &str) -> ThingdResult<Vec<QueueJob>> {
        let prefix = self.make_queue_prefix(queue);
        let mut jobs = Vec::new();
        for kv in self.queue_jobs.prefix(&prefix) {
            let (_, value) = guard_data(kv)?;
            jobs.push(self.deserialize(&value)?);
        }
        Ok(jobs)
    }

    fn list_dead_jobs(&self, queue: &str) -> ThingdResult<Vec<QueueJob>> {
        let prefix = self.make_queue_prefix(queue);
        let mut jobs = Vec::new();
        for kv in self.queue_jobs.prefix(&prefix) {
            let (_, value) = guard_data(kv)?;
            let job: QueueJob = self.deserialize(&value)?;
            if job.status == QueueJobStatus::Dead {
                jobs.push(job);
            }
        }
        Ok(jobs)
    }

    fn list_queues(&self) -> ThingdResult<Vec<String>> {
        let mut queues: Vec<String> = Vec::new();
        for kv in self.queue_jobs.iter() {
            let (_, value) = guard_data(kv)?;
            let job: QueueJob = self.deserialize(&value)?;
            if !queues.contains(&job.queue) {
                queues.push(job.queue);
            }
        }
        Ok(queues)
    }

    fn count_active_jobs(&self) -> ThingdResult<u64> {
        let mut count = 0u64;
        for kv in self.queue_jobs.iter() {
            let (_, value) = guard_data(kv)?;
            let job: QueueJob = self.deserialize(&value)?;
            if job.status == QueueJobStatus::Ready || job.status == QueueJobStatus::Leased {
                count += 1;
            }
        }
        Ok(count)
    }

    fn count_dead_jobs(&self) -> ThingdResult<u64> {
        let mut count = 0u64;
        for kv in self.queue_jobs.iter() {
            let (_, value) = guard_data(kv)?;
            let job: QueueJob = self.deserialize(&value)?;
            if job.status == QueueJobStatus::Dead {
                count += 1;
            }
        }
        Ok(count)
    }
}

// ── Searcher ─────────────────────────────────────────────────────────────────

impl Searcher for PersistentEngine {
    fn search(&self, query: &str, options: SearchOptions) -> ThingdResult<Vec<SearchHit>> {
        let started = std::time::Instant::now();
        let result = (|| {
            #[cfg(feature = "search")]
            if self.maintenance.state != "idle"
                || self.search_rebuild.is_some()
                || self.search_mode == PersistentSearchMode::PersistentNoRebuild
                || self
                    .search_queue
                    .as_ref()
                    .is_some_and(|queue| queue.needs_fallback())
            {
                return self.search_naive(query, options);
            }
            // Try Tantivy search first
            #[cfg(feature = "search")]
            if let (Some(index), Some(reader)) = (&self.search_index, &self.search_reader) {
                return self.search_tantivy(index, reader, query, options);
            }

            // Fallback: naive substring search (same as MemoryEngine)
            self.search_naive(query, options)
        })();
        self.db
            .record_ram_search(u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX));
        result
    }
}

fn normalize_search_text(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect()
}

impl PersistentEngine {
    #[cfg(feature = "search")]
    fn record_search_mutation(&mut self, key: String, mutation: SearchReplayMutation) {
        let Some(progress) = self.search_rebuild.as_mut() else {
            return;
        };
        if progress.replay.len() >= SEARCH_REBUILD_REPLAY_LIMIT
            && !progress.replay.contains_key(&key)
        {
            progress.replay_overflow = true;
            return;
        }
        progress.replay.insert(key, mutation);
    }

    #[cfg(feature = "search")]
    fn replay_search_mutation(
        &self,
        key: &str,
        mutation: SearchReplayMutation,
    ) -> ThingdResult<()> {
        match mutation {
            SearchReplayMutation::UpsertObject => {
                let Some((collection, id)) = key.strip_prefix("object:").and_then(|value| {
                    value
                        .split_once('/')
                        .map(|(collection, id)| (collection.to_string(), id.to_string()))
                }) else {
                    return Ok(());
                };
                let object_key = self.make_object_key(&collection, &id);
                if let Some(value) = value_to_vec(self.objects.get(&object_key)?) {
                    let object: MemoryObject = self.deserialize(&value)?;
                    self.index_object_for_search_with_commit(&object, false);
                } else {
                    self.delete_object_from_search_index(&collection, &id);
                }
            },
            SearchReplayMutation::DeleteObject => {
                let Some((collection, id)) = key.strip_prefix("object:").and_then(|value| {
                    value
                        .split_once('/')
                        .map(|(collection, id)| (collection.to_string(), id.to_string()))
                }) else {
                    return Ok(());
                };
                self.delete_object_from_search_index(&collection, &id);
            },
            SearchReplayMutation::UpsertEvent => {
                let Some(value) = key.strip_prefix("event:") else {
                    return Ok(());
                };
                let Some((stream, sequence)) = value.rsplit_once('/') else {
                    return Ok(());
                };
                let Ok(sequence) = sequence.parse::<u64>() else {
                    return Ok(());
                };
                let event_key = self.make_event_key(stream, sequence);
                if let Some(value) = value_to_vec(self.events.get(&event_key)?) {
                    let event: MemoryEvent = self.deserialize(&value)?;
                    self.index_event_for_search_with_commit(&event, false);
                } else {
                    self.delete_event_from_search_index(stream, sequence);
                }
            },
            SearchReplayMutation::DeleteEvent => {
                let Some(value) = key.strip_prefix("event:") else {
                    return Ok(());
                };
                let Some((stream, sequence)) = value.rsplit_once('/') else {
                    return Ok(());
                };
                if let Ok(sequence) = sequence.parse::<u64>() {
                    self.delete_event_from_search_index(stream, sequence);
                }
            },
        }
        Ok(())
    }

    #[cfg(feature = "search")]
    fn index_object_for_search(&self, object: &MemoryObject) {
        if self.search_mode == PersistentSearchMode::PersistentNoRebuild {
            return;
        }
        if let Some(queue) = &self.search_queue {
            queue.enqueue(
                format!("object:{}/{}", object.key.collection, object.key.id),
                SearchIndexMutation::UpsertObject(object.clone()),
            );
        }
    }

    #[cfg(feature = "search")]
    fn index_object_for_search_with_commit(&self, object: &MemoryObject, commit: bool) {
        let Some(ref index) = self.search_index else {
            return;
        };
        let schema = index.schema();
        let doc_key_field = schema.get_field("doc_key").unwrap();
        let body_field = schema.get_field("body").unwrap();
        let collection_field = schema.get_field("collection").unwrap();
        let id_field = schema.get_field("id").unwrap();
        let kind_field = schema.get_field("kind").unwrap();

        let Some(ref writer) = self.search_writer else {
            return;
        };
        let Ok(mut writer) = writer.lock() else {
            return;
        };

        let doc_key = format!("{}/{}", object.key.collection, object.key.id);
        // Remove existing document with the same doc_key to prevent duplicates
        let term = tantivy::Term::from_field_text(doc_key_field, &doc_key);
        let _ = writer.delete_term(term);

        let mut doc = tantivy::TantivyDocument::new();
        doc.add_text(doc_key_field, &doc_key);
        doc.add_text(collection_field, &object.key.collection);
        doc.add_text(id_field, &object.key.id);
        doc.add_text(body_field, &object.body);
        doc.add_text(kind_field, "object");

        let _ = writer.add_document(doc);
        if commit {
            let _ = writer.commit();
            drop(writer);
            self.reload_search_reader();
        }
    }

    #[cfg(feature = "search")]
    fn index_event_for_search(&self, event: &MemoryEvent) {
        if self.search_mode == PersistentSearchMode::PersistentNoRebuild {
            return;
        }
        if let Some(queue) = &self.search_queue {
            queue.enqueue(
                format!("event:{}/{}", event.stream, event.sequence),
                SearchIndexMutation::UpsertEvent(event.clone()),
            );
        }
    }

    #[cfg(feature = "search")]
    fn index_event_for_search_with_commit(&self, event: &MemoryEvent, commit: bool) {
        let Some(ref index) = self.search_index else {
            return;
        };
        let schema = index.schema();
        let doc_key_field = schema.get_field("doc_key").unwrap();
        let body_field = schema.get_field("body").unwrap();
        let collection_field = schema.get_field("collection").unwrap();
        let id_field = schema.get_field("id").unwrap();
        let kind_field = schema.get_field("kind").unwrap();

        let Some(ref writer) = self.search_writer else {
            return;
        };
        let Ok(mut writer) = writer.lock() else {
            return;
        };

        let mut doc = tantivy::TantivyDocument::new();
        let doc_key = format!("event:{}/{}", event.stream, event.sequence);
        doc.add_text(doc_key_field, &doc_key);
        doc.add_text(collection_field, &event.stream);
        doc.add_text(id_field, event.sequence.to_string());
        doc.add_text(body_field, &event.body);
        doc.add_text(kind_field, "event");

        let _ = writer.add_document(doc);
        if commit {
            let _ = writer.commit();
            drop(writer);
            self.reload_search_reader();
        }
    }

    #[cfg(feature = "search")]
    fn commit_search_index(&self) {
        let Some(ref writer) = self.search_writer else {
            return;
        };
        let Ok(mut writer) = writer.lock() else {
            return;
        };
        let _ = writer.commit();
        drop(writer);
        self.reload_search_reader();
    }

    #[cfg(feature = "search")]
    fn reload_search_reader(&self) {
        if let Some(ref reader) = self.search_reader
            && let Ok(reader) = reader.lock()
        {
            let _ = reader.reload();
        }
    }

    #[cfg(feature = "search")]
    fn delete_event_from_search_index(&self, stream: &str, sequence: u64) {
        let Some(ref index) = self.search_index else {
            return;
        };
        let schema = index.schema();
        let doc_key_field = schema.get_field("doc_key").unwrap();

        let Some(ref writer) = self.search_writer else {
            return;
        };
        let Ok(mut writer) = writer.lock() else {
            return;
        };

        let doc_key = format!("event:{stream}/{sequence}");
        let term = tantivy::Term::from_field_text(doc_key_field, &doc_key);
        let _ = writer.delete_term(term);
        let _ = writer.commit();
        drop(writer);
        self.reload_search_reader();
    }

    #[cfg(feature = "search")]
    fn enqueue_delete_event_for_search(&self, stream: &str, sequence: u64) {
        if self.search_mode == PersistentSearchMode::PersistentNoRebuild {
            return;
        }
        if let Some(queue) = &self.search_queue {
            queue.enqueue(
                format!("event:{stream}/{sequence}"),
                SearchIndexMutation::DeleteEvent {
                    stream: stream.to_string(),
                    sequence,
                },
            );
        }
    }

    #[cfg(feature = "search")]
    fn enqueue_delete_object_for_search(&self, collection: &str, id: &str) {
        if self.search_mode == PersistentSearchMode::PersistentNoRebuild {
            return;
        }
        if let Some(queue) = &self.search_queue {
            queue.enqueue(
                format!("object:{collection}/{id}"),
                SearchIndexMutation::DeleteObject {
                    collection: collection.to_string(),
                    id: id.to_string(),
                },
            );
        }
    }

    #[cfg(feature = "search")]
    fn delete_events_from_search_index(&mut self, events: &[(String, u64)]) {
        if events.is_empty() {
            return;
        }
        for (stream, sequence) in events {
            self.record_search_mutation(
                format!("event:{stream}/{sequence}"),
                SearchReplayMutation::DeleteEvent,
            );
            self.enqueue_delete_event_for_search(stream, *sequence);
        }
    }

    #[cfg(feature = "search")]
    fn delete_object_from_search_index(&self, collection: &str, id: &str) {
        let Some(ref index) = self.search_index else {
            return;
        };
        let schema = index.schema();
        let doc_key_field = schema.get_field("doc_key").unwrap();

        let Some(ref writer) = self.search_writer else {
            return;
        };
        let Ok(mut writer) = writer.lock() else {
            return;
        };

        let doc_key = format!("{collection}/{id}");
        let term = tantivy::Term::from_field_text(doc_key_field, &doc_key);
        let _ = writer.delete_term(term);
        let _ = writer.commit();
        drop(writer);
        self.reload_search_reader();
    }

    #[cfg(feature = "search")]
    fn search_tantivy(
        &self,
        index: &tantivy::Index,
        reader: &Arc<Mutex<tantivy::IndexReader>>,
        query: &str,
        options: SearchOptions,
    ) -> ThingdResult<Vec<SearchHit>> {
        use tantivy::collector::TopDocs;
        use tantivy::query::QueryParser;
        use tantivy::schema::Value;

        let reader = reader
            .lock()
            .map_err(|_| ThingdError::Storage("search reader lock poisoned".to_string()))?;
        let searcher = reader.searcher();
        let schema = index.schema();

        let body_field = schema.get_field("body").unwrap();
        let collection_field = schema.get_field("collection").unwrap();
        let id_field = schema.get_field("id").unwrap();
        let kind_field = schema.get_field("kind").unwrap();

        let mut parser = QueryParser::for_index(index, vec![body_field]);
        parser.set_conjunction_by_default();

        let tantivy_query = parser
            .parse_query(query)
            .map_err(|e| ThingdError::InvalidInput(e.to_string()))?;

        let limit = options.limit.unwrap_or(10).min(1000);
        let doc_ids = searcher
            .search(&tantivy_query, &TopDocs::with_limit(limit).order_by_score())
            .map_err(|e| ThingdError::Storage(e.to_string()))?;

        let mut hits = Vec::new();

        for (score, doc_address) in doc_ids {
            let doc = searcher
                .doc::<tantivy::TantivyDocument>(doc_address)
                .map_err(|e| ThingdError::Storage(e.to_string()))?;

            let collection = doc
                .get_first(collection_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let id = doc
                .get_first(id_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let body = doc
                .get_first(body_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let kind = doc
                .get_first(kind_field)
                .and_then(|v| v.as_str())
                .unwrap_or("object")
                .to_string();

            if let Some(ref collections) = options.collections
                && !collections.contains(&collection)
            {
                continue;
            }

            if let Some(ref filter) = options.filter
                && !matches_filter_memory(&body, filter)
            {
                continue;
            }

            let col = collection.clone();
            let doc_id = id.clone();
            let doc_body = body.clone();

            if kind == "object"
                && let Some(obj_data) = self
                    .objects
                    .get(self.make_object_key(&col, &doc_id))
                    .ok()
                    .and_then(value_to_vec)
                && let Ok(obj) = self.deserialize::<MemoryObject>(&obj_data)
            {
                hits.push(SearchHit {
                    kind: "object".to_string(),
                    collection: col,
                    id: doc_id,
                    text: doc_body.clone(),
                    score: score.into(),
                    body: doc_body,
                    version: Some(obj.version),
                    created_at: obj.created_at,
                    updated_at: Some(obj.updated_at),
                    event_type: None,
                });
            } else if kind == "event"
                && let Ok(seq) = id.parse::<u64>()
                && let Some(ev_data) = self
                    .events
                    .get(self.make_event_key(&collection, seq))
                    .ok()
                    .and_then(value_to_vec)
                && let Ok(event) = self.deserialize::<MemoryEvent>(&ev_data)
            {
                hits.push(SearchHit {
                    kind: "event".to_string(),
                    collection: collection.clone(),
                    id: seq.to_string(),
                    text: body.clone(),
                    score: score.into(),
                    body: body.clone(),
                    version: None,
                    created_at: event.created_at,
                    updated_at: None,
                    event_type: Some(event.event_type),
                });
            }
        }

        drop(reader);
        Ok(hits)
    }

    fn search_naive(&self, query: &str, options: SearchOptions) -> ThingdResult<Vec<SearchHit>> {
        let query_words: Vec<String> = query
            .split_whitespace()
            .map(normalize_search_text)
            .filter(|w: &String| !w.is_empty())
            .collect();

        if query_words.is_empty() {
            return Ok(Vec::new());
        }

        let mut hits = Vec::new();

        for kv in self.objects.iter() {
            let (_, value) = guard_data(kv)?;
            let object: MemoryObject = self.deserialize(&value)?;

            if let Some(ref collections) = options.collections
                && !collections.contains(&object.key.collection)
            {
                continue;
            }

            if let Some(ref filter) = options.filter
                && !matches_filter_memory(&object.body, filter)
            {
                continue;
            }

            let text_to_search = normalize_search_text(&format!(
                "{} {} {}",
                object.key.collection, object.key.id, object.body
            ));
            let matches_all = query_words.iter().all(|word| text_to_search.contains(word));

            if matches_all {
                hits.push(SearchHit {
                    kind: "object".to_string(),
                    collection: object.key.collection.clone(),
                    id: object.key.id.clone(),
                    text: object.body.clone(),
                    score: 1.0,
                    body: object.body.clone(),
                    version: Some(object.version),
                    created_at: object.created_at.clone(),
                    updated_at: Some(object.updated_at.clone()),
                    event_type: None,
                });
            }
        }

        for kv in self.events.iter() {
            let (_, value) = guard_data(kv)?;
            let event: MemoryEvent = self.deserialize(&value)?;

            if let Some(ref collections) = options.collections
                && !collections.contains(&event.stream)
            {
                continue;
            }

            if let Some(ref filter) = options.filter
                && !matches_filter_memory(&event.body, filter)
            {
                continue;
            }

            let text_to_search = normalize_search_text(&format!(
                "{} {} {}",
                event.stream, event.event_type, event.body
            ));
            let matches_all = query_words.iter().all(|word| text_to_search.contains(word));

            if matches_all {
                hits.push(SearchHit {
                    kind: "event".to_string(),
                    collection: event.stream.clone(),
                    id: event.sequence.to_string(),
                    text: event.body.clone(),
                    score: 1.0,
                    body: event.body.clone(),
                    version: None,
                    created_at: event.created_at.clone(),
                    updated_at: None,
                    event_type: Some(event.event_type.clone()),
                });
            }
        }

        if let Some(limit) = options.limit {
            hits.truncate(limit);
        }

        Ok(hits)
    }
}

// ── LinkStore ────────────────────────────────────────────────────────────────

impl LinkStore for PersistentEngine {
    fn create_link(&mut self, mut link: Link) -> ThingdResult<Link> {
        let id = self.next_link_id.fetch_add(1, Ordering::Relaxed);
        link.id = format!("link-{id}");
        if link.created_at.is_empty() {
            link.created_at = now_iso_string();
        }

        let data = self.serialize(&link)?;

        let link_id_key = self.make_link_id_key(&link.id);
        let index_data = self.serialize(&link.id)?;
        self.links_by_id.insert(&link_id_key, &data)?;
        self.links_from.insert(
            self.make_link_from_key(&link.from_ref, &link.link_type, &link.id),
            &index_data,
        )?;
        self.links_to.insert(
            self.make_link_to_key(&link.to_ref, &link.link_type, &link.id),
            &index_data,
        )?;

        Ok(link)
    }

    fn delete_link(&mut self, id: &str) -> ThingdResult<bool> {
        let link_id_key = self.make_link_id_key(id);
        if let Some(data) = value_to_vec(self.links_by_id.get(&link_id_key)?) {
            let link: Link = self.deserialize(&data)?;
            self.links_by_id.remove(&link_id_key)?;
            self.links_from
                .remove(self.make_link_from_key(&link.from_ref, &link.link_type, id))?;
            self.links_to
                .remove(self.make_link_to_key(&link.to_ref, &link.link_type, id))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn get_link(&self, id: &str) -> ThingdResult<Option<Link>> {
        match value_to_vec(self.links_by_id.get(self.make_link_id_key(id))?) {
            Some(data) => Ok(Some(self.deserialize(&data)?)),
            None => Ok(None),
        }
    }

    fn get_neighbors(
        &self,
        reference: &str,
        direction: LinkDirection,
        options: LinkQueryOptions,
    ) -> ThingdResult<Vec<Link>> {
        let mut results: Vec<Link> = Vec::new();
        let outgoing_prefix = self.make_link_from_prefix(reference);
        let incoming_prefix = self.make_link_to_prefix(reference);

        if direction == LinkDirection::Outgoing || direction == LinkDirection::Both {
            for kv in self.links_from.prefix(&outgoing_prefix) {
                let (key, value) = guard_data(kv)?;
                let link_id = if value.is_empty() {
                    let key_str = String::from_utf8_lossy(&key);
                    key_str.splitn(3, '\0').nth(2).unwrap_or("").to_string()
                } else {
                    self.deserialize::<String>(&value)?
                };
                if let Some(data) =
                    value_to_vec(self.links_by_id.get(self.make_link_id_key(&link_id))?)
                {
                    let link: Link = self.deserialize(&data)?;
                    if options
                        .link_type
                        .as_ref()
                        .is_none_or(|link_type| link.link_type == *link_type)
                    {
                        results.push(link);
                    }
                }
            }
        }

        if direction == LinkDirection::Incoming || direction == LinkDirection::Both {
            for kv in self.links_to.prefix(&incoming_prefix) {
                let (key, value) = guard_data(kv)?;
                let link_id = if value.is_empty() {
                    let key_str = String::from_utf8_lossy(&key);
                    key_str.splitn(3, '\0').nth(2).unwrap_or("").to_string()
                } else {
                    self.deserialize::<String>(&value)?
                };
                if let Some(data) =
                    value_to_vec(self.links_by_id.get(self.make_link_id_key(&link_id))?)
                {
                    let link: Link = self.deserialize(&data)?;
                    if options
                        .link_type
                        .as_ref()
                        .is_none_or(|link_type| link.link_type == *link_type)
                    {
                        results.push(link);
                    }
                }
            }
        }

        if let Some(limit) = options.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    fn count_links(&self) -> ThingdResult<u64> {
        let mut count = 0u64;
        for kv in self.links_by_id.iter() {
            let _ = kv;
            count += 1;
        }
        Ok(count)
    }
}

// ── AggregateStore ───────────────────────────────────────────────────────────

impl AggregateStore for PersistentEngine {
    fn aggregate(
        &self,
        collection: &str,
        options: &AggregateOptions,
    ) -> ThingdResult<AggregateResult> {
        let prefix = self.make_object_prefix(collection);
        let mut objects: Vec<MemoryObject> = Vec::new();

        for kv in self.objects.prefix(&prefix) {
            let (_, value) = guard_data(kv)?;
            let obj: MemoryObject = self.deserialize(&value)?;

            if options.filter.is_empty() {
                objects.push(obj);
            } else {
                let Ok(body) = serde_json::from_str::<serde_json::Value>(&obj.body) else {
                    continue;
                };
                let matches = options
                    .filter
                    .iter()
                    .all(|(key, expected)| body.get(key.as_str()).is_some_and(|v| v == expected));
                if matches {
                    objects.push(obj);
                }
            }
        }

        if let Some(group_field) = &options.group_by {
            let mut groups: HashMap<String, Vec<MemoryObject>> = HashMap::new();
            for obj in &objects {
                let key = extract_field_str(&obj.body, group_field);
                groups.entry(key).or_default().push(obj.clone());
            }

            let mut group_results: Vec<AggregateGroupResult> = groups
                .iter()
                .map(|(key, objs)| AggregateGroupResult {
                    key: key.clone(),
                    value: compute_aggregate(objs, options.function, options.field.as_deref()),
                })
                .collect();
            group_results.sort_by(|a, b| {
                b.value
                    .partial_cmp(&a.value)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let total = group_results.iter().map(|g| g.value).sum();

            Ok(AggregateResult {
                total,
                groups: group_results,
            })
        } else {
            let total = compute_aggregate(&objects, options.function, options.field.as_deref());
            Ok(AggregateResult {
                total,
                groups: vec![],
            })
        }
    }

    fn timeseries(
        &self,
        collection: &str,
        options: &TimeSeriesOptions,
    ) -> ThingdResult<TimeSeriesResult> {
        let prefix = self.make_object_prefix(collection);
        let mut bucket_map: HashMap<String, Vec<f64>> = HashMap::new();

        for kv in self.objects.prefix(&prefix) {
            let (_, value) = guard_data(kv)?;
            let obj: MemoryObject = self.deserialize(&value)?;

            if !options.filter.is_empty() {
                let Ok(body) = serde_json::from_str::<serde_json::Value>(&obj.body) else {
                    continue;
                };
                let matches = options
                    .filter
                    .iter()
                    .all(|(key, expected)| body.get(key.as_str()).is_some_and(|v| v == expected));
                if !matches {
                    continue;
                }
            }

            let bucket_label = bucket_label_for_date(&obj.created_at, options.bucket);

            if let Some(ref from) = options.from
                && bucket_label.as_str() < from.as_str()
            {
                continue;
            }
            if let Some(ref to) = options.to
                && bucket_label.as_str() > to.as_str()
            {
                continue;
            }

            if options.function == AggregateFunction::Count {
                bucket_map.entry(bucket_label).or_default().push(1.0);
            } else if let Some(ref field) = options.field
                && let Ok(body) = serde_json::from_str::<serde_json::Value>(&obj.body)
                && let Some(val) = body.get(field.as_str()).and_then(serde_json::Value::as_f64)
            {
                bucket_map.entry(bucket_label).or_default().push(val);
            }
        }

        let mut bucket_list: Vec<(String, f64)> = bucket_map
            .into_iter()
            .map(|(label, values)| {
                let value = match options.function {
                    AggregateFunction::Count => values.len() as f64,
                    AggregateFunction::Sum => values.iter().sum(),
                    AggregateFunction::Avg => {
                        if values.is_empty() {
                            0.0
                        } else {
                            values.iter().sum::<f64>() / values.len() as f64
                        }
                    },
                    AggregateFunction::Min => values.iter().copied().fold(f64::MAX, f64::min),
                    AggregateFunction::Max => {
                        values.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                    },
                };
                (label, value)
            })
            .collect();

        bucket_list.sort_by(|a, b| a.0.cmp(&b.0));

        let buckets: Vec<TimeSeriesBucket> = bucket_list
            .into_iter()
            .map(|(label, value)| TimeSeriesBucket { label, value })
            .collect();

        Ok(TimeSeriesResult { buckets })
    }
}

// ── VectorStore ──────────────────────────────────────────────────────────────

impl crate::store::VectorStore for PersistentEngine {
    fn vector_search(
        &self,
        collection: &str,
        query_vector: &[f32],
        options: VectorSearchOptions,
    ) -> ThingdResult<Vec<VectorSearchHit>> {
        if query_vector.is_empty() {
            return Err(ThingdError::InvalidInput(
                "query vector must not be empty".to_string(),
            ));
        }

        #[cfg(not(feature = "vectors"))]
        {
            let _ = (collection, query_vector, options);
            Ok(vec![])
        }

        #[cfg(feature = "vectors")]
        {
            let prefix = self.make_vector_prefix(collection);
            let mut hits: Vec<VectorSearchHit> = Vec::new();

            for kv in self.vectors.prefix(&prefix) {
                let (physical_key, value) = guard_data(kv)?;
                let stored = if let Ok(stored) = self.deserialize::<StoredVector>(&value) {
                    stored
                } else {
                    let Some(separator) = physical_key.iter().rposition(|byte| *byte == 0) else {
                        return Err(ThingdError::Storage(
                            "legacy vector record has no collection/id separator".to_string(),
                        ));
                    };
                    StoredVector {
                        collection: collection.to_string(),
                        id: String::from_utf8_lossy(&physical_key[separator + 1..]).into_owned(),
                        vector: self.deserialize(&value)?,
                    }
                };
                let id = stored.id;
                let vector = stored.vector;

                if vector.len() != query_vector.len() {
                    return Err(ThingdError::InvalidInput(format!(
                        "query vector dimension {} does not match stored vector dimension {}",
                        query_vector.len(),
                        vector.len()
                    )));
                }

                let Some(object) = self.get_object(collection, &id)? else {
                    continue;
                };

                if let Some(ref filter) = options.filter
                    && !matches_filter_memory(&object.body, filter)
                {
                    continue;
                }

                let score = crate::cosine_similarity(query_vector, &vector);
                hits.push(VectorSearchHit {
                    id: id.clone(),
                    score,
                    value: object,
                });
            }

            hits.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            if let Some(top_k) = options.top_k {
                hits.truncate(top_k);
            }

            Ok(hits)
        }
    }

    fn add_vector(&mut self, collection: &str, id: &str, vector: &[f32]) -> ThingdResult<()> {
        #[cfg(not(feature = "vectors"))]
        {
            let _ = (collection, id, vector);
        }

        #[cfg(feature = "vectors")]
        {
            let vkey = self.make_vector_key(collection, id);
            let vdata = self.serialize(&StoredVector {
                collection: collection.to_string(),
                id: id.to_string(),
                vector: vector.to_vec(),
            })?;
            self.vectors.insert(&vkey, &vdata)?;
        }

        Ok(())
    }

    fn remove_vector(&mut self, collection: &str, id: &str) -> ThingdResult<()> {
        #[cfg(not(feature = "vectors"))]
        {
            let _ = (collection, id);
        }

        #[cfg(feature = "vectors")]
        {
            let vkey = self.make_vector_key(collection, id);
            let _ = self.vectors.remove(&vkey);
        }

        Ok(())
    }
}

// ── Helper functions ─────────────────────────────────────────────────────────

fn value_compare(a: &serde_json::Value, b: &serde_json::Value) -> std::cmp::Ordering {
    match (a, b) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            let a_f = a.as_f64().unwrap_or(0.0);
            let b_f = b.as_f64().unwrap_or(0.0);
            a_f.partial_cmp(&b_f).unwrap_or(std::cmp::Ordering::Equal)
        },
        (serde_json::Value::String(a), serde_json::Value::String(b)) => a.cmp(b),
        (serde_json::Value::Bool(a), serde_json::Value::Bool(b)) => a.cmp(b),
        _ => format!("{a}").cmp(&format!("{b}")),
    }
}

fn like_match(s: &str, pattern: &str) -> bool {
    let parts = pattern.split('%');
    let mut pos = 0;
    for part in parts {
        if part.is_empty() {
            continue;
        }
        if let Some(idx) = s[pos..].find(part) {
            pos += idx + part.len();
        } else {
            return false;
        }
    }
    true
}

fn matches_filter_memory(body_str: &str, filter: &serde_json::Value) -> bool {
    let Ok(body) = serde_json::from_str::<serde_json::Value>(body_str) else {
        return false;
    };
    if let serde_json::Value::Object(map) = filter {
        map.iter()
            .all(|(key, expected)| body.get(key.as_str()).is_some_and(|v| v == expected))
    } else {
        false
    }
}

fn matches_object_filters(object: &MemoryObject, filters: &[(String, serde_json::Value)]) -> bool {
    if filters.is_empty() {
        return true;
    }
    let Ok(body) = serde_json::from_str::<serde_json::Value>(&object.body) else {
        return false;
    };
    filters.iter().all(|(key, expected)| {
        let field_val = body.get(key.as_str());
        match expected {
            serde_json::Value::Object(ops)
                if ops.keys().any(|k| {
                    matches!(
                        k.as_str(),
                        "$gt" | "$gte" | "$lt" | "$lte" | "$ne" | "$in" | "$like"
                    )
                }) =>
            {
                let Some(fv) = field_val else {
                    return false;
                };
                ops.iter().all(|(op, operand)| match op.as_str() {
                    "$gt" => value_compare(fv, operand) == std::cmp::Ordering::Greater,
                    "$gte" => matches!(
                        value_compare(fv, operand),
                        std::cmp::Ordering::Greater | std::cmp::Ordering::Equal
                    ),
                    "$lt" => value_compare(fv, operand) == std::cmp::Ordering::Less,
                    "$lte" => matches!(
                        value_compare(fv, operand),
                        std::cmp::Ordering::Less | std::cmp::Ordering::Equal
                    ),
                    "$ne" => value_compare(fv, operand) != std::cmp::Ordering::Equal,
                    "$in" => operand
                        .as_array()
                        .is_some_and(|items| items.iter().any(|item| fv == item)),
                    "$like" => {
                        if let (Some(s), Some(pattern)) = (fv.as_str(), operand.as_str()) {
                            like_match(s, pattern)
                        } else {
                            false
                        }
                    },
                    _ => true,
                })
            },
            _ => field_val.is_some_and(|value| value == expected),
        }
    })
}

fn extract_field_str(body_str: &str, field: &str) -> String {
    if let Ok(body) = serde_json::from_str::<serde_json::Value>(body_str)
        && let Some(val) = body.get(field)
    {
        if let Some(s) = val.as_str() {
            return s.to_string();
        }
        return format!("{val}");
    }
    String::new()
}

fn compute_aggregate(
    objects: &[MemoryObject],
    function: AggregateFunction,
    field: Option<&str>,
) -> f64 {
    if function == AggregateFunction::Count {
        objects.len() as f64
    } else {
        let values: Vec<f64> = objects
            .iter()
            .filter_map(|obj| {
                field.and_then(|f| {
                    let Ok(body) = serde_json::from_str::<serde_json::Value>(&obj.body) else {
                        return None;
                    };
                    body.get(f).and_then(serde_json::Value::as_f64)
                })
            })
            .collect();

        match function {
            AggregateFunction::Sum => values.iter().sum(),
            AggregateFunction::Avg => {
                if values.is_empty() {
                    0.0
                } else {
                    values.iter().sum::<f64>() / values.len() as f64
                }
            },
            AggregateFunction::Min => values.iter().copied().fold(f64::MAX, f64::min),
            AggregateFunction::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            _ => values.len() as f64,
        }
    }
}

fn infer_json_type(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(_) => "boolean".to_string(),
        serde_json::Value::Number(_) => "number".to_string(),
        serde_json::Value::String(s) => {
            if s.parse::<chrono::DateTime<chrono::Utc>>().is_ok() {
                "date".to_string()
            } else {
                "string".to_string()
            }
        },
        serde_json::Value::Array(_) => "array".to_string(),
        serde_json::Value::Object(_) => "object".to_string(),
    }
}

fn bucket_label_for_date(iso_date: &str, bucket: TimeBucket) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(iso_date) {
        match bucket {
            TimeBucket::Hour => dt.format("%Y-%m-%dT%H:00:00Z").to_string(),
            TimeBucket::Day => dt.format("%Y-%m-%d").to_string(),
            TimeBucket::Week => dt.format("%Y-W%V").to_string(),
            TimeBucket::Month => dt.format("%Y-%m").to_string(),
        }
    } else {
        iso_date.to_string()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::cast_precision_loss)]
mod tests {
    use super::*;
    #[cfg(feature = "vectors")]
    use crate::VectorSearchOptions;
    #[cfg(feature = "vectors")]
    use crate::store::VectorStore;
    use crate::store::{AggregateStore, EventLog, LinkStore, ObjectStore, QueueStore, Searcher};
    use crate::{
        Link, ListObjectsOptions, MemoryEvent, MemoryObject, QueueClaimOptions, QueueJob,
        QueueJobStatus, QueueNackOptions, SearchOptions, TimeBucket,
    };
    use std::sync::{Mutex as TestMutex, OnceLock};

    /// Create a test engine with a temp directory that stays alive for the caller.
    fn setup() -> (PersistentEngine, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let engine = PersistentEngine::open(dir.path()).unwrap();
        (engine, dir)
    }

    #[test]
    fn persistent_open_writes_and_validates_storage_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let _engine = PersistentEngine::open(dir.path()).unwrap();
        let report = PersistentEngine::validate_path(dir.path()).unwrap();
        assert_eq!(report.format_version, STORAGE_FORMAT_VERSION);
        assert!(!report.legacy_manifest);
        assert!(report.lock_present);
        assert!(report.keyspaces_present);
        assert!(dir.path().join(STORAGE_MANIFEST_FILE).is_file());
    }

    #[test]
    fn persistent_retention_is_dry_run_by_default_and_preserves_protected_events() {
        let (mut engine, _dir) = setup();
        let mut old = MemoryEvent::new("old", "test", "{}");
        old.created_at = "2020-01-01T00:00:00Z".to_string();
        engine.append_event(old).unwrap();
        let mut protected = MemoryEvent::new("__thingd:system", "test", "{}");
        protected.created_at = "2020-01-01T00:00:00Z".to_string();
        engine.append_event(protected).unwrap();
        let mut replication = MemoryEvent::new(REPLICATION_STREAM, "object.upsert", "{}");
        replication.created_at = "2020-01-01T00:00:00Z".to_string();
        engine.append_event(replication).unwrap();

        let preview = engine
            .retain(RetentionOptions {
                before_unix_ms: 1_700_000_000_000,
                dry_run: true,
                compact: false,
                include_replication: false,
            })
            .unwrap();
        assert_eq!(preview.events, 1);
        assert_eq!(preview.skipped_replication_events, 1);
        assert_eq!(preview.safe_replication_cursor, None);
        assert_eq!(engine.count_events().unwrap(), 3);

        engine
            .put_object(MemoryObject::new(
                REPLICATION_STATE_COLLECTION,
                "source:replica-a",
                r#"{"sourceId":"replica-a","lastAppliedCursor":1}"#,
            ))
            .unwrap();
        let checkpointed = engine
            .retain(RetentionOptions {
                before_unix_ms: 1_700_000_000_000,
                dry_run: true,
                compact: false,
                include_replication: true,
            })
            .unwrap();
        assert_eq!(checkpointed.events, 2);
        assert_eq!(checkpointed.safe_replication_cursor, Some(1));

        let deleted = engine
            .retain(RetentionOptions {
                before_unix_ms: 1_700_000_000_000,
                dry_run: false,
                compact: true,
                include_replication: false,
            })
            .unwrap();
        assert_eq!(deleted.events, 1);
        assert!(deleted.compacted);
        assert_eq!(engine.count_events().unwrap(), 2);
        assert_eq!(
            engine
                .list_events(Some("__thingd:system"), ListEventsOptions::default())
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn validation_rejects_existing_directory_without_lock() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(STORAGE_KEYSPACES_DIR)).unwrap();
        std::fs::write(dir.path().join("0.jnl"), []).unwrap();
        let result = PersistentEngine::validate_path(dir.path());
        assert!(matches!(
            result,
            Err(ThingdError::StorageValidation(message)) if message.contains("lock")
        ));
    }

    #[test]
    fn validation_rejects_legacy_storage_directory_for_rocksdb_runtime() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(STORAGE_KEYSPACES_DIR)).unwrap();
        std::fs::write(dir.path().join(STORAGE_LOCK_FILE), []).unwrap();
        std::fs::write(dir.path().join("0.jnl"), []).unwrap();
        let result = PersistentEngine::validate_path(dir.path());
        assert!(matches!(
            result,
            Err(ThingdError::UnsupportedStorageFormat(message))
                if message.contains("legacy storage format")
        ));
    }

    #[test]
    fn validation_rejects_unsupported_manifest() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(STORAGE_KEYSPACES_DIR)).unwrap();
        std::fs::write(dir.path().join(STORAGE_LOCK_FILE), []).unwrap();
        std::fs::write(
            dir.path().join(STORAGE_MANIFEST_FILE),
            r#"{"format_version":99,"contract":"future","keyspaces":[],"search_schema_version":1}"#,
        )
        .unwrap();
        let result = PersistentEngine::validate_path(dir.path());
        assert!(matches!(
            result,
            Err(ThingdError::UnsupportedStorageFormat(message)) if message.contains("format version")
        ));
    }

    #[cfg(feature = "search")]
    #[test]
    fn disabled_search_mode_does_not_create_search_directory() {
        let dir = tempfile::tempdir().unwrap();
        let options = PersistentOpenOptions {
            search_mode: PersistentSearchMode::Disabled,
            ..PersistentOpenOptions::default()
        };
        let mut engine = PersistentEngine::open_with_options(dir.path(), options).unwrap();
        engine
            .put_object(MemoryObject::new("notes", "one", r#"{"text":"hello"}"#))
            .unwrap();
        assert!(!dir.path().join("search").exists());
        assert_eq!(
            engine
                .search("hello", SearchOptions::default())
                .unwrap()
                .len(),
            1
        );
    }

    #[cfg(feature = "search")]
    #[test]
    fn no_rebuild_search_mode_uses_fallback_without_an_index() {
        let dir = tempfile::tempdir().unwrap();
        let options = PersistentOpenOptions {
            search_mode: PersistentSearchMode::PersistentNoRebuild,
            ..PersistentOpenOptions::default()
        };
        let mut engine = PersistentEngine::open_with_options(dir.path(), options.clone()).unwrap();
        engine
            .put_object(MemoryObject::new("notes", "one", r#"{"text":"hello"}"#))
            .unwrap();
        drop(engine);
        let reopened = PersistentEngine::open_with_options(dir.path(), options).unwrap();
        assert!(!dir.path().join("search").exists());
        assert_eq!(
            reopened
                .search("hello", SearchOptions::default())
                .unwrap()
                .len(),
            1
        );
    }

    #[cfg(feature = "search")]
    #[test]
    fn recovery_search_mode_defers_index_creation_until_maintenance() {
        let dir = tempfile::tempdir().unwrap();
        let engine = PersistentEngine::open_with_options(
            dir.path(),
            PersistentOpenOptions {
                search_mode: PersistentSearchMode::PersistentRecovery,
                ..PersistentOpenOptions::default()
            },
        )
        .unwrap();
        assert!(engine.search_rebuild_required());
        assert!(!dir.path().join("search").exists());
        assert_eq!(
            engine.storage_maintenance_status().state,
            "rebuilding_search"
        );
    }

    #[cfg(feature = "search")]
    #[test]
    fn recovery_search_mode_reuses_compatible_index_without_rebuilding() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            engine
                .put_object(MemoryObject::new("notes", "one", r#"{"text":"ready"}"#))
                .unwrap();
        }
        let engine = PersistentEngine::open_with_options(
            dir.path(),
            PersistentOpenOptions {
                search_mode: PersistentSearchMode::PersistentRecovery,
                ..PersistentOpenOptions::default()
            },
        )
        .unwrap();
        assert!(!engine.search_rebuild_required());
        assert_eq!(engine.storage_maintenance_status().state, "idle");
    }

    #[cfg(feature = "search")]
    #[test]
    fn async_search_rebuild_interleaves_writes_and_replays_mutations() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            engine
                .put_object(MemoryObject::new("notes", "one", r#"{"text":"before"}"#))
                .unwrap();
        }
        std::fs::remove_dir_all(dir.path().join("search")).unwrap();

        let options = PersistentOpenOptions {
            search_mode: PersistentSearchMode::PersistentAsync,
            ..PersistentOpenOptions::default()
        };
        let mut engine = PersistentEngine::open_with_options(dir.path(), options).unwrap();
        assert!(engine.search_rebuild_required());
        engine.search_rebuild_step(1).unwrap();
        engine
            .put_object(MemoryObject::new("notes", "two", r#"{"text":"during"}"#))
            .unwrap();
        while !engine.search_rebuild_step(1).unwrap() {}

        let hits = engine.search("during", SearchOptions::default()).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(!engine.search_rebuild_required());
    }

    #[cfg(feature = "search")]
    #[test]
    fn async_search_rebuild_overflow_enters_degraded_state_without_rescanning() {
        let dir = tempfile::tempdir().unwrap();
        let options = PersistentOpenOptions {
            search_mode: PersistentSearchMode::PersistentAsync,
            ..PersistentOpenOptions::default()
        };
        let mut engine = PersistentEngine::open_with_options(dir.path(), options).unwrap();
        engine.search_rebuild_step(1).unwrap();
        engine.search_rebuild.as_mut().unwrap().phase = SearchRebuildPhase::Replay;
        for index in 0..=SEARCH_REBUILD_REPLAY_LIMIT {
            engine.record_search_mutation(
                format!("object:notes/{index}"),
                SearchReplayMutation::UpsertObject,
            );
        }
        assert!(!engine.search_rebuild_step(1).unwrap());
        let status = engine.search_rebuild_status().unwrap();
        assert_eq!(status.state, "degraded");
        assert_eq!(status.processed, 0);
        assert_eq!(engine.storage_maintenance_status().state, "degraded");
    }

    #[cfg(feature = "search")]
    #[test]
    fn async_search_rebuild_marker_forces_restart_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let options = PersistentOpenOptions {
            search_mode: PersistentSearchMode::PersistentAsync,
            ..PersistentOpenOptions::default()
        };
        {
            let mut engine =
                PersistentEngine::open_with_options(dir.path(), options.clone()).unwrap();
            engine.search_rebuild_step(1).unwrap();
            assert!(dir.path().join(".thingd-search-rebuild").exists());
        }
        let reopened = PersistentEngine::open_with_options(dir.path(), options).unwrap();
        assert!(reopened.search_rebuild_required());
        assert_eq!(
            reopened.storage_maintenance_status().state,
            "rebuilding_search"
        );
    }

    #[test]
    fn compact_storage_reports_maintenance_completion() {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = PersistentEngine::open_with_options(
            dir.path(),
            PersistentOpenOptions {
                search_mode: PersistentSearchMode::Disabled,
                ..PersistentOpenOptions::default()
            },
        )
        .unwrap();
        engine
            .put_object(MemoryObject::new("notes", "one", r#"{"text":"hello"}"#))
            .unwrap();
        engine.compact_storage().unwrap();
        assert_eq!(engine.storage_maintenance_status().state, "idle");
        assert!(engine.journal_count() >= 1);
    }

    #[test]
    fn repack_preserves_primary_records_and_rejects_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let destination = dir.path().join("destination");
        let mut engine = PersistentEngine::open_with_options(
            &source,
            PersistentOpenOptions {
                search_mode: PersistentSearchMode::Disabled,
                ..PersistentOpenOptions::default()
            },
        )
        .unwrap();
        let object = MemoryObject::new("notes", "one", r#"{"text":"hello"}"#);
        let stored_object = engine.put_object(object).unwrap();
        engine
            .append_event(MemoryEvent::new("audit", "created", r#"{"id":"one"}"#))
            .unwrap();
        drop(engine);

        PersistentEngine::repack_to(&source, &destination, None).unwrap();
        let repacked = PersistentEngine::open_with_options(
            &destination,
            PersistentOpenOptions {
                search_mode: PersistentSearchMode::Disabled,
                ..PersistentOpenOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            repacked.get_object("notes", "one").unwrap(),
            Some(stored_object)
        );
        assert_eq!(repacked.count_events().unwrap(), 1);
        assert!(matches!(
            PersistentEngine::repack_to(&source, &destination, None),
            Err(ThingdError::Conflict(message)) if message.contains("already exists")
        ));
    }

    #[test]
    fn encrypted_database_reopens_and_rejects_missing_or_wrong_keys() {
        let dir = tempfile::tempdir().unwrap();
        let key = [9_u8; 32];
        let options = PersistentOpenOptions {
            encryption: Some(EncryptionConfig::from_key(&key).unwrap()),
            ..PersistentOpenOptions::default()
        };
        {
            let mut engine =
                PersistentEngine::open_with_options(dir.path(), options.clone()).unwrap();
            engine
                .put_object(MemoryObject::new("private", "id", r#"{"secret":"value"}"#))
                .unwrap();
        }
        let missing =
            PersistentEngine::open_with_options(dir.path(), PersistentOpenOptions::default());
        assert!(matches!(missing, Err(ThingdError::EncryptionRequired(_))));
        let wrong = PersistentEngine::open_with_options(
            dir.path(),
            PersistentOpenOptions {
                encryption: Some(EncryptionConfig::from_key(&[8_u8; 32]).unwrap()),
                ..PersistentOpenOptions::default()
            },
        );
        assert!(matches!(
            wrong,
            Err(ThingdError::EncryptionAuthentication(_))
        ));
        let engine = PersistentEngine::open_with_options(dir.path(), options).unwrap();
        assert_eq!(
            engine.get_object("private", "id").unwrap().unwrap().body,
            r#"{"secret":"value"}"#
        );
    }

    #[test]
    fn encrypted_thingdb_reopens_and_rejects_missing_or_wrong_keys() {
        let dir = tempfile::tempdir().unwrap();
        let key = [0x2a_u8; 32];
        let options = PersistentOpenOptions {
            backend: PersistentBackend::ThingDb,
            encryption: Some(EncryptionConfig::from_key(&key).unwrap()),
            ..PersistentOpenOptions::default()
        };
        {
            let mut engine = PersistentEngine::open_with_options(&dir, options.clone()).unwrap();
            engine
                .put_object(MemoryObject::new(
                    "private",
                    "id",
                    r#"{"secret":"thingdb"}"#,
                ))
                .unwrap();
        }
        assert!(matches!(
            PersistentEngine::open_with_options(
                &dir,
                PersistentOpenOptions {
                    backend: PersistentBackend::ThingDb,
                    ..PersistentOpenOptions::default()
                }
            ),
            Err(ThingdError::EncryptionRequired(_))
        ));
        assert!(matches!(
            PersistentEngine::open_with_options(
                &dir,
                PersistentOpenOptions {
                    backend: PersistentBackend::ThingDb,
                    encryption: Some(EncryptionConfig::from_key(&[0x2b_u8; 32]).unwrap()),
                    ..PersistentOpenOptions::default()
                }
            ),
            Err(ThingdError::EncryptionAuthentication(_))
        ));
        let engine = PersistentEngine::open_with_options(&dir, options).unwrap();
        assert_eq!(
            engine.get_object("private", "id").unwrap().unwrap().body,
            r#"{"secret":"thingdb"}"#
        );
    }

    #[cfg(feature = "search")]
    #[test]
    fn encrypted_search_rebuilds_in_memory_without_search_directory() {
        let dir = tempfile::tempdir().unwrap();
        let key = [7_u8; 32];
        let options = PersistentOpenOptions {
            encryption: Some(EncryptionConfig::from_key(&key).unwrap()),
            ..PersistentOpenOptions::default()
        };
        {
            let mut engine =
                PersistentEngine::open_with_options(dir.path(), options.clone()).unwrap();
            engine
                .put_object(MemoryObject::new(
                    "private_collection",
                    "private_id",
                    r#"{"body":"encrypted_search_term"}"#,
                ))
                .unwrap();
            engine
                .append_event(MemoryEvent::new(
                    "private_stream",
                    "event-id",
                    "encrypted event",
                ))
                .unwrap();
            engine
                .push_job(QueueJob::new(
                    "private_queue",
                    "private_job",
                    "private payload",
                    2,
                ))
                .unwrap();
            engine
                .put_object(MemoryObject::new("private_nodes", "a", "{}"))
                .unwrap();
            engine
                .put_object(MemoryObject::new("private_nodes", "b", "{}"))
                .unwrap();
            engine
                .create_link(Link::new(
                    "private_nodes/a",
                    "private_link",
                    "private_nodes/b",
                ))
                .unwrap();
            assert_eq!(
                engine
                    .search("encrypted_search_term", SearchOptions::default())
                    .unwrap()
                    .len(),
                1
            );
        }
        assert!(!dir.path().join("search").exists());
        for bytes in walk_files(dir.path()) {
            assert!(
                !bytes
                    .windows("private_collection".len())
                    .any(|w| w == b"private_collection")
            );
            assert!(
                !bytes
                    .windows("private_id".len())
                    .any(|w| w == b"private_id")
            );
            assert!(
                !bytes
                    .windows("encrypted_search_term".len())
                    .any(|w| w == b"encrypted_search_term")
            );
            for identifier in [
                "private_stream",
                "private_queue",
                "private_job",
                "private_nodes",
                "private_link",
                "private payload",
            ] {
                assert!(
                    !bytes
                        .windows(identifier.len())
                        .any(|w| w == identifier.as_bytes())
                );
            }
        }

        let mut reopened = PersistentEngine::open_with_options(dir.path(), options).unwrap();
        assert_eq!(
            reopened
                .search("encrypted_search_term", SearchOptions::default())
                .unwrap()
                .len(),
            1
        );
        reopened
            .delete_object("private_collection", "private_id")
            .unwrap();
        assert!(
            reopened
                .search("encrypted_search_term", SearchOptions::default())
                .unwrap()
                .is_empty()
        );
    }

    fn walk_files(path: &std::path::Path) -> Vec<Vec<u8>> {
        let mut files = Vec::new();
        let Ok(entries) = std::fs::read_dir(path) else {
            return files;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(walk_files(&path));
            } else if let Ok(bytes) = std::fs::read(path) {
                files.push(bytes);
            }
        }
        files
    }

    #[test]
    fn reencrypts_without_modifying_source() {
        let root = tempfile::tempdir().unwrap();
        let source_path = root.path().join("source");
        let destination_path = root.path().join("encrypted");
        {
            let mut source = PersistentEngine::open(&source_path).unwrap();
            source
                .put_object(MemoryObject::new("users", "alice", r#"{"name":"Alice"}"#))
                .unwrap();
        }
        let destination_options = PersistentOpenOptions {
            encryption: Some(EncryptionConfig::from_key(&[3_u8; 32]).unwrap()),
            ..PersistentOpenOptions::default()
        };
        PersistentEngine::reencrypt_to(
            &source_path,
            &destination_path,
            PersistentOpenOptions::default(),
            destination_options.clone(),
        )
        .unwrap();
        let source = PersistentEngine::open(&source_path).unwrap();
        assert!(source.get_object("users", "alice").unwrap().is_some());
        let destination =
            PersistentEngine::open_with_options(&destination_path, destination_options).unwrap();
        assert_eq!(
            destination
                .get_object("users", "alice")
                .unwrap()
                .unwrap()
                .body,
            r#"{"name":"Alice"}"#
        );
    }

    #[test]
    fn repacks_rocksdb_into_thingdb_without_overwriting_source() {
        let root = tempfile::tempdir().unwrap();
        let source_path = root.path().join("rocksdb-source");
        let destination_path = root.path().join("thingdb-destination");
        {
            let mut source = PersistentEngine::open(&source_path).unwrap();
            source
                .put_object(MemoryObject::new("users", "alice", r#"{"name":"Alice"}"#))
                .unwrap();
            source
                .append_event(MemoryEvent::new("audit", "created", "body"))
                .unwrap();
        }

        PersistentEngine::repack_to_with_backends(
            &source_path,
            &destination_path,
            PersistentBackend::RocksDb,
            PersistentBackend::ThingDb,
            None,
        )
        .unwrap();

        let destination = PersistentEngine::open_with_options(
            &destination_path,
            PersistentOpenOptions {
                backend: PersistentBackend::ThingDb,
                ..PersistentOpenOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            destination
                .get_object("users", "alice")
                .unwrap()
                .unwrap()
                .body,
            r#"{"name":"Alice"}"#
        );
        assert_eq!(
            destination
                .list_events(Some("audit"), ListEventsOptions::default())
                .unwrap()
                .len(),
            1
        );
        assert!(source_path.join("CURRENT").is_file());
        assert!(destination_path.join("MANIFEST.json").is_file());
    }

    #[test]
    fn rotates_encrypted_database_without_overwriting_source() {
        let root = tempfile::tempdir().unwrap();
        let source_path = root.path().join("source-encrypted");
        let destination_path = root.path().join("rotated-encrypted");
        let source_options = PersistentOpenOptions {
            encryption: Some(EncryptionConfig::from_key(&[0x11_u8; 32]).unwrap()),
            ..PersistentOpenOptions::default()
        };
        let destination_options = PersistentOpenOptions {
            encryption: Some(EncryptionConfig::from_key(&[0x22_u8; 32]).unwrap()),
            ..PersistentOpenOptions::default()
        };
        {
            let mut source =
                PersistentEngine::open_with_options(&source_path, source_options.clone()).unwrap();
            source
                .put_object(MemoryObject::new("objects", "object", r#"{"value":true}"#))
                .unwrap();
            source
                .append_event(MemoryEvent::new("events", "created", "event body"))
                .unwrap();
            source
                .push_job(QueueJob::new("queue", "job", "payload", 2))
                .unwrap();
        }
        PersistentEngine::reencrypt_to(
            &source_path,
            &destination_path,
            source_options,
            destination_options.clone(),
        )
        .unwrap();

        let destination =
            PersistentEngine::open_with_options(&destination_path, destination_options).unwrap();
        assert!(
            destination
                .get_object("objects", "object")
                .unwrap()
                .is_some()
        );
        assert_eq!(
            destination
                .list_events(Some("events"), ListEventsOptions::default())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(destination.list_jobs("queue").unwrap().len(), 1);
        let old_key = PersistentEngine::open_with_options(
            &destination_path,
            PersistentOpenOptions {
                encryption: Some(EncryptionConfig::from_key(&[0x11_u8; 32]).unwrap()),
                ..PersistentOpenOptions::default()
            },
        );
        assert!(matches!(
            old_key,
            Err(ThingdError::EncryptionAuthentication(_))
        ));
        assert!(
            PersistentEngine::reencrypt_to(
                &source_path,
                root.path().join("plaintext-refused"),
                PersistentOpenOptions {
                    encryption: Some(EncryptionConfig::from_key(&[0x11_u8; 32]).unwrap()),
                    ..PersistentOpenOptions::default()
                },
                PersistentOpenOptions::default(),
            )
            .is_err()
        );
    }

    // ── ObjectStore ───────────────────────────────────────────────────────

    #[test]
    fn persistent_stores_and_reads_objects() {
        let (mut engine, _dir) = setup();
        let object = engine
            .put_object(MemoryObject::new(
                "decisions",
                "rust-core",
                r#"{"text":"Use Rust"}"#,
            ))
            .unwrap();
        let stored = engine
            .get_object("decisions", "rust-core")
            .unwrap()
            .unwrap();
        assert_eq!(object.version, 1);
        assert_eq!(stored.key.collection, "decisions");
        assert_eq!(stored.key.id, "rust-core");
    }

    #[test]
    fn persistent_object_created_at_preserved_on_update() {
        let (mut engine, _dir) = setup();
        let first = engine
            .put_object(MemoryObject::new("col", "id", r#"{"v":1}"#))
            .unwrap();
        assert!(!first.created_at.is_empty());
        let second = engine
            .put_object(MemoryObject::new("col", "id", r#"{"v":2}"#))
            .unwrap();
        assert_eq!(second.created_at, first.created_at);
        assert!(second.updated_at >= first.created_at);
    }

    #[test]
    fn persistent_object_version_increments_on_update() {
        let (mut engine, _dir) = setup();
        let v1 = engine
            .put_object(MemoryObject::new("col", "x", "{}"))
            .unwrap();
        assert_eq!(v1.version, 1);
        let v2 = engine
            .put_object(MemoryObject::new("col", "x", r#"{"v":2}"#))
            .unwrap();
        assert_eq!(v2.version, 2);
    }

    #[test]
    fn persistent_lists_objects_with_filter() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("w", "a", r#"{"color":"red","size":1}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("w", "b", r#"{"color":"blue","size":2}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("w", "c", r#"{"color":"red","size":3}"#))
            .unwrap();
        let opts = ListObjectsOptions {
            filter: vec![("color".into(), serde_json::json!("red"))],
            ..Default::default()
        };
        let results = engine
            .list_objects(Some(&["w".to_string()]), &opts)
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|o| o.body.contains("\"red\"")));
    }

    #[test]
    fn persistent_list_objects_pagination() {
        let (mut engine, _dir) = setup();
        for i in 0..5u32 {
            engine
                .put_object(MemoryObject::new("col", format!("id-{i}"), "{}"))
                .unwrap();
        }
        let limit_opts = ListObjectsOptions {
            limit: Some(3),
            ..Default::default()
        };
        assert_eq!(
            engine
                .list_objects(Some(&["col".to_string()]), &limit_opts)
                .unwrap()
                .len(),
            3
        );
        let offset_opts = ListObjectsOptions {
            offset: Some(3),
            ..Default::default()
        };
        assert_eq!(
            engine
                .list_objects(Some(&["col".to_string()]), &offset_opts)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn persistent_list_objects_sort_by_created_at_desc() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("w", "a", r#"{"x":1}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("w", "b", r#"{"x":2}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("w", "c", r#"{"x":3}"#))
            .unwrap();
        let opts = ListObjectsOptions {
            sort_by: Some(crate::SortBy::desc("created_at")),
            ..Default::default()
        };
        let results = engine
            .list_objects(Some(&["w".to_string()]), &opts)
            .unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn persistent_list_objects_sort_by_id_asc() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("w", "c", r#"{"x":3}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("w", "a", r#"{"x":1}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("w", "b", r#"{"x":2}"#))
            .unwrap();
        let opts = ListObjectsOptions {
            sort_by: Some(crate::SortBy::asc("id")),
            ..Default::default()
        };
        let results = engine
            .list_objects(Some(&["w".to_string()]), &opts)
            .unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].key.id, "a");
        assert_eq!(results[1].key.id, "b");
        assert_eq!(results[2].key.id, "c");
    }

    #[test]
    fn persistent_cas_succeeds_on_matching_version() {
        let (mut engine, _dir) = setup();
        let stored = engine
            .put_object(MemoryObject::new("col", "id", r#"{"v":1}"#))
            .unwrap();
        assert_eq!(stored.version, 1);
        let opts = crate::PutObjectOptions {
            expected_version: Some(1),
            ..Default::default()
        };
        let updated = engine
            .put_object_with_options(MemoryObject::new("col", "id", r#"{"v":2}"#), opts)
            .unwrap();
        assert_eq!(updated.version, 2);
    }

    #[test]
    fn persistent_cas_fails_on_version_mismatch() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("col", "id", r#"{"v":1}"#))
            .unwrap();
        let opts = crate::PutObjectOptions {
            expected_version: Some(42),
            ..Default::default()
        };
        let err = engine
            .put_object_with_options(MemoryObject::new("col", "id", r#"{"v":2}"#), opts)
            .unwrap_err();
        assert!(matches!(err, crate::ThingdError::Conflict(_)));
    }

    #[test]
    fn persistent_cas_fails_on_nonexistent_object() {
        let (mut engine, _dir) = setup();
        let opts = crate::PutObjectOptions {
            expected_version: Some(1),
            ..Default::default()
        };
        let err = engine
            .put_object_with_options(MemoryObject::new("col", "id", r#"{"v":1}"#), opts)
            .unwrap_err();
        assert!(matches!(err, crate::ThingdError::Conflict(_)));
    }

    #[test]
    fn persistent_delete_objects_batch() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("w", "a", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("w", "b", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("w", "c", "{}"))
            .unwrap();
        let keys = vec![
            ("w".to_string(), "a".to_string()),
            ("w".to_string(), "b".to_string()),
        ];
        let deleted = engine.delete_objects_batch(&keys).unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(engine.count_objects().unwrap(), 1);
        assert!(engine.get_object("w", "a").unwrap().is_none());
        assert!(engine.get_object("w", "c").unwrap().is_some());
    }

    // ── EventLog ──────────────────────────────────────────────────────────

    #[test]
    fn persistent_appends_events_with_sequence_numbers() {
        let (mut engine, _dir) = setup();
        let event = engine
            .append_event(MemoryEvent::new(
                "project:thingd",
                "decision.made",
                "MCP-native object storage",
            ))
            .unwrap();
        assert_eq!(event.sequence, 1);
        assert_eq!(
            engine
                .list_events(Some("project:thingd"), ListEventsOptions::default())
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn persistent_event_idempotency() {
        let (mut engine, _dir) = setup();
        let mut event = MemoryEvent::new("stream", "test", r#"{"key":"val"}"#);
        event.idempotency_key = "idem-1".to_string();
        let first = engine.append_event(event.clone()).unwrap();
        assert_eq!(first.sequence, 1);
        let second = engine.append_event(event).unwrap();
        assert_eq!(second.sequence, first.sequence);
        assert_eq!(second.body, first.body);
    }

    #[test]
    fn persistent_deletes_last_event_from_stream() {
        let (mut engine, _dir) = setup();
        engine
            .append_event(MemoryEvent::new("match:1", "turn.recorded", "{}"))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("match:1", "turn.recorded", "{}"))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("match:2", "turn.recorded", "{}"))
            .unwrap();
        let deleted = engine.delete_last_event("match:1").unwrap().unwrap();
        assert_eq!(deleted.sequence, 2);
        let remaining = engine
            .list_events(Some("match:1"), ListEventsOptions::default())
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].sequence, 1);
        let match2 = engine
            .list_events(Some("match:2"), ListEventsOptions::default())
            .unwrap();
        assert_eq!(match2.len(), 1);
    }

    #[test]
    fn persistent_deletes_stream_and_returns_count() {
        let (mut engine, _dir) = setup();
        engine
            .append_event(MemoryEvent::new("match:1", "turn.recorded", "{}"))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("match:1", "turn.recorded", "{}"))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("match:2", "turn.recorded", "{}"))
            .unwrap();
        assert_eq!(engine.delete_stream("match:1").unwrap(), 2);
        assert_eq!(
            engine
                .list_events(Some("match:1"), ListEventsOptions::default())
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            engine
                .list_events(Some("match:2"), ListEventsOptions::default())
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn persistent_lists_streams() {
        let (mut engine, _dir) = setup();
        assert!(engine.list_streams().unwrap().is_empty());
        engine
            .append_event(MemoryEvent::new("s1", "t", "e1"))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("s2", "t", "e2"))
            .unwrap();
        let mut streams = engine.list_streams().unwrap();
        streams.sort();
        assert_eq!(streams, vec!["s1", "s2"]);
    }

    // ── QueueStore ────────────────────────────────────────────────────────

    #[test]
    fn persistent_claims_and_acks_queue_jobs() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("embed", "job-1", "doc-1", 3))
            .unwrap();
        let claimed = engine.claim_job("embed").unwrap().unwrap();
        let acked = engine.ack_job("embed", "job-1").unwrap().unwrap();
        assert_eq!(claimed.status, QueueJobStatus::Leased);
        assert_eq!(claimed.attempts, 1);
        assert_eq!(acked.status, QueueJobStatus::Completed);
    }

    #[test]
    fn persistent_queue_lease_index_rebuilds_after_reopen() {
        let (mut engine, dir) = setup();
        engine
            .push_job(QueueJob::new("embed", "job-1", "doc-1", 3))
            .unwrap();
        engine
            .claim_job_with_options("embed", QueueClaimOptions::new(0))
            .unwrap()
            .unwrap();
        drop(engine);

        let mut reopened = PersistentEngine::open(dir.path()).unwrap();
        let reclaimed = reopened
            .claim_job_with_options("embed", QueueClaimOptions::new(0))
            .unwrap()
            .unwrap();
        assert_eq!(reclaimed.id, "job-1");
        assert!(reopened.queue_diagnostics().lease_entries_examined >= 1);
    }

    #[test]
    fn persistent_queue_expiry_work_scales_with_active_leases() {
        let (mut engine, _dir) = setup();
        let historical_jobs = 256;
        let jobs = (0..historical_jobs)
            .map(|index| QueueJob::new("embed", format!("historical-{index}"), "doc", 3))
            .collect();
        engine.push_jobs_batch(jobs).unwrap();

        for _ in 0..historical_jobs {
            let claimed = engine.claim_job("embed").unwrap().unwrap();
            engine.ack_job("embed", &claimed.id).unwrap().unwrap();
        }

        engine
            .push_job(QueueJob::new("embed", "active", "doc", 3))
            .unwrap();
        engine
            .claim_job_with_options("embed", QueueClaimOptions::new(0))
            .unwrap()
            .unwrap();
        engine
            .claim_job_with_options("embed", QueueClaimOptions::new(0))
            .unwrap()
            .unwrap();

        let diagnostics = engine.queue_diagnostics();
        assert_eq!(diagnostics.lease_entries_examined, 1);
        assert!(diagnostics.lease_entries_examined < historical_jobs as u64);
    }

    #[test]
    fn persistent_nacks_to_dead_letter_after_max_attempts() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("embed", "job-1", "doc-1", 1))
            .unwrap();
        engine.claim_job("embed").unwrap().unwrap();
        let nacked = engine.nack_job("embed", "job-1").unwrap().unwrap();
        assert_eq!(nacked.status, QueueJobStatus::Dead);
        assert_eq!(engine.list_dead_jobs("embed").unwrap().len(), 1);
    }

    #[test]
    fn persistent_does_not_claim_delayed_jobs() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("embed", "job-1", "doc-1", 3).delay_by_ms(60_000))
            .unwrap();
        assert!(engine.claim_job("embed").unwrap().is_none());
    }

    #[test]
    fn persistent_nacks_with_retry_delay() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("embed", "job-1", "doc-1", 3))
            .unwrap();
        engine.claim_job("embed").unwrap().unwrap();
        let retried = engine
            .nack_job_with_options("embed", "job-1", QueueNackOptions::new(60_000))
            .unwrap()
            .unwrap();
        assert_eq!(retried.status, QueueJobStatus::Ready);
        assert!(engine.claim_job("embed").unwrap().is_none());
    }

    #[test]
    fn persistent_queue_counts() {
        let (mut engine, _dir) = setup();
        assert_eq!(engine.count_active_jobs().unwrap(), 0);
        assert_eq!(engine.count_dead_jobs().unwrap(), 0);
        engine
            .push_job(QueueJob::new("work", "j1", "p1", 3))
            .unwrap();
        engine
            .push_job(QueueJob::new("work", "j2", "p2", 3))
            .unwrap();
        engine
            .push_job(QueueJob::new("other", "j3", "p3", 1))
            .unwrap();
        assert_eq!(engine.count_active_jobs().unwrap(), 3);
        engine.claim_job("other").unwrap();
        engine.nack_job("other", "j3").unwrap();
        assert_eq!(engine.count_dead_jobs().unwrap(), 1);
        assert_eq!(engine.count_active_jobs().unwrap(), 2);
    }

    #[test]
    fn persistent_lists_queues() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("work", "j1", "p1", 3))
            .unwrap();
        engine
            .push_job(QueueJob::new("jobs", "j2", "p2", 3))
            .unwrap();
        let mut queues = engine.list_queues().unwrap();
        queues.sort();
        assert_eq!(queues, vec!["jobs", "work"]);
    }

    #[test]
    fn persistent_claim_reclaims_expired_lease() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("embed", "job-1", "doc-1", 3))
            .unwrap();
        let first = engine
            .claim_job_with_options("embed", QueueClaimOptions::new(0))
            .unwrap()
            .unwrap();
        let second = engine.claim_job("embed").unwrap().unwrap();
        assert_eq!(first.status, QueueJobStatus::Leased);
        assert_eq!(second.status, QueueJobStatus::Leased);
        assert_eq!(second.attempts, 2);
    }

    #[test]
    fn persistent_priority_ordering() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("q", "low", "body", 3).with_priority(0))
            .unwrap();
        engine
            .push_job(QueueJob::new("q", "high", "body", 3).with_priority(10))
            .unwrap();
        engine
            .push_job(QueueJob::new("q", "mid", "body", 3).with_priority(5))
            .unwrap();
        let first = engine.claim_job("q").unwrap().unwrap();
        assert_eq!(first.id, "high", "highest priority claimed first");
        let second = engine.claim_job("q").unwrap().unwrap();
        assert_eq!(second.id, "mid", "medium priority claimed second");
        let third = engine.claim_job("q").unwrap().unwrap();
        assert_eq!(third.id, "low", "lowest priority claimed last");
    }

    // ── LinkStore ─────────────────────────────────────────────────────────

    #[test]
    fn persistent_create_get_delete_link() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("n", "a", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("n", "b", "{}"))
            .unwrap();
        let link = engine
            .create_link(Link::new("n/a", "connects", "n/b"))
            .unwrap();
        assert!(!link.id.is_empty());
        let fetched = engine.get_link(&link.id).unwrap().unwrap();
        assert_eq!(fetched.id, link.id);
        assert!(engine.delete_link(&link.id).unwrap());
        assert!(engine.get_link(&link.id).unwrap().is_none());
    }

    #[test]
    fn persistent_neighbor_query() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("n", "a", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("n", "b", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("n", "c", "{}"))
            .unwrap();
        engine
            .create_link(Link::new("n/a", "knows", "n/b"))
            .unwrap();
        engine
            .create_link(Link::new("n/a", "knows", "n/c"))
            .unwrap();
        let outgoing = engine
            .get_neighbors("n/a", LinkDirection::Outgoing, LinkQueryOptions::default())
            .unwrap();
        assert_eq!(outgoing.len(), 2);
        let incoming = engine
            .get_neighbors("n/b", LinkDirection::Incoming, LinkQueryOptions::default())
            .unwrap();
        assert_eq!(incoming.len(), 1);
    }

    #[test]
    fn persistent_link_count() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("n", "a", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("n", "b", "{}"))
            .unwrap();
        assert_eq!(engine.count_links().unwrap(), 0);
        engine
            .create_link(Link::new("n/a", "knows", "n/b"))
            .unwrap();
        assert_eq!(engine.count_links().unwrap(), 1);
    }

    // ── Searcher (naive — Tantivy is feature-gated) ──────────────────────

    #[test]
    fn persistent_search_objects_and_events() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("docs", "a", r#"{"text":"hello world"}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new(
                "docs",
                "b",
                r#"{"text":"goodbye world"}"#,
            ))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("audit", "test", "hello event"))
            .unwrap();
        let results = engine.search("hello", SearchOptions::default()).unwrap();
        assert_eq!(results.len(), 2);
        let kinds: Vec<&str> = results.iter().map(|h| h.kind.as_str()).collect();
        assert!(kinds.contains(&"object"));
        assert!(kinds.contains(&"event"));
    }

    #[test]
    fn persistent_search_with_collections() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("docs", "a", r#"{"text":"hello world"}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("notes", "b", r#"{"text":"hello there"}"#))
            .unwrap();
        let opts = SearchOptions {
            collections: Some(vec!["docs".into()]),
            ..Default::default()
        };
        let results = engine.search("hello", opts).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].collection, "docs");
    }

    // ── Tantivy search (feature-gated) ──────────────────────────────────

    #[cfg(feature = "search")]
    #[test]
    fn persistent_search_indexes_on_put() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new(
                "docs",
                "a",
                r#"{"text":"unique_search_term_xyz"}"#,
            ))
            .unwrap();
        let results = engine
            .search("unique_search_term_xyz", SearchOptions::default())
            .unwrap();
        assert_eq!(
            results.len(),
            1,
            "search must find indexed content immediately after put"
        );
        assert_eq!(results[0].id, "a");
    }

    #[cfg(feature = "search")]
    #[test]
    fn async_search_coalesces_mutations_without_timing_assumptions() {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = PersistentEngine::open_with_options(
            dir.path(),
            PersistentOpenOptions {
                search_commit_interval_ms: 5_000,
                search_commit_batch_size: 32,
                ..PersistentOpenOptions::default()
            },
        )
        .unwrap();

        for value in 0..4 {
            engine
                .put_object(MemoryObject::new(
                    "docs",
                    "same",
                    format!(r#"{{"text":"version-{value}"}}"#),
                ))
                .unwrap();
        }

        let queued = engine.storage_maintenance_status();
        assert!(queued.search_mutations_queued >= 4);
        assert!(queued.search_mutations_coalesced >= 1);
        assert!(queued.search_queue_depth <= 1);
        assert_eq!(
            engine
                .search("version-3", SearchOptions::default())
                .unwrap()
                .len(),
            1
        );
    }

    #[cfg(feature = "search")]
    #[test]
    fn async_search_commits_after_debounce_without_blocking_primary_writes() {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = PersistentEngine::open_with_options(
            dir.path(),
            PersistentOpenOptions {
                search_commit_interval_ms: 10,
                search_commit_batch_size: 32,
                ..PersistentOpenOptions::default()
            },
        )
        .unwrap();

        for index in 0..1_000 {
            engine
                .put_object(MemoryObject::new(
                    "bulk",
                    format!("object-{index}"),
                    format!(r#"{{"text":"bulk-term-{index}"}}"#),
                ))
                .unwrap();
        }
        assert_eq!(engine.count_objects().unwrap(), 1_000);

        std::thread::sleep(Duration::from_millis(200));
        let status = engine.storage_maintenance_status();
        assert!(status.search_mutations_queued >= 1_000);
        assert!(status.search_mutations_committed > 0);
        assert_eq!(
            engine
                .search("bulk-term-999", SearchOptions::default())
                .unwrap()
                .len(),
            1
        );
    }

    #[cfg(feature = "search")]
    #[test]
    fn async_search_queue_overflow_preserves_primary_writes() {
        let queue = SearchMutationQueue::new(1);
        let first = MemoryObject::new("docs", "one", r#"{"text":"one"}"#);
        let second = MemoryObject::new("docs", "two", r#"{"text":"two"}"#);
        assert!(queue.enqueue(
            "object:docs/one".to_string(),
            SearchIndexMutation::UpsertObject(first)
        ));
        assert!(!queue.enqueue(
            "object:docs/two".to_string(),
            SearchIndexMutation::UpsertObject(second)
        ));
        let snapshot = queue.snapshot();
        assert!(snapshot.stale);
        assert!(snapshot.pending.is_empty());
        assert_eq!(snapshot.queued, 2);
        assert!(snapshot.last_error.is_some());
    }

    #[cfg(feature = "search")]
    #[test]
    fn legacy_search_schema_is_rebuilt_without_panicking() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            engine
                .put_object(MemoryObject::new(
                    "legacy",
                    "object",
                    r#"{"text":"legacy_search_term"}"#,
                ))
                .unwrap();
        }

        let search_dir = dir.path().join("search");
        std::fs::remove_dir_all(&search_dir).unwrap();
        std::fs::create_dir_all(&search_dir).unwrap();
        let mut schema_builder = tantivy::schema::Schema::builder();
        schema_builder.add_text_field("body", tantivy::schema::TEXT | tantivy::schema::STORED);
        let legacy_schema = schema_builder.build();
        tantivy::Index::create_in_dir(&search_dir, legacy_schema).unwrap();

        let engine = PersistentEngine::open(dir.path()).unwrap();
        let results = engine
            .search("legacy_search_term", SearchOptions::default())
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "object");
    }

    #[cfg(feature = "search")]
    #[test]
    fn persistent_search_removes_on_delete() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new(
                "docs",
                "to-delete",
                r#"{"text":"deletable_content"}"#,
            ))
            .unwrap();
        // Should be findable after put
        assert_eq!(
            engine
                .search("deletable_content", SearchOptions::default())
                .unwrap()
                .len(),
            1
        );
        // Delete and verify it's gone from search
        engine.delete_object("docs", "to-delete").unwrap();
        let after = engine
            .search("deletable_content", SearchOptions::default())
            .unwrap();
        assert_eq!(
            after.len(),
            0,
            "deleted object must not appear in search results"
        );
    }

    #[cfg(feature = "search")]
    #[test]
    fn persistent_search_deleted_batch_removes_from_index() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new(
                "docs",
                "a",
                r#"{"text":"batch_deleted_a"}"#,
            ))
            .unwrap();
        engine
            .put_object(MemoryObject::new(
                "docs",
                "b",
                r#"{"text":"batch_deleted_b"}"#,
            ))
            .unwrap();
        assert_eq!(
            engine
                .search("batch_deleted", SearchOptions::default())
                .unwrap()
                .len(),
            2
        );
        let keys = vec![
            ("docs".to_string(), "a".to_string()),
            ("docs".to_string(), "b".to_string()),
        ];
        engine.delete_objects_batch(&keys).unwrap();
        let after = engine
            .search("batch_deleted", SearchOptions::default())
            .unwrap();
        assert_eq!(
            after.len(),
            0,
            "batch-deleted objects must be removed from search index"
        );
    }

    // ── AggregateStore ────────────────────────────────────────────────────

    #[test]
    fn persistent_aggregate_count_sum_avg() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("stats", "a", r#"{"val":10}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("stats", "b", r#"{"val":20}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("stats", "c", r#"{"val":30}"#))
            .unwrap();
        let count = engine
            .aggregate(
                "stats",
                &AggregateOptions {
                    function: AggregateFunction::Count,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(count.total, 3.0);
        let sum = engine
            .aggregate(
                "stats",
                &AggregateOptions {
                    function: AggregateFunction::Sum,
                    field: Some("val".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(sum.total, 60.0);
        let avg = engine
            .aggregate(
                "stats",
                &AggregateOptions {
                    function: AggregateFunction::Avg,
                    field: Some("val".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(avg.total, 20.0);
    }

    #[test]
    fn persistent_aggregate_group_by() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new(
                "sales",
                "a",
                r#"{"region":"EU","val":100}"#,
            ))
            .unwrap();
        engine
            .put_object(MemoryObject::new(
                "sales",
                "b",
                r#"{"region":"US","val":200}"#,
            ))
            .unwrap();
        engine
            .put_object(MemoryObject::new(
                "sales",
                "c",
                r#"{"region":"EU","val":50}"#,
            ))
            .unwrap();
        let result = engine
            .aggregate(
                "sales",
                &AggregateOptions {
                    function: AggregateFunction::Sum,
                    field: Some("val".into()),
                    group_by: Some("region".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(result.total, 350.0);
        assert_eq!(result.groups.len(), 2);
        for group in &result.groups {
            match group.key.as_str() {
                "EU" => assert_eq!(group.value, 150.0),
                "US" => assert_eq!(group.value, 200.0),
                _ => panic!("unexpected group key"),
            }
        }
    }

    #[test]
    fn persistent_timeseries_bucketing() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("events", "a", r#"{"val":1}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("events", "b", r#"{"val":2}"#))
            .unwrap();
        let result = engine
            .timeseries(
                "events",
                &TimeSeriesOptions {
                    function: AggregateFunction::Count,
                    bucket: TimeBucket::Day,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(result.buckets.len(), 1);
        assert_eq!(result.buckets[0].value, 2.0);
    }

    // ── ready_jobs index behavior (Persistent-specific) ───────────────────────

    #[test]
    fn persistent_ready_jobs_indexes_on_push() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("q", "j1", "body", 3))
            .unwrap();
        let prefix = b"q\0";
        let count = engine.ready_jobs.prefix(prefix).count();
        assert_eq!(count, 1, "ready_jobs must have one entry after push");
    }

    #[test]
    fn persistent_ready_jobs_removed_on_claim() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("q", "j1", "body", 3))
            .unwrap();
        engine.claim_job("q").unwrap();
        let prefix = b"q\0";
        let count = engine.ready_jobs.prefix(prefix).count();
        assert_eq!(
            count, 0,
            "ready_jobs must be empty after claiming the only job"
        );
    }

    #[test]
    fn persistent_ready_jobs_priority_order() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("q", "low", "body", 3).with_priority(0))
            .unwrap();
        engine
            .push_job(QueueJob::new("q", "high", "body", 3).with_priority(10))
            .unwrap();
        // ready_jobs should iterate in priority order (highest first)
        let prefix = b"q\0";
        let keys: Vec<Vec<u8>> = engine
            .ready_jobs
            .prefix(prefix)
            .map(|kv| {
                let (k, _) = guard_data(kv).unwrap();
                k
            })
            .collect();
        assert_eq!(keys.len(), 2);
        // First key should contain "high" — it has higher priority
        let first_key_str = String::from_utf8_lossy(&keys[0]);
        assert!(
            first_key_str.contains("high"),
            "first ready entry should be high-priority job; got {first_key_str}"
        );
    }

    #[test]
    fn persistent_ready_jobs_fifo_order() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("q", "first", "body", 3))
            .unwrap();
        // Slight delay so created_at differs
        std::thread::sleep(std::time::Duration::from_millis(5));
        engine
            .push_job(QueueJob::new("q", "second", "body", 3))
            .unwrap();
        let prefix = b"q\0";
        let keys: Vec<Vec<u8>> = engine
            .ready_jobs
            .prefix(prefix)
            .map(|kv| {
                let (k, _) = guard_data(kv).unwrap();
                k
            })
            .collect();
        assert_eq!(keys.len(), 2);
        let first_key_str = String::from_utf8_lossy(&keys[0]);
        assert!(
            first_key_str.contains("first"),
            "first ready entry should be FIFO; got {first_key_str}"
        );
    }

    #[test]
    fn persistent_ready_jobs_reindex_on_nack() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("q", "j1", "body", 3))
            .unwrap();
        engine.claim_job("q").unwrap();
        let prefix = b"q\0";
        assert_eq!(engine.ready_jobs.prefix(prefix).count(), 0);
        // Nack with no delay — should re-index into ready_jobs
        engine
            .nack_job_with_options("q", "j1", QueueNackOptions::new(0))
            .unwrap();
        assert_eq!(
            engine.ready_jobs.prefix(prefix).count(),
            1,
            "ready_jobs must have entry after nack with retry"
        );
    }

    #[test]
    fn persistent_ready_jobs_reindex_on_lease_expire() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("q", "j1", "body", 3))
            .unwrap();
        // Claim with zero lease so it immediately expires
        engine
            .claim_job_with_options("q", QueueClaimOptions::new(0))
            .unwrap();
        // The claim method should have reaped the expired lease and re-indexed
        let prefix = b"q\0";
        let _count = engine.ready_jobs.prefix(prefix).count();
        // claim_job called next will reap expired lease and return the job
        let claimed = engine.claim_job("q").unwrap();
        assert!(
            claimed.is_some(),
            "job should be claimable after lease expires"
        );
        let job = claimed.unwrap();
        assert_eq!(job.attempts, 2, "second attempt after lease expiry");
        assert_eq!(
            engine.ready_jobs.prefix(prefix).count(),
            0,
            "ready_jobs must be empty after re-claiming"
        );
    }

    // ── VectorStore ───────────────────────────────────────────────────────

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_search_returns_by_cosine_similarity() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(
                MemoryObject::new("docs", "a", r#"{"text":"alpha"}"#)
                    .with_vector(vec![1.0, 0.0, 0.0]),
            )
            .unwrap();
        engine
            .put_object(
                MemoryObject::new("docs", "b", r#"{"text":"beta"}"#)
                    .with_vector(vec![0.0, 1.0, 0.0]),
            )
            .unwrap();

        let results = engine
            .vector_search("docs", &[0.9, 0.1, 0.0], VectorSearchOptions::default())
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "a");
        assert!(results[0].score > results[1].score);
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_search_respects_filter() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(
                MemoryObject::new("docs", "a", r#"{"tag":"x"}"#).with_vector(vec![1.0, 0.0]),
            )
            .unwrap();
        engine
            .put_object(
                MemoryObject::new("docs", "b", r#"{"tag":"y"}"#).with_vector(vec![0.0, 1.0]),
            )
            .unwrap();

        let results = engine
            .vector_search(
                "docs",
                &[1.0, 0.0],
                VectorSearchOptions {
                    filter: Some(serde_json::json!({"tag": "x"})),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "a");
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_search_excludes_deleted_objects() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("docs", "a", "{}").with_vector(vec![1.0, 0.0]))
            .unwrap();
        engine.delete_object("docs", "a").unwrap();
        let results = engine
            .vector_search("docs", &[1.0, 0.0], VectorSearchOptions::default())
            .unwrap();
        assert!(results.is_empty());
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_search_respects_top_k() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(
                MemoryObject::new("docs", "a", r#"{"text":"alpha"}"#).with_vector(vec![1.0, 0.0]),
            )
            .unwrap();
        engine
            .put_object(
                MemoryObject::new("docs", "b", r#"{"text":"beta"}"#).with_vector(vec![0.0, 1.0]),
            )
            .unwrap();

        let results = engine
            .vector_search(
                "docs",
                &[1.0, 0.0],
                VectorSearchOptions {
                    top_k: Some(1),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "a");
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_search_empty_collection_returns_empty() {
        let (engine, _dir) = setup();
        let results = engine
            .vector_search("docs", &[1.0, 0.0, 0.0], VectorSearchOptions::default())
            .unwrap();
        assert!(results.is_empty());
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_search_rejects_dimension_mismatch() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("docs", "a", "{}").with_vector(vec![1.0, 0.0]))
            .unwrap();

        let error = engine
            .vector_search("docs", &[1.0, 0.0, 0.0], VectorSearchOptions::default())
            .unwrap_err();
        assert!(
            matches!(error, ThingdError::InvalidInput(message) if message.contains("dimension"))
        );
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_search_rejects_empty_query() {
        let (engine, _dir) = setup();
        let error = engine
            .vector_search("docs", &[], VectorSearchOptions::default())
            .unwrap_err();
        assert!(matches!(error, ThingdError::InvalidInput(message) if message.contains("empty")));
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_put_object_without_vector_does_not_store_vector() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("docs", "a", "{}"))
            .unwrap();
        let results = engine
            .vector_search("docs", &[1.0, 0.0, 0.0], VectorSearchOptions::default())
            .unwrap();
        assert!(results.is_empty());
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_search_persists_across_engine_reopen() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            engine
                .put_object(
                    MemoryObject::new("docs", "a", r#"{"text":"persist"}"#)
                        .with_vector(vec![1.0, 0.0, 0.0]),
                )
                .unwrap();
        }
        {
            let engine = PersistentEngine::open(dir.path()).unwrap();
            let results = engine
                .vector_search("docs", &[1.0, 0.0, 0.0], VectorSearchOptions::default())
                .unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].id, "a");
        }
    }

    // ── Reopen tests ─────────────────────────────────────────────────────────

    #[test]
    fn persistent_event_sequence_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let stream = "test-stream";
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            let e1 = engine
                .append_event(MemoryEvent::new(stream, "t1", "{}"))
                .unwrap();
            assert_eq!(e1.sequence, 1);
            let e2 = engine
                .append_event(MemoryEvent::new(stream, "t2", "{}"))
                .unwrap();
            assert_eq!(e2.sequence, 2);
        }
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            // Next event should continue at sequence 3
            let e3 = engine
                .append_event(MemoryEvent::new(stream, "t3", "{}"))
                .unwrap();
            assert_eq!(
                e3.sequence, 3,
                "sequence must continue from durable max after reopen"
            );
            // Sequence 1 should not be overwritten
            let events = engine
                .list_events(Some(stream), ListEventsOptions::default())
                .unwrap();
            assert_eq!(events.len(), 3);
        }
    }

    #[test]
    fn persistent_event_idempotency_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let stream = "test-stream";
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            let mut e = MemoryEvent::new(stream, "t1", r#"{"x":1}"#);
            e.idempotency_key = "key-1".to_string();
            let e1 = engine.append_event(e).unwrap();
            assert_eq!(e1.sequence, 1);
        }
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            // Same idempotency key — must return existing event, not duplicate
            let mut e = MemoryEvent::new(stream, "t1", r#"{"x":1}"#);
            e.idempotency_key = "key-1".to_string();
            let e2 = engine.append_event(e).unwrap();
            assert_eq!(
                e2.sequence, 1,
                "idempotency must be preserved across reopen"
            );
            // New event should continue
            let e3 = engine
                .append_event(MemoryEvent::new(stream, "t2", "{}"))
                .unwrap();
            assert_eq!(e3.sequence, 2);
        }
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            engine
                .put_object(
                    MemoryObject::new("docs", "a", r#"{"text":"persist"}"#)
                        .with_vector(vec![1.0, 0.0, 0.0]),
                )
                .unwrap();
        }
        {
            let engine = PersistentEngine::open(dir.path()).unwrap();
            let results = engine
                .vector_search("docs", &[1.0, 0.0, 0.0], VectorSearchOptions::default())
                .unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].id, "a");
        }
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_removed_on_update_without_vector_reopen() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            engine
                .put_object(
                    MemoryObject::new("docs", "a", r#"{"v":1}"#).with_vector(vec![1.0, 0.0, 0.0]),
                )
                .unwrap();
        }
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            // Update without vector — old vector must be removed
            engine
                .put_object(MemoryObject::new("docs", "a", r#"{"v":2}"#))
                .unwrap();
        }
        {
            let engine = PersistentEngine::open(dir.path()).unwrap();
            let results = engine
                .vector_search("docs", &[1.0, 0.0, 0.0], VectorSearchOptions::default())
                .unwrap();
            assert_eq!(results.len(), 0, "vector must survive reopen and removal");
        }
    }

    // ── Shared contract tests ───────────────────────────────────────────────

    fn setup_persistent() -> (PersistentEngine, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let engine = PersistentEngine::open(dir.path()).unwrap();
        (engine, dir)
    }

    fn setup_thingdb() -> (PersistentEngine, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let engine = PersistentEngine::open_with_options(
            dir.path(),
            PersistentOpenOptions {
                backend: PersistentBackend::ThingDb,
                ..PersistentOpenOptions::default()
            },
        )
        .unwrap();
        (engine, dir)
    }

    fn setup_thingdb_memory() -> PersistentEngine {
        PersistentEngine::open_in_memory_with_backend(PersistentBackend::ThingDb).unwrap()
    }

    #[test]
    fn thingdb_backend_runs_shared_contracts() {
        let (mut engine, _dir) = setup_thingdb();
        crate::contract_tests::test_contract_object_lifecycle(&mut engine);
        crate::contract_tests::test_contract_object_batches_and_ordering(&mut engine);
        crate::contract_tests::test_contract_link_consistency(&mut engine);
        crate::contract_tests::test_contract_vector_lifecycle(&mut engine);
        crate::contract_tests::test_contract_schema_store(&mut engine);
        crate::contract_tests::test_contract_indexes(&mut engine);
        crate::contract_tests::test_contract_event_idempotency(&mut engine);
        crate::contract_tests::test_contract_queue_lifecycle(&mut engine);
        crate::contract_tests::test_contract_delayed_job(&mut engine);
        crate::contract_tests::test_contract_lease_expiration(&mut engine);
        crate::contract_tests::test_contract_nack_dead_letter(&mut engine);
        crate::contract_tests::test_contract_search(&mut engine);
    }

    #[test]
    fn contract_object_lifecycle() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_object_lifecycle(&mut engine);
        crate::contract_tests::test_contract_object_batches_and_ordering(&mut engine);
        crate::contract_tests::test_contract_link_consistency(&mut engine);
    }

    #[test]
    fn contract_vector_lifecycle() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_vector_lifecycle(&mut engine);
    }

    #[test]
    fn contract_schema_store() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_schema_store(&mut engine);
        crate::contract_tests::test_contract_indexes(&mut engine);
    }

    #[test]
    fn contract_event_idempotency() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_event_idempotency(&mut engine);
    }

    #[test]
    fn contract_queue_lifecycle() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_queue_lifecycle(&mut engine);
    }

    #[test]
    fn contract_delayed_job() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_delayed_job(&mut engine);
    }

    #[test]
    fn contract_lease_expiration() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_lease_expiration(&mut engine);
    }

    #[test]
    fn contract_nack_dead_letter() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_nack_dead_letter(&mut engine);
    }

    #[test]
    fn thingdb_memory_runs_shared_contracts_without_filesystem_state() {
        let mut engine = setup_thingdb_memory();
        assert!(engine.is_in_memory());
        assert_eq!(engine.journal_bytes(), 0);
        assert_eq!(engine.journal_count(), 0);

        crate::contract_tests::test_contract_object_lifecycle(&mut engine);
        crate::contract_tests::test_contract_object_batches_and_ordering(&mut engine);
        crate::contract_tests::test_contract_link_consistency(&mut engine);
        crate::contract_tests::test_contract_vector_lifecycle(&mut engine);
        crate::contract_tests::test_contract_schema_store(&mut engine);
        crate::contract_tests::test_contract_indexes(&mut engine);
        crate::contract_tests::test_contract_event_idempotency(&mut engine);
        crate::contract_tests::test_contract_queue_lifecycle(&mut engine);
        crate::contract_tests::test_contract_delayed_job(&mut engine);
        crate::contract_tests::test_contract_lease_expiration(&mut engine);
        crate::contract_tests::test_contract_nack_dead_letter(&mut engine);
        crate::contract_tests::test_contract_search(&mut engine);

        assert!(engine.compact_storage().is_err());
        assert_eq!(engine.journal_bytes(), 0);
        assert_eq!(engine.journal_count(), 0);
        drop(engine);
    }

    #[cfg(feature = "search")]
    #[test]
    fn thingdb_memory_search_rebuild_is_filesystem_isolated() {
        static CURRENT_DIR_LOCK: OnceLock<TestMutex<()>> = OnceLock::new();
        let _lock = CURRENT_DIR_LOCK
            .get_or_init(|| TestMutex::new(()))
            .lock()
            .unwrap();
        let sandbox = tempfile::tempdir().unwrap();
        let before = std::fs::read_dir(sandbox.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(sandbox.path()).unwrap();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut engine = setup_thingdb_memory();
            assert!(engine.is_in_memory());
            assert!(!engine.search_rebuild_required());
            assert_eq!(engine.journal_bytes(), 0);
            assert_eq!(engine.journal_count(), 0);

            engine
                .put_object(MemoryObject::new(
                    "notes",
                    "one",
                    r#"{"text":"alpha only"}"#,
                ))
                .unwrap();
            assert_eq!(
                engine
                    .search("alpha", SearchOptions::default())
                    .unwrap()
                    .len(),
                1
            );

            engine
                .put_object(MemoryObject::new("notes", "one", r#"{"text":"beta only"}"#))
                .unwrap();
            assert!(
                engine
                    .search("alpha", SearchOptions::default())
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                engine
                    .search("beta", SearchOptions::default())
                    .unwrap()
                    .len(),
                1
            );

            engine
                .append_event(MemoryEvent::new(
                    "audit",
                    "indexed",
                    r#"{"text":"event-term"}"#,
                ))
                .unwrap();
            assert_eq!(
                engine
                    .search("event-term", SearchOptions::default())
                    .unwrap()
                    .len(),
                1
            );
            engine
                .create_link(Link::new("notes/one", "mentions", "notes/two"))
                .unwrap();
            engine.delete_object("notes", "one").unwrap();
            assert!(
                engine
                    .search("beta", SearchOptions::default())
                    .unwrap()
                    .is_empty()
            );

            // Exercise the defensive rebuild path. It must create a RAM index
            // and must not write a rebuild generation or marker.
            engine.search_index = None;
            engine.search_rebuild_required = true;
            engine.search_rebuild = None;
            let mut complete = false;
            for _ in 0..64 {
                complete = engine.search_rebuild_step(1).unwrap();
                if complete {
                    break;
                }
            }
            assert!(complete, "in-memory search rebuild did not complete");
            assert!(!engine.search_rebuild_required());
            assert_eq!(
                engine
                    .search("event-term", SearchOptions::default())
                    .unwrap()
                    .len(),
                1
            );
            drop(engine);
        }));

        std::env::set_current_dir(original_dir).unwrap();
        result.unwrap();

        let after = std::fs::read_dir(sandbox.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(before, after, "ThingDB RAM search created filesystem state");
    }

    #[test]
    fn differential_matrix_matches_memory_and_durable_backends() {
        let memory =
            crate::contract_tests::run_differential_scenario(&mut crate::MemoryEngine::new())
                .unwrap();
        let thingdb_memory =
            crate::contract_tests::run_differential_scenario(&mut setup_thingdb_memory()).unwrap();

        let rocksdb_dir = tempfile::tempdir().unwrap();
        let mut rocksdb = PersistentEngine::open(rocksdb_dir.path()).unwrap();
        let rocksdb_digest =
            crate::contract_tests::run_differential_scenario(&mut rocksdb).unwrap();

        let thingdb_dir = tempfile::tempdir().unwrap();
        let mut thingdb = PersistentEngine::open_with_options(
            thingdb_dir.path(),
            PersistentOpenOptions {
                backend: PersistentBackend::ThingDb,
                ..PersistentOpenOptions::default()
            },
        )
        .unwrap();
        let thingdb_digest =
            crate::contract_tests::run_differential_scenario(&mut thingdb).unwrap();

        assert_eq!(
            memory, thingdb_memory,
            "MemoryEngine vs ThingDB RAM mismatch"
        );
        assert_eq!(memory, rocksdb_digest, "MemoryEngine vs RocksDB mismatch");
        assert_eq!(
            memory, thingdb_digest,
            "MemoryEngine vs durable ThingDB mismatch"
        );
    }

    #[test]
    fn contract_search() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_search(&mut engine);
    }
}
