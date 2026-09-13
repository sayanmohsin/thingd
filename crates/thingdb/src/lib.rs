//! Experimental Rust-native durable ordered key-value storage for Thingd.
//!
//! The initial format is intentionally small and conservative: a checksummed
//! write-ahead log provides crash recovery, while immutable sorted table
//! snapshots bound WAL growth. The API is shaped around Thingd's keyspace,
//! batch, prefix, and range needs so it can later be extracted as a standalone
//! database without coupling the file format to the public Thingd API.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::doc_markdown,
    clippy::iter_without_into_iter,
    clippy::map_unwrap_or,
    clippy::missing_const_for_fn,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::return_self_not_must_use,
    clippy::significant_drop_tightening,
    clippy::type_complexity
)]

mod cache;

pub use cache::{CacheOptions, CacheStats, MemoryCache};

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::{Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Seek, SeekFrom, Write};
use std::ops::{Bound, RangeBounds};
use std::path::{Component, Path, PathBuf};
use std::sync::{
    Arc, Mutex, OnceLock, RwLock, Weak,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, RecvTimeoutError, SyncSender, TryRecvError},
};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::fs::FileExt;

#[cfg(feature = "benchmark")]
use arc_swap::ArcSwap;
use crc32fast::Hasher;
use serde::{Deserialize, Serialize};

const WAL_MAGIC: &[u8; 8] = b"TDBWAL01";
const TABLE_MAGIC: &[u8; 8] = b"TDBTAB01";
const TABLE_MAGIC_V2: &[u8; 8] = b"TDBTAB02";
const TABLE_MAGIC_V3: &[u8; 8] = b"TDBTAB03";
const TABLE_FOOTER_MAGIC: &[u8; 8] = b"TDBFTR01";
const FORMAT_VERSION: u32 = 1;
const WAL_FILE: &str = "WAL";
const MANIFEST_FILE: &str = "MANIFEST.json";
const MANIFEST_TEMP_FILE: &str = ".MANIFEST.json.tmp";
const LOCK_FILE: &str = "LOCK";

#[cfg(feature = "benchmark")]
type RamGeneration = HashMap<Arc<str>, Arc<BTreeMap<Vec<u8>, Arc<Vec<u8>>>>>;

#[derive(Clone, Copy)]
enum WalSyncMode {
    Data,
    #[cfg(feature = "benchmark")]
    All,
}

fn sync_wal(file: &File, mode: WalSyncMode) -> io::Result<()> {
    match mode {
        WalSyncMode::Data => file.sync_data(),
        #[cfg(feature = "benchmark")]
        WalSyncMode::All => file.sync_all(),
    }
}

#[cfg(test)]
thread_local! {
    static FAULT_POINT: std::cell::RefCell<Option<&'static str>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn set_fault_point(point: Option<&'static str>) {
    FAULT_POINT.with(|fault| *fault.borrow_mut() = point);
}

#[cfg(test)]
fn current_fault_point() -> Option<&'static str> {
    FAULT_POINT.with(|fault| *fault.borrow())
}

#[cfg(not(test))]
fn current_fault_point() -> Option<&'static str> {
    None
}

fn maybe_fail(point: &'static str, fault_point: Option<&'static str>) -> Result<()> {
    if fault_point == Some(point) {
        Err(Error::message(format!("injected ThingDB fault: {point}")))
    } else {
        Ok(())
    }
}

fn validate_table_name(name: &str) -> Result<()> {
    let path = Path::new(name);
    if name.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(Error::message("invalid ThingDB table filename"));
    }
    Ok(())
}

fn validate_manifest(manifest: &Manifest) -> Result<()> {
    let mut files = BTreeSet::new();
    for table_file in &manifest.table_files {
        validate_table_name(table_file)?;
        if !files.insert(table_file) {
            return Err(Error::message("duplicate ThingDB table filename"));
        }
    }
    if let Some(table_file) = &manifest.table_file {
        validate_table_name(table_file)?;
        if !manifest.table_files.is_empty() && manifest.table_files.last() != Some(table_file) {
            return Err(Error::message(
                "ThingDB manifest legacy table filename does not match table layers",
            ));
        }
    }
    let table_files = if manifest.table_files.is_empty() {
        manifest.table_file.iter().collect::<Vec<_>>()
    } else {
        manifest.table_files.iter().collect::<Vec<_>>()
    };
    if table_files.is_empty() && manifest.table_sequence != 0 {
        return Err(Error::message(
            "ThingDB manifest has a table sequence without a table",
        ));
    }
    if table_files.windows(2).any(|files| files[0] >= files[1]) {
        return Err(Error::message(
            "ThingDB manifest table layers are not in filename order",
        ));
    }
    Ok(())
}

fn cleanup_temporary_files(path: &Path) -> Result<()> {
    let entries = fs::read_dir(path)?;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == MANIFEST_TEMP_FILE || (name.starts_with(".table-") && name.ends_with(".tdb.tmp"))
        {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

/// Result type returned by ThingDB.
pub type Result<T> = std::result::Result<T, Error>;

/// ThingDB error.
#[derive(Debug)]
pub struct Error(String);

impl Error {
    fn message(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl Display for Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

/// Durability mode for a database flush.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistMode {
    /// Flush the WAL and current table state to durable storage.
    SyncAll,
}

/// Bounded WAL and recovery diagnostics for local measurements.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct WalDiagnostics {
    /// Bytes currently present in the WAL.
    pub journal_bytes: u64,
    /// Complete frames currently present in the WAL.
    pub frame_count: u64,
    /// Bytes inspected while replaying the WAL during the last open.
    pub recovery_bytes: u64,
    /// Nanoseconds spent replaying the WAL during the last open.
    pub recovery_duration_ns: u64,
    /// Nanoseconds spent encoding WAL frames.
    pub encode_duration_ns: u64,
    /// Nanoseconds spent reserving the grouped WAL buffer.
    pub buffer_allocation_duration_ns: u64,
    /// Bytes reserved for grouped WAL buffers.
    pub buffer_reserved_bytes: u64,
    /// Nanoseconds spent encoding WAL frames outside the state lock.
    pub encode_outside_lock_duration_ns: u64,
    /// Nanoseconds spent appending WAL frames.
    pub append_duration_ns: u64,
    /// Total bytes appended to the WAL since opening the database.
    pub wal_bytes_appended: u64,
    /// Nanoseconds spent syncing WAL frames.
    pub sync_duration_ns: u64,
    /// Nanoseconds spent waiting for the WAL I/O lock.
    pub wal_lock_wait_duration_ns: u64,
    /// Nanoseconds spent holding the WAL I/O lock.
    pub wal_lock_held_duration_ns: u64,
    /// Nanoseconds spent applying committed operations to memory.
    pub state_apply_duration_ns: u64,
    /// Nanoseconds spent holding the database lock for commits.
    pub lock_duration_ns: u64,
    /// Number of logical commit requests processed.
    pub logical_commit_count: u64,
    /// Number of physical WAL sync calls.
    pub physical_sync_count: u64,
    /// Total number of requests included in commit groups.
    pub total_group_size: u64,
    /// Largest commit group observed.
    pub max_group_size: u64,
    /// Nanoseconds spent waiting in the commit queue.
    pub queue_wait_duration_ns: u64,
    /// Whether the database requires reopen and recovery before writing.
    pub recovery_required: bool,
    /// Number of commit groups rejected after a state-generation change.
    pub state_generation_conflict_count: u64,
    /// Number of WAL truncation failures observed during recovery of a group.
    pub truncation_failure_count: u64,
    /// Whether the WAL is above the configured soft budget.
    pub wal_over_budget: bool,
    /// Bytes held by the current mutable table delta.
    pub memtable_bytes: u64,
    /// Number of table flushes completed.
    pub flush_count: u64,
    /// Number of automatic bound-triggered flushes completed.
    pub automatic_flush_count: u64,
    /// Nanoseconds spent flushing table deltas.
    pub flush_duration_ns: u64,
    /// Whether the mutable table delta is above its configured bound.
    pub memtable_over_budget: bool,
    /// Last WAL error observed after the database opened, if any.
    pub last_error: Option<String>,
    /// Number of durable point lookups that consulted table layers.
    pub table_lookup_count: u64,
    /// Number of point lookups checking the mutable state.
    pub mutable_state_lookup_count: u64,
    /// Number of point lookups checking the pending table.
    pub pending_table_lookup_count: u64,
    /// Number of point lookups checking immutable table layers.
    pub immutable_layer_lookup_count: u64,
    /// Number of table layers inspected by point lookups.
    pub table_layers_consulted: u64,
    /// Bytes read from table records.
    pub table_bytes_read: u64,
    /// Nanoseconds spent reading table records.
    pub table_read_duration_ns: u64,
    /// Nanoseconds spent waiting for the database read lock.
    pub read_lock_wait_duration_ns: u64,
    /// Nanoseconds spent holding the database read lock.
    pub read_lock_held_duration_ns: u64,
    /// Nanoseconds spent waiting for an immutable table reader.
    pub table_reader_wait_duration_ns: u64,
    /// Nanoseconds spent selecting a table record.
    pub table_lookup_duration_ns: u64,
    /// Nanoseconds spent reading a table record after selection.
    pub file_read_duration_ns: u64,
    /// Nanoseconds spent opening table files during database open.
    pub table_open_duration_ns: u64,
    /// Nanoseconds spent materializing or merging durable scans.
    pub scan_duration_ns: u64,
    /// Number of durable scans completed.
    pub scan_count: u64,
    /// Number of physical keys examined by durable scans.
    pub scan_keys_examined: u64,
    /// Number of table layers consulted while merging durable scans.
    pub scan_layers_consulted: u64,
    /// Nanoseconds spent waiting for the database lock before a scan.
    pub scan_lock_wait_duration_ns: u64,
    /// Nanoseconds spent preparing a scan while holding the database lock.
    pub scan_lock_held_duration_ns: u64,
    /// Nanoseconds spent initializing scan cursors.
    pub scan_cursor_init_duration_ns: u64,
    /// Nanoseconds spent merging scan cursors and reading values.
    pub scan_merge_duration_ns: u64,
    /// Number of entries returned by durable scans.
    pub scan_returned_entries: u64,
    /// Number of durable scans that encountered an error.
    pub scan_error_count: u64,
    /// Number of table layers currently open for reads.
    pub table_layer_count: u64,
    /// Number of completed table compactions.
    pub compaction_count: u64,
    /// Nanoseconds spent compacting immutable table layers.
    pub compaction_duration_ns: u64,
    /// Bytes represented by the last compacted table layers.
    pub compaction_input_bytes: u64,
    /// Bytes written by completed compacted tables.
    pub compaction_output_bytes: u64,
    /// Total bytes written to immutable table files since opening the database.
    pub table_bytes_written: u64,
}

/// Timing and allocation-adjacent counters for the RAM-only keyspace path.
///
/// These values are intentionally diagnostic rather than a performance
/// contract. Durable databases return zeroes because they use the existing
/// WAL/table path instead of this process-local layout.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct RamDiagnostics {
    /// Number of point lookups.
    pub lookup_count: u64,
    /// Nanoseconds spent constructing lookup keys.
    pub key_encode_duration_ns: u64,
    /// Nanoseconds spent waiting for the RAM state lock.
    pub lock_wait_duration_ns: u64,
    /// Nanoseconds spent holding the RAM state lock.
    pub lock_held_duration_ns: u64,
    /// Nanoseconds spent locating values in the ordered map.
    pub lookup_duration_ns: u64,
    /// Nanoseconds spent cloning returned values.
    pub value_clone_duration_ns: u64,
    /// Nanoseconds spent acquiring shared ownership of returned values.
    pub value_acquire_duration_ns: u64,
    /// Number of RAM mutations.
    pub mutation_count: u64,
    /// Nanoseconds spent applying RAM mutations.
    pub mutation_duration_ns: u64,
    /// Number of RAM iteration requests.
    pub iteration_count: u64,
    /// Nanoseconds spent materializing RAM iteration results.
    pub iteration_duration_ns: u64,
    /// Number of RAM entries inspected by bounded iterations.
    pub iteration_entries_examined: u64,
    /// Number of entries returned by RAM iterations.
    pub iteration_entries_returned: u64,
    /// Nanoseconds spent deserializing Thingd objects from RAM values.
    pub deserialization_duration_ns: u64,
    /// Number of RAM-side serialized values recorded by the semantic layer.
    pub serialization_count: u64,
    /// Nanoseconds spent serializing values for the RAM semantic layer.
    pub serialization_duration_ns: u64,
    /// Number of RAM search operations.
    pub search_count: u64,
    /// Nanoseconds spent executing RAM searches.
    pub search_duration_ns: u64,
    /// Number of RAM derived-index mutations recorded by the semantic layer.
    pub search_index_count: u64,
    /// Nanoseconds spent applying RAM derived-index mutations.
    pub search_index_duration_ns: u64,
    /// Number of ThingDB RAM snapshots created.
    pub snapshot_count: u64,
    /// Nanoseconds spent creating ThingDB RAM snapshots.
    pub snapshot_duration_ns: u64,
}

/// Lock-free counters for diagnostics that are updated on every RAM lookup,
/// mutation, or semantic serialization. Keeping these counters separate from
/// the richer snapshot state avoids adding a diagnostics mutex to the hot
/// storage path.
#[derive(Default)]
struct RamHotDiagnostics {
    lookup_count: AtomicU64,
    lock_wait_duration_ns: AtomicU64,
    lock_held_duration_ns: AtomicU64,
    lookup_duration_ns: AtomicU64,
    value_clone_duration_ns: AtomicU64,
    value_acquire_duration_ns: AtomicU64,
    mutation_count: AtomicU64,
    mutation_duration_ns: AtomicU64,
    deserialization_duration_ns: AtomicU64,
    serialization_count: AtomicU64,
    serialization_duration_ns: AtomicU64,
}

impl RamHotDiagnostics {
    fn add(counter: &AtomicU64, value: u64) {
        counter.fetch_add(value, Ordering::Relaxed);
    }

    fn snapshot_into(&self, diagnostics: &mut RamDiagnostics) {
        diagnostics.lookup_count = self.lookup_count.load(Ordering::Relaxed);
        diagnostics.lock_wait_duration_ns = self.lock_wait_duration_ns.load(Ordering::Relaxed);
        diagnostics.lock_held_duration_ns = self.lock_held_duration_ns.load(Ordering::Relaxed);
        diagnostics.lookup_duration_ns = self.lookup_duration_ns.load(Ordering::Relaxed);
        diagnostics.value_clone_duration_ns = self.value_clone_duration_ns.load(Ordering::Relaxed);
        diagnostics.value_acquire_duration_ns =
            self.value_acquire_duration_ns.load(Ordering::Relaxed);
        diagnostics.mutation_count = self.mutation_count.load(Ordering::Relaxed);
        diagnostics.mutation_duration_ns = self.mutation_duration_ns.load(Ordering::Relaxed);
        diagnostics.deserialization_duration_ns =
            self.deserialization_duration_ns.load(Ordering::Relaxed);
        diagnostics.serialization_count = self.serialization_count.load(Ordering::Relaxed);
        diagnostics.serialization_duration_ns =
            self.serialization_duration_ns.load(Ordering::Relaxed);
    }
}

/// Keyspace creation options reserved for future per-keyspace tuning.
#[derive(Clone, Copy, Debug, Default)]
pub struct KeyspaceCreateOptions;

impl KeyspaceCreateOptions {
    /// Compatibility constant matching the existing Thingd adapter API.
    #[allow(non_upper_case_globals)]
    pub const default: Self = Self;
}

/// Builder for opening a ThingDB database.
pub struct DatabaseBuilder {
    path: PathBuf,
    max_journaling_size: u64,
    max_memtable_bytes: u64,
    max_table_layers: usize,
    wal_sync_mode: WalSyncMode,
}

/// A shared ThingDB database handle.
#[derive(Clone)]
pub struct Database {
    inner: Arc<Mutex<Inner>>,
    ram_keyspaces: Option<Arc<RwLock<HashMap<Arc<str>, BTreeMap<Vec<u8>, Arc<Vec<u8>>>>>>>,
    #[cfg(feature = "benchmark")]
    ram_rcu_keyspaces: Option<Arc<ArcSwap<RamGeneration>>>,
    ram_diagnostics: Arc<Mutex<RamDiagnostics>>,
    ram_hot_diagnostics: Arc<RamHotDiagnostics>,
    ram_logical_commit_count: Arc<AtomicU64>,
    ram_total_group_size: Arc<AtomicU64>,
    ram_state_apply_duration_ns: Arc<AtomicU64>,
    writer: Arc<WriterCoordinator>,
    commit_gate: Arc<Mutex<()>>,
}

struct Inner {
    path: PathBuf,
    // Share the opened handle with commit groups. Cloning the Arc avoids a
    // file-descriptor duplication syscall for every durable commit while the
    // WAL mutex still serializes append, sync, and truncation ordering.
    wal: Option<Arc<File>>,
    lock: Option<File>,
    in_memory: bool,
    state: BTreeMap<Vec<u8>, Vec<u8>>,
    sequence: u64,
    table_sequence: u64,
    table_files: Vec<String>,
    table_layers: Vec<TableLayer>,
    pending_table: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    pending_table_bytes: u64,
    max_journaling_size: u64,
    max_memtable_bytes: u64,
    max_table_layers: usize,
    diagnostics: WalDiagnostics,
    recovery_required: bool,
    commit_gate: Arc<Mutex<()>>,
    wal_io_lock: Arc<Mutex<()>>,
    state_generation: u64,
    wal_sync_mode: WalSyncMode,
    flush_scheduled: Arc<AtomicBool>,
}

#[derive(Clone)]
struct TableLayer {
    file: Arc<File>,
    path: PathBuf,
    entries: Arc<OnceLock<Arc<Vec<TableIndexEntry>>>>,
    blocks: Arc<Vec<TableBlock>>,
    is_v2: bool,
}

#[derive(Clone)]
struct TableIndexEntry {
    key: Vec<u8>,
    offset: u64,
    length: u64,
}

#[derive(Clone, Debug)]
struct TableBlock {
    offset: u64,
    length: u64,
    first_key: Vec<u8>,
    last_key: Vec<u8>,
    checksum: u32,
}

impl TableLayer {
    fn loaded_entries(&self) -> Result<Arc<Vec<TableIndexEntry>>> {
        if self.entries.get().is_none() {
            let (_, entries, _, _) = read_table_index(&self.path)?;
            let _ = self.entries.set(Arc::new(entries));
        }
        Ok(Arc::clone(self.entries.get().ok_or_else(|| {
            Error::message("ThingDB table index was not initialized")
        })?))
    }

    fn candidate_entry_range(&self, key: &[u8]) -> std::ops::Range<usize> {
        if self.blocks.is_empty() {
            let entries = self.entries.get().map_or(0, |entries| entries.len());
            return 0..entries;
        }
        let block_index = self
            .blocks
            .partition_point(|block| block.last_key.as_slice() < key);
        let Some(block) = self.blocks.get(block_index) else {
            return 0..0;
        };
        if block.first_key.as_slice() > key {
            return 0..0;
        }
        let end = block.offset.saturating_add(block.length);
        let Some(entries) = self.entries.get() else {
            return 0..0;
        };
        let start_index = entries.partition_point(|entry| entry.offset < block.offset);
        let end_index = entries.partition_point(|entry| entry.offset < end);
        start_index..end_index
    }

    fn candidate_block(&self, key: &[u8]) -> Option<&TableBlock> {
        if self.blocks.is_empty() {
            return None;
        }
        let index = self
            .blocks
            .partition_point(|block| block.last_key.as_slice() < key);
        self.blocks
            .get(index)
            .filter(|block| block.first_key.as_slice() <= key && key <= block.last_key.as_slice())
    }
}

impl Inner {
    fn materialize_state(&mut self) -> Result<BTreeMap<Vec<u8>, Vec<u8>>> {
        let mut state = BTreeMap::new();
        for layer in &mut self.table_layers {
            let entries = layer.loaded_entries()?;
            for entry in entries.iter() {
                match read_table_value_handle(&layer.file, entry, layer.is_v2)?.0 {
                    Some(value) => {
                        state.insert(entry.key.clone(), value);
                    },
                    None => {
                        state.remove(&entry.key);
                    },
                }
            }
        }
        for (key, value) in &self.state {
            state.insert(key.clone(), value.clone());
        }
        for (key, value) in &self.pending_table {
            match value {
                Some(value) => {
                    state.insert(key.clone(), value.clone());
                },
                None => {
                    state.remove(key);
                },
            }
        }
        Ok(state)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    format_version: u32,
    table_file: Option<String>,
    #[serde(default)]
    table_files: Vec<String>,
    table_sequence: u64,
}

/// A named ordered keyspace in a database.
#[derive(Clone)]
pub struct Keyspace {
    db: Database,
    name: Arc<str>,
    namespace: Arc<[u8]>,
}

/// A write batch applied atomically to all included keyspaces.
pub struct Batch {
    db: Database,
    memory_mode: bool,
    operations: Vec<BatchOperation>,
}

struct BatchOperation {
    keyspace: Arc<str>,
    key: Vec<u8>,
    value: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
enum Operation {
    Put { key: Vec<u8>, value: Vec<u8> },
    Delete { key: Vec<u8> },
}

struct CommitRequest {
    operations: Vec<Operation>,
    // Computed before enqueueing so the coordinator does not walk every
    // operation once for admission and again when reserving the WAL buffer.
    encoded_bytes: usize,
    submitted_at: Instant,
    fault_point: Option<&'static str>,
    response: mpsc::Sender<Result<()>>,
}

struct WriterCoordinator {
    sender: Mutex<Option<SyncSender<CommitRequest>>>,
    handle: Mutex<Option<thread::JoinHandle<()>>>,
}

const WRITER_QUEUE_CAPACITY: usize = 1024;
// A short window lets already-queued writers coalesce without adding a
// millisecond of latency to every burst. Isolated writes still bypass it.
const GROUP_COMMIT_WINDOW: Duration = Duration::from_micros(250);
const MAX_GROUP_OPERATIONS: usize = 4_096;
const MAX_GROUP_BYTES: usize = 4 * 1024 * 1024;

impl WriterCoordinator {
    fn new(inner: Weak<Mutex<Inner>>) -> Result<Arc<Self>> {
        let (sender, receiver) = mpsc::sync_channel(WRITER_QUEUE_CAPACITY);
        let handle = thread::Builder::new()
            .name("thingdb-writer".to_string())
            .spawn(move || writer_loop(inner, receiver))
            .map_err(Error::from)?;
        Ok(Arc::new(Self {
            sender: Mutex::new(Some(sender)),
            handle: Mutex::new(Some(handle)),
        }))
    }
}

impl Drop for WriterCoordinator {
    fn drop(&mut self) {
        // Close the channel before joining so a writer waiting in recv() can
        // observe disconnection and exit. Any queued requests are completed or
        // rejected by the writer before it terminates.
        let _ = self.sender.get_mut().ok().and_then(Option::take);
        if let Some(handle) = self.handle.get_mut().ok().and_then(Option::take) {
            let _ = handle.join();
        }
    }
}

fn writer_loop(inner: Weak<Mutex<Inner>>, receiver: mpsc::Receiver<CommitRequest>) {
    let mut pending = None;
    // Groups are processed serially by this dedicated thread. Reusing the
    // encoded WAL buffer avoids allocating and freeing a large Vec for every
    // group while keeping its lifetime bounded by the writer thread.
    let mut wal_buffer = Vec::new();
    loop {
        let Some(first) = pending.take().or_else(|| receiver.recv().ok()) else {
            return;
        };
        let mut group = vec![first];
        let mut operation_count = group[0].operations.len();
        let mut operation_bytes = group[0].encoded_bytes;
        // An isolated durable write should not pay the group-commit window.
        // Once another request is already queued, retain the bounded window
        // so concurrent writers can still share one WAL sync.
        let deadline = match receiver.try_recv() {
            Ok(request) => {
                let request_operations = request.operations.len();
                let request_bytes = request.encoded_bytes;
                if operation_count + request_operations <= MAX_GROUP_OPERATIONS
                    && operation_bytes + request_bytes <= MAX_GROUP_BYTES
                {
                    operation_count += request_operations;
                    operation_bytes += request_bytes;
                    group.push(request);
                    Some(Instant::now() + GROUP_COMMIT_WINDOW)
                } else {
                    pending = Some(request);
                    None
                }
            },
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
        };

        if let Some(deadline) = deadline {
            loop {
                if operation_count >= MAX_GROUP_OPERATIONS || operation_bytes >= MAX_GROUP_BYTES {
                    break;
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                match receiver.recv_timeout(remaining) {
                    Ok(request) => {
                        let request_operations = request.operations.len();
                        let request_bytes = request.encoded_bytes;
                        if operation_count + request_operations > MAX_GROUP_OPERATIONS
                            || operation_bytes + request_bytes > MAX_GROUP_BYTES
                        {
                            pending = Some(request);
                            break;
                        }
                        operation_count += request_operations;
                        operation_bytes += request_bytes;
                        group.push(request);
                    },
                    Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
                }
            }
        }

        process_group(&inner, group, &mut wal_buffer);
    }
}

fn schedule_background_flush(inner_arc: &Arc<Mutex<Inner>>, scheduled: Arc<AtomicBool>) -> bool {
    if scheduled.swap(true, Ordering::AcqRel) {
        return true;
    }
    let weak_inner = Arc::downgrade(inner_arc);
    let worker_scheduled = Arc::clone(&scheduled);
    if thread::Builder::new()
        .name("thingdb-flush".to_string())
        .spawn(move || background_flush_worker(weak_inner, worker_scheduled))
        .is_err()
    {
        scheduled.store(false, Ordering::Release);
        return false;
    }
    true
}

fn background_flush_worker(inner_weak: Weak<Mutex<Inner>>, scheduled: Arc<AtomicBool>) {
    loop {
        let Some(inner_arc) = inner_weak.upgrade() else {
            break;
        };
        let gate = match inner_arc.lock() {
            Ok(inner) => Arc::clone(&inner.commit_gate),
            Err(_) => break,
        };
        let Ok(_gate) = gate.lock() else {
            break;
        };
        let Ok(mut inner) = inner_arc.lock() else {
            break;
        };
        if inner.recovery_required || inner.pending_table.is_empty() {
            break;
        }
        if let Err(error) = flush_table_locked(&mut inner, true, None) {
            inner.recovery_required = true;
            inner.diagnostics.recovery_required = true;
            inner.diagnostics.last_error = Some(error.to_string());
            break;
        }
        inner.diagnostics.automatic_flush_count =
            inner.diagnostics.automatic_flush_count.saturating_add(1);
        if inner.table_layers.len() >= inner.max_table_layers
            && let Err(error) = compact_tables_locked(&mut inner, true, None)
        {
            inner.recovery_required = true;
            inner.diagnostics.recovery_required = true;
            inner.diagnostics.last_error = Some(error.to_string());
            break;
        }
        if inner.pending_table_bytes < inner.max_memtable_bytes {
            break;
        }
    }

    scheduled.store(false, Ordering::Release);
    let reschedule = inner_weak
        .upgrade()
        .and_then(|inner_arc| {
            inner_arc.lock().ok().map(|inner| {
                !inner.recovery_required && inner.pending_table_bytes >= inner.max_memtable_bytes
            })
        })
        .unwrap_or(false);
    if reschedule
        && let Some(inner_arc) = inner_weak.upgrade()
        && !schedule_background_flush(&inner_arc, Arc::clone(&scheduled))
        && let Ok(mut inner) = inner_arc.lock()
    {
        inner.recovery_required = true;
        inner.diagnostics.recovery_required = true;
        inner.diagnostics.last_error =
            Some("ThingDB could not restart its background flush worker".to_string());
    }
}

fn request_has_fault(requests: &[CommitRequest], point: &'static str) -> Option<&'static str> {
    requests
        .iter()
        .find_map(|request| (request.fault_point == Some(point)).then_some(point))
}

#[allow(clippy::too_many_lines)]
fn process_group(
    inner: &Weak<Mutex<Inner>>,
    mut requests: Vec<CommitRequest>,
    wal_buffer: &mut Vec<u8>,
) {
    let Some(inner_arc) = inner.upgrade() else {
        for request in &mut *requests {
            let _ = request
                .response
                .send(Err(Error::message("ThingDB writer is unavailable")));
        }
        return;
    };
    let gate = if let Ok(inner) = inner_arc.lock() {
        Arc::clone(&inner.commit_gate)
    } else {
        for request in requests {
            let _ = request
                .response
                .send(Err(Error::message("database lock poisoned")));
        }
        return;
    };
    let Ok(_gate) = gate.lock() else {
        finish_group_with_error(&mut requests, "ThingDB commit gate poisoned");
        return;
    };
    let Ok(mut inner) = inner_arc.lock() else {
        drop(inner_arc);
        for request in requests {
            let _ = request
                .response
                .send(Err(Error::message("database lock poisoned")));
        }
        return;
    };
    let lock_held_started = Instant::now();

    let group_size = requests.len() as u64;
    inner.diagnostics.logical_commit_count = inner
        .diagnostics
        .logical_commit_count
        .saturating_add(group_size);
    inner.diagnostics.total_group_size = inner
        .diagnostics
        .total_group_size
        .saturating_add(group_size);
    inner.diagnostics.max_group_size = inner.diagnostics.max_group_size.max(group_size);
    for request in &requests {
        inner.diagnostics.queue_wait_duration_ns = inner
            .diagnostics
            .queue_wait_duration_ns
            .saturating_add(elapsed_nanos(request.submitted_at.elapsed()));
    }

    if inner.recovery_required {
        let message = "ThingDB requires reopen and recovery before writing";
        inner.diagnostics.last_error = Some(message.to_string());
        inner.diagnostics.lock_duration_ns = inner
            .diagnostics
            .lock_duration_ns
            .saturating_add(elapsed_nanos(lock_held_started.elapsed()));
        drop(inner);
        drop(inner_arc);
        finish_group_with_error(&mut requests, message);
        return;
    }

    // Let the background flusher catch up after acknowledged writes. If it
    // falls behind by two memtable bounds, apply bounded backpressure before
    // admitting another WAL group rather than allowing unbounded memory/WAL
    // growth.
    if !inner.in_memory
        && inner.max_memtable_bytes > 1
        && inner.pending_table_bytes >= inner.max_memtable_bytes.saturating_mul(2)
    {
        if let Err(error) = flush_table_locked(&mut inner, true, None) {
            let message = error.to_string();
            inner.recovery_required = true;
            inner.diagnostics.recovery_required = true;
            inner.diagnostics.last_error = Some(message.clone());
            inner.diagnostics.lock_duration_ns = inner
                .diagnostics
                .lock_duration_ns
                .saturating_add(elapsed_nanos(lock_held_started.elapsed()));
            drop(inner);
            drop(inner_arc);
            finish_group_with_error(&mut requests, &message);
            return;
        }
        inner.diagnostics.automatic_flush_count =
            inner.diagnostics.automatic_flush_count.saturating_add(1);
    }

    let wal_start = if inner.in_memory {
        0
    } else if inner.wal.is_some() {
        // The diagnostic is maintained whenever the WAL is opened, appended,
        // truncated, or rotated. Reusing it avoids another metadata syscall
        // while the database lock is held.
        inner.diagnostics.journal_bytes
    } else {
        let message = "ThingDB WAL is unavailable";
        inner.diagnostics.last_error = Some(message.to_string());
        inner.diagnostics.lock_duration_ns = inner
            .diagnostics
            .lock_duration_ns
            .saturating_add(elapsed_nanos(lock_held_started.elapsed()));
        drop(inner);
        drop(inner_arc);
        finish_group_with_error(&mut requests, message);
        return;
    };
    let (result, synced) = if inner.in_memory {
        let result = execute_memory_group(&mut inner, &mut requests, group_size);
        inner.diagnostics.lock_duration_ns = inner
            .diagnostics
            .lock_duration_ns
            .saturating_add(elapsed_nanos(lock_held_started.elapsed()));
        drop(inner);
        result
    } else {
        let expected_sequence = inner.sequence.saturating_add(1);
        let expected_generation = inner.state_generation;
        let wal_io_lock = Arc::clone(&inner.wal_io_lock);
        let wal_file = inner.wal.as_ref().map(Arc::clone);
        let wal_sync_mode = inner.wal_sync_mode;
        inner.diagnostics.lock_duration_ns = inner
            .diagnostics
            .lock_duration_ns
            .saturating_add(elapsed_nanos(lock_held_started.elapsed()));
        drop(inner);
        execute_group(
            &inner_arc,
            &mut requests,
            expected_sequence,
            expected_generation,
            wal_start,
            wal_file,
            wal_io_lock,
            wal_sync_mode,
            group_size,
            wal_buffer,
        )
    };

    let Ok(mut inner) = inner_arc.lock() else {
        finish_group_with_error(&mut requests, "database lock poisoned");
        return;
    };
    let post_lock_held_started = Instant::now();
    if let Err(error) = &result {
        inner.diagnostics.last_error = Some(error.to_string());
        if synced {
            inner.recovery_required = true;
            inner.diagnostics.recovery_required = true;
        }
    }
    inner.diagnostics.lock_duration_ns = inner
        .diagnostics
        .lock_duration_ns
        .saturating_add(elapsed_nanos(post_lock_held_started.elapsed()));
    drop(inner);

    match result {
        Ok(()) => {
            for request in requests {
                let _ = request.response.send(Ok(()));
            }
        },
        Err(error) => finish_group_with_error(&mut requests, &error.to_string()),
    }
}

fn execute_memory_group(
    inner: &mut Inner,
    requests: &mut [CommitRequest],
    group_size: u64,
) -> (Result<()>, bool) {
    let started = Instant::now();
    for request in requests.iter_mut() {
        for operation in std::mem::take(&mut request.operations) {
            apply_operation(&mut inner.state, operation);
        }
    }
    inner.sequence = inner.sequence.saturating_add(group_size);
    inner.diagnostics.state_apply_duration_ns = inner
        .diagnostics
        .state_apply_duration_ns
        .saturating_add(elapsed_nanos(started.elapsed()));
    inner.diagnostics.journal_bytes = 0;
    inner.diagnostics.frame_count = 0;
    inner.diagnostics.wal_over_budget = false;
    inner.diagnostics.memtable_bytes = 0;
    inner.diagnostics.memtable_over_budget = false;
    (Ok(()), false)
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn execute_group(
    inner_arc: &Arc<Mutex<Inner>>,
    requests: &mut [CommitRequest],
    expected_sequence: u64,
    expected_generation: u64,
    wal_start: u64,
    wal_file: Option<Arc<File>>,
    wal_io_lock: Arc<Mutex<()>>,
    wal_sync_mode: WalSyncMode,
    group_size: u64,
    wal_buffer: &mut Vec<u8>,
) -> (Result<()>, bool) {
    let allocation_started = Instant::now();
    let reserved_bytes = requests.iter().map(|request| request.encoded_bytes).sum();
    wal_buffer.clear();
    if wal_buffer.capacity() < reserved_bytes {
        wal_buffer.reserve(reserved_bytes - wal_buffer.capacity());
    }
    let allocation_duration_ns = elapsed_nanos(allocation_started.elapsed());
    let encode_started = Instant::now();
    let mut next_sequence = expected_sequence;
    let encode_result = (|| {
        for request in requests.iter() {
            encode_frame_into(wal_buffer, next_sequence, &request.operations)?;
            next_sequence = next_sequence.saturating_add(1);
        }
        Ok::<(), Error>(())
    })();
    let encode_duration_ns = elapsed_nanos(encode_started.elapsed());
    if let Ok(mut inner) = inner_arc.lock() {
        inner.diagnostics.buffer_allocation_duration_ns = inner
            .diagnostics
            .buffer_allocation_duration_ns
            .saturating_add(allocation_duration_ns);
        inner.diagnostics.buffer_reserved_bytes = inner
            .diagnostics
            .buffer_reserved_bytes
            .saturating_add(reserved_bytes as u64);
        inner.diagnostics.encode_duration_ns = inner
            .diagnostics
            .encode_duration_ns
            .saturating_add(encode_duration_ns);
        inner.diagnostics.encode_outside_lock_duration_ns = inner
            .diagnostics
            .encode_outside_lock_duration_ns
            .saturating_add(encode_duration_ns);
    }
    if let Err(error) = encode_result {
        return (Err(error), false);
    }
    let Some(mut wal_file) = wal_file else {
        return (Err(Error::message("ThingDB WAL is unavailable")), false);
    };
    let mut synced = false;
    let mut appended = false;
    let mut append_duration_ns = 0;
    let mut sync_duration_ns = 0;
    let mut wal_lock_wait_ns = 0;
    let result: Result<()> = (|| {
        if let Some(point) = request_has_fault(requests, "before-wal-append") {
            maybe_fail(point, Some(point))?;
        }
        {
            let Ok(mut inner) = inner_arc.lock() else {
                return Err(Error::message("database lock poisoned"));
            };
            if inner.recovery_required
                || inner.sequence.saturating_add(1) != expected_sequence
                || inner.state_generation != expected_generation
            {
                inner.diagnostics.state_generation_conflict_count = inner
                    .diagnostics
                    .state_generation_conflict_count
                    .saturating_add(1);
                return Err(Error::message(
                    "ThingDB commit state changed while preparing WAL frames",
                ));
            }
        }
        {
            let wal_lock_started = Instant::now();
            let Ok(_wal_lock) = wal_io_lock.lock() else {
                return Err(Error::message("ThingDB WAL lock poisoned"));
            };
            wal_lock_wait_ns = elapsed_nanos(wal_lock_started.elapsed());
            let started = Instant::now();
            wal_file.write_all(wal_buffer)?;
            appended = true;
            append_duration_ns = elapsed_nanos(started.elapsed());
            if let Some(point) = request_has_fault(requests, "after-wal-write-before-sync") {
                maybe_fail(point, Some(point))?;
            }
            let started = Instant::now();
            sync_wal(&wal_file, wal_sync_mode)?;
            synced = true;
            sync_duration_ns = elapsed_nanos(started.elapsed());
        }
        if let Ok(mut inner) = inner_arc.lock() {
            let journal_bytes = wal_start.saturating_add(wal_buffer.len() as u64);
            inner.diagnostics.append_duration_ns = inner
                .diagnostics
                .append_duration_ns
                .saturating_add(append_duration_ns);
            inner.diagnostics.wal_bytes_appended = inner
                .diagnostics
                .wal_bytes_appended
                .saturating_add(wal_buffer.len() as u64);
            inner.diagnostics.journal_bytes = journal_bytes;
            inner.diagnostics.wal_over_budget = journal_bytes > inner.max_journaling_size;
            if synced {
                inner.diagnostics.physical_sync_count =
                    inner.diagnostics.physical_sync_count.saturating_add(1);
                inner.diagnostics.sync_duration_ns = inner
                    .diagnostics
                    .sync_duration_ns
                    .saturating_add(sync_duration_ns);
            }
            inner.diagnostics.wal_lock_wait_duration_ns = inner
                .diagnostics
                .wal_lock_wait_duration_ns
                .saturating_add(wal_lock_wait_ns);
            inner.diagnostics.wal_lock_held_duration_ns = inner
                .diagnostics
                .wal_lock_held_duration_ns
                .saturating_add(append_duration_ns.saturating_add(sync_duration_ns));
        }
        if let Some(point) = request_has_fault(requests, "after-wal-sync-before-state-apply") {
            maybe_fail(point, Some(point))?;
        }
        let Ok(mut inner) = inner_arc.lock() else {
            return Err(Error::message("database lock poisoned"));
        };
        if inner.recovery_required
            || inner.sequence.saturating_add(1) != expected_sequence
            || inner.state_generation != expected_generation
        {
            inner.diagnostics.state_generation_conflict_count = inner
                .diagnostics
                .state_generation_conflict_count
                .saturating_add(1);
            return Err(Error::message(
                "ThingDB commit state changed before state application",
            ));
        }
        let started = Instant::now();
        let inner = &mut *inner;
        for request in requests.iter_mut() {
            for operation in std::mem::take(&mut request.operations) {
                apply_operation_with_pending(
                    &mut inner.state,
                    &mut inner.pending_table,
                    &mut inner.pending_table_bytes,
                    operation,
                );
            }
        }
        inner.diagnostics.state_apply_duration_ns = inner
            .diagnostics
            .state_apply_duration_ns
            .saturating_add(elapsed_nanos(started.elapsed()));
        inner.sequence = next_sequence.saturating_sub(1);
        inner.state_generation = inner.state_generation.saturating_add(1);
        inner.diagnostics.frame_count = inner.diagnostics.frame_count.saturating_add(group_size);
        inner.diagnostics.memtable_bytes = inner.pending_table_bytes;
        inner.diagnostics.memtable_over_budget =
            inner.pending_table_bytes >= inner.max_memtable_bytes;
        let wal_over_budget = inner.max_journaling_size > 0
            && inner.diagnostics.journal_bytes >= inner.max_journaling_size;
        if inner.pending_table_bytes >= inner.max_memtable_bytes || wal_over_budget {
            let scheduled = Arc::clone(&inner.flush_scheduled);
            let synchronous_flush = inner.max_memtable_bytes <= 1
                || request_has_fault(requests, "before-table-write").is_some();
            if synchronous_flush {
                flush_table_locked(
                    inner,
                    true,
                    request_has_fault(requests, "before-table-write"),
                )?;
                inner.diagnostics.automatic_flush_count =
                    inner.diagnostics.automatic_flush_count.saturating_add(1);
                if inner.table_layers.len() >= inner.max_table_layers {
                    compact_tables_locked(inner, true, current_fault_point())?;
                }
            } else if !schedule_background_flush(inner_arc, scheduled) {
                inner.recovery_required = true;
                inner.diagnostics.recovery_required = true;
                inner.diagnostics.last_error =
                    Some("ThingDB could not start its background flush worker".to_string());
                return Err(Error::message(
                    "ThingDB could not start its background flush worker",
                ));
            }
        }
        Ok(())
    })();

    if result.is_err() && !synced && appended {
        if let Ok(_wal_lock) = wal_io_lock.lock()
            && wal_file.set_len(wal_start).is_ok()
        {
            let _ = wal_file.seek(SeekFrom::End(0));
            if let Ok(mut inner) = inner_arc.lock() {
                inner.diagnostics.journal_bytes = wal_start;
            }
        } else if let Ok(mut inner) = inner_arc.lock() {
            inner.diagnostics.truncation_failure_count =
                inner.diagnostics.truncation_failure_count.saturating_add(1);
        }
    }
    (result, synced)
}

fn finish_group_with_error(requests: &mut [CommitRequest], message: &str) {
    for request in requests.iter_mut() {
        let _ = request.response.send(Err(Error::message(message)));
    }
}

/// An owned key/value returned by an iterator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    /// User key.
    pub key: Vec<u8>,
    /// User value.
    pub value: Vec<u8>,
}

/// An owned iterator over keyspace entries.
pub struct Iter {
    entries: std::vec::IntoIter<Entry>,
    error: Option<Error>,
}

impl Iter {
    /// Return a scan error observed while producing this iterator, if any.
    pub fn error(&self) -> Option<&Error> {
        self.error.as_ref()
    }

    /// Consume the iterator and return either all entries or its scan error.
    pub fn into_result(self) -> Result<Vec<Entry>> {
        if let Some(error) = self.error {
            return Err(error);
        }
        Ok(self.entries.collect())
    }
}

/// A consistent read snapshot.
#[derive(Clone)]
pub struct Snapshot {
    state: Arc<BTreeMap<Vec<u8>, Vec<u8>>>,
}

impl Database {
    fn update_ram_diagnostics(&self, update: impl FnOnce(&mut RamDiagnostics)) {
        if self.ram_keyspaces.is_some()
            && let Ok(mut diagnostics) = self.ram_diagnostics.lock()
        {
            update(&mut diagnostics);
        }
    }

    fn record_ram_lookup_timing(
        &self,
        lock_wait: u64,
        lock_held: u64,
        lookup: u64,
        clone: u64,
        acquire: u64,
    ) {
        if self.ram_keyspaces.is_none() {
            return;
        }
        self.ram_hot_diagnostics
            .lookup_count
            .fetch_add(1, Ordering::Relaxed);
        RamHotDiagnostics::add(&self.ram_hot_diagnostics.lock_wait_duration_ns, lock_wait);
        RamHotDiagnostics::add(&self.ram_hot_diagnostics.lock_held_duration_ns, lock_held);
        RamHotDiagnostics::add(&self.ram_hot_diagnostics.lookup_duration_ns, lookup);
        RamHotDiagnostics::add(&self.ram_hot_diagnostics.value_clone_duration_ns, clone);
        RamHotDiagnostics::add(&self.ram_hot_diagnostics.value_acquire_duration_ns, acquire);
    }

    fn record_ram_mutation_timing(&self, operation_count: u64, duration_ns: u64) {
        if self.ram_keyspaces.is_some() {
            RamHotDiagnostics::add(&self.ram_hot_diagnostics.mutation_count, operation_count);
            RamHotDiagnostics::add(&self.ram_hot_diagnostics.mutation_duration_ns, duration_ns);
        }
    }

    #[cfg(feature = "benchmark")]
    fn commit_ram_rcu_batch(&self, batch: Vec<BatchOperation>) -> Result<()> {
        let started = Instant::now();
        let rcu = self
            .ram_rcu_keyspaces
            .as_ref()
            .ok_or_else(|| Error::message("ThingDB RCU state is unavailable"))?;
        for operation in &batch {
            validate_memory_batch_operation(operation)?;
        }
        let operation_count = batch.len() as u64;
        let current = rcu.load_full();
        let mut next = current.as_ref().clone();
        for operation in batch {
            let keyspace = next
                .entry(Arc::clone(&operation.keyspace))
                .or_insert_with(|| Arc::new(BTreeMap::new()));
            let keyspace = Arc::make_mut(keyspace);
            match operation.value {
                Some(value) => {
                    keyspace.insert(operation.key, Arc::new(value));
                },
                None => {
                    keyspace.remove(&operation.key);
                },
            }
        }
        rcu.store(Arc::new(next));
        let elapsed = elapsed_nanos(started.elapsed());
        self.record_ram_mutation_timing(operation_count, elapsed);
        self.ram_logical_commit_count
            .fetch_add(1, Ordering::Relaxed);
        self.ram_total_group_size.fetch_add(1, Ordering::Relaxed);
        self.ram_state_apply_duration_ns
            .fetch_add(elapsed, Ordering::Relaxed);
        Ok(())
    }

    /// Create a true RAM-only ThingDB instance.
    ///
    /// This mode creates no files, WAL, manifest, table layers, or durable
    /// recovery state. All data is lost when the returned instance is dropped.
    pub fn in_memory() -> Result<Self> {
        let commit_gate = Arc::new(Mutex::new(()));
        let ram_diagnostics = Arc::new(Mutex::new(RamDiagnostics::default()));
        let ram_hot_diagnostics = Arc::new(RamHotDiagnostics::default());
        let ram_logical_commit_count = Arc::new(AtomicU64::new(0));
        let ram_total_group_size = Arc::new(AtomicU64::new(0));
        let ram_state_apply_duration_ns = Arc::new(AtomicU64::new(0));
        let inner = Arc::new(Mutex::new(Inner {
            path: PathBuf::new(),
            wal: None,
            lock: None,
            in_memory: true,
            state: BTreeMap::new(),
            sequence: 0,
            table_sequence: 0,
            table_files: Vec::new(),
            table_layers: Vec::new(),
            pending_table: BTreeMap::new(),
            pending_table_bytes: 0,
            max_journaling_size: 0,
            max_memtable_bytes: 0,
            max_table_layers: 0,
            diagnostics: WalDiagnostics::default(),
            recovery_required: false,
            commit_gate: Arc::clone(&commit_gate),
            wal_io_lock: Arc::new(Mutex::new(())),
            state_generation: 0,
            wal_sync_mode: WalSyncMode::Data,
            flush_scheduled: Arc::new(AtomicBool::new(false)),
        }));
        let writer = WriterCoordinator::new(Arc::downgrade(&inner))?;
        Ok(Self {
            inner,
            ram_keyspaces: Some(Arc::new(RwLock::new(HashMap::new()))),
            #[cfg(feature = "benchmark")]
            ram_rcu_keyspaces: None,
            ram_diagnostics,
            ram_hot_diagnostics,
            ram_logical_commit_count,
            ram_total_group_size,
            ram_state_apply_duration_ns,
            writer,
            commit_gate,
        })
    }

    /// Create a benchmark-only RAM database using immutable generations for
    /// reads. This does not change the default RAM implementation.
    #[cfg(feature = "benchmark")]
    #[doc(hidden)]
    pub fn in_memory_with_rcu() -> Result<Self> {
        let mut database = Self::in_memory()?;
        database.ram_rcu_keyspaces = Some(Arc::new(ArcSwap::from_pointee(HashMap::new())));
        Ok(database)
    }

    /// Start building a database at `path`.
    pub fn builder(path: impl AsRef<Path>) -> DatabaseBuilder {
        DatabaseBuilder {
            path: path.as_ref().to_path_buf(),
            max_journaling_size: 32 * 1024 * 1024,
            max_memtable_bytes: 64 * 1024 * 1024,
            max_table_layers: 8,
            wal_sync_mode: WalSyncMode::Data,
        }
    }

    /// Open a database with default options.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::builder(path).open()
    }

    /// Open a named keyspace.
    pub fn keyspace(&self, name: &str, _options: KeyspaceCreateOptions) -> Result<Keyspace> {
        if name.is_empty() || name.as_bytes().contains(&0) {
            return Err(Error::message(
                "keyspace names must be non-empty and contain no NUL",
            ));
        }
        Ok(Keyspace {
            db: self.clone(),
            name: Arc::from(name.to_owned()),
            namespace: Arc::from(namespace(name)),
        })
    }

    /// Create an empty atomic batch.
    pub fn batch(&self) -> Batch {
        self.batch_with_capacity(0)
    }

    /// Create an empty atomic batch with room for `capacity` operations.
    ///
    /// This is an allocation hint only; it does not change batch semantics.
    pub fn batch_with_capacity(&self, capacity: usize) -> Batch {
        Batch {
            db: self.clone(),
            memory_mode: self.ram_keyspaces.is_some(),
            operations: Vec::with_capacity(capacity),
        }
    }

    /// Flush durable state and compact the current state into one table.
    pub fn persist(&self, mode: PersistMode) -> Result<()> {
        if self
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?
            .in_memory
        {
            let _ = mode;
            return Err(Error::message(
                "ThingDB in-memory databases do not support persistence",
            ));
        }
        match mode {
            PersistMode::SyncAll => self.flush_table(true),
        }
    }

    /// Compact the database into a new immutable table.
    pub fn compact(&self) -> Result<()> {
        if self
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?
            .in_memory
        {
            return Err(Error::message(
                "ThingDB in-memory databases do not support compaction",
            ));
        }
        self.compact_tables(true)
    }

    /// Return a consistent snapshot of all keyspaces.
    pub fn snapshot(&self) -> Result<Snapshot> {
        let started = Instant::now();
        #[cfg(feature = "benchmark")]
        if let Some(rcu) = &self.ram_rcu_keyspaces {
            let generation = rcu.load_full();
            let mut state = BTreeMap::new();
            for (name, entries) in generation.iter() {
                let namespace = namespace(name);
                for (key, value) in entries.iter() {
                    let mut physical = namespace.clone();
                    physical.extend_from_slice(key);
                    state.insert(physical, value.as_ref().clone());
                }
            }
            let elapsed = elapsed_nanos(started.elapsed());
            self.update_ram_diagnostics(|diagnostics| {
                diagnostics.snapshot_count = diagnostics.snapshot_count.saturating_add(1);
                diagnostics.snapshot_duration_ns =
                    diagnostics.snapshot_duration_ns.saturating_add(elapsed);
            });
            return Ok(Snapshot {
                state: Arc::new(state),
            });
        }
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        if inner.in_memory {
            drop(inner);
            let keyspaces = self
                .ram_keyspaces
                .as_ref()
                .ok_or_else(|| Error::message("ThingDB RAM state is unavailable"))?
                .read()
                .map_err(|_| Error::message("ThingDB RAM state lock poisoned"))?;
            let mut state = BTreeMap::new();
            for (name, entries) in keyspaces.iter() {
                let namespace = namespace(name);
                for (key, value) in entries {
                    let mut physical = namespace.clone();
                    physical.extend_from_slice(key);
                    state.insert(physical, value.as_ref().clone());
                }
            }
            drop(keyspaces);
            let elapsed = elapsed_nanos(started.elapsed());
            self.update_ram_diagnostics(|diagnostics| {
                diagnostics.snapshot_count = diagnostics.snapshot_count.saturating_add(1);
                diagnostics.snapshot_duration_ns =
                    diagnostics.snapshot_duration_ns.saturating_add(elapsed);
            });
            return Ok(Snapshot {
                state: Arc::new(state),
            });
        }
        Ok(Snapshot {
            state: Arc::new(inner.materialize_state()?),
        })
    }

    /// Approximate current WAL size in bytes.
    pub fn journal_disk_space(&self) -> Result<u64> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        if inner.in_memory {
            Ok(0)
        } else {
            inner
                .wal
                .as_ref()
                .ok_or_else(|| Error::message("ThingDB WAL is unavailable"))?
                .metadata()
                .map(|metadata| metadata.len())
                .map_err(Error::from)
        }
    }

    /// Return the number of WAL files currently present.
    pub fn journal_count(&self) -> usize {
        self.inner
            .lock()
            .map(|inner| usize::from(!inner.in_memory))
            .unwrap_or_default()
    }

    /// Return bounded WAL and recovery diagnostics.
    pub fn wal_diagnostics(&self) -> Result<WalDiagnostics> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        let mut diagnostics = inner.diagnostics.clone();
        if inner.in_memory {
            diagnostics.logical_commit_count =
                self.ram_logical_commit_count.load(Ordering::Relaxed);
            diagnostics.total_group_size = self.ram_total_group_size.load(Ordering::Relaxed);
            diagnostics.max_group_size = u64::from(diagnostics.logical_commit_count > 0);
            diagnostics.state_apply_duration_ns =
                self.ram_state_apply_duration_ns.load(Ordering::Relaxed);
        }
        diagnostics.journal_bytes = if inner.in_memory {
            0
        } else {
            inner
                .wal
                .as_ref()
                .ok_or_else(|| Error::message("ThingDB WAL is unavailable"))?
                .metadata()
                .map_err(Error::from)?
                .len()
        };
        Ok(diagnostics)
    }

    /// Return RAM-only lookup and mutation timings.
    pub fn ram_diagnostics(&self) -> Result<RamDiagnostics> {
        if self.ram_keyspaces.is_some() {
            let mut diagnostics = self
                .ram_diagnostics
                .lock()
                .map_err(|_| Error::message("RAM diagnostics lock poisoned"))?
                .clone();
            self.ram_hot_diagnostics.snapshot_into(&mut diagnostics);
            Ok(diagnostics)
        } else {
            Ok(RamDiagnostics::default())
        }
    }

    /// Record Thingd-layer deserialization time for RAM diagnostics.
    pub fn record_ram_deserialization(&self, duration_ns: u64) {
        if self.ram_keyspaces.is_some() {
            RamHotDiagnostics::add(
                &self.ram_hot_diagnostics.deserialization_duration_ns,
                duration_ns,
            );
        }
    }

    /// Record Thingd-layer search time for RAM diagnostics.
    pub fn record_ram_search(&self, duration_ns: u64) {
        self.update_ram_diagnostics(|diagnostics| {
            diagnostics.search_count = diagnostics.search_count.saturating_add(1);
            diagnostics.search_duration_ns =
                diagnostics.search_duration_ns.saturating_add(duration_ns);
        });
    }

    /// Record semantic-layer serialization time for RAM diagnostics.
    pub fn record_ram_serialization(&self, duration_ns: u64) {
        if self.ram_keyspaces.is_some() {
            self.ram_hot_diagnostics
                .serialization_count
                .fetch_add(1, Ordering::Relaxed);
            RamHotDiagnostics::add(
                &self.ram_hot_diagnostics.serialization_duration_ns,
                duration_ns,
            );
        }
    }

    /// Record semantic-layer derived-index mutation time for RAM diagnostics.
    pub fn record_ram_search_index(&self, duration_ns: u64) {
        self.update_ram_diagnostics(|diagnostics| {
            diagnostics.search_index_count = diagnostics.search_index_count.saturating_add(1);
            diagnostics.search_index_duration_ns = diagnostics
                .search_index_duration_ns
                .saturating_add(duration_ns);
        });
    }

    fn flush_table(&self, sync: bool) -> Result<()> {
        let _gate = self
            .commit_gate
            .lock()
            .map_err(|_| Error::message("ThingDB commit gate poisoned"))?;
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        flush_table_locked(&mut inner, sync, current_fault_point())
    }

    fn commit_batch(&self, batch: Vec<BatchOperation>) -> Result<()> {
        if batch.is_empty() {
            return Ok(());
        }

        #[cfg(feature = "benchmark")]
        if self.ram_rcu_keyspaces.is_some() {
            return self.commit_ram_rcu_batch(batch);
        }

        if self.ram_keyspaces.is_some() {
            let started = Instant::now();
            let ram_keyspaces = self
                .ram_keyspaces
                .as_ref()
                .ok_or_else(|| Error::message("ThingDB RAM state is unavailable"))?;
            let mut keyspaces = ram_keyspaces
                .write()
                .map_err(|_| Error::message("ThingDB RAM state lock poisoned"))?;
            for operation in &batch {
                validate_memory_batch_operation(operation)?;
            }
            let operation_count = batch.len() as u64;
            for operation in batch {
                apply_memory_batch_operation(&mut keyspaces, operation);
            }
            let elapsed = elapsed_nanos(started.elapsed());
            drop(keyspaces);
            self.record_ram_mutation_timing(operation_count, elapsed);
            self.ram_logical_commit_count
                .fetch_add(1, Ordering::Relaxed);
            self.ram_total_group_size.fetch_add(1, Ordering::Relaxed);
            self.ram_state_apply_duration_ns
                .fetch_add(elapsed, Ordering::Relaxed);
            return Ok(());
        }

        let operations = batch
            .into_iter()
            .map(|operation| match operation.value {
                Some(value) => Operation::Put {
                    key: operation.key,
                    value,
                },
                None => Operation::Delete { key: operation.key },
            })
            .collect();
        self.commit_operations(operations)
    }

    fn commit_operations(&self, operations: Vec<Operation>) -> Result<()> {
        if operations.is_empty() {
            return Ok(());
        }

        if self.ram_keyspaces.is_some() {
            let started = Instant::now();
            let ram_keyspaces = self
                .ram_keyspaces
                .as_ref()
                .ok_or_else(|| Error::message("ThingDB RAM state is unavailable"))?;
            let mut keyspaces = ram_keyspaces
                .write()
                .map_err(|_| Error::message("ThingDB RAM state lock poisoned"))?;
            for operation in &operations {
                validate_memory_operation(operation)?;
            }
            let operation_count = operations.len() as u64;
            for operation in operations {
                apply_memory_operation(&mut keyspaces, operation)?;
            }
            let elapsed = elapsed_nanos(started.elapsed());
            drop(keyspaces);
            self.record_ram_mutation_timing(operation_count, elapsed);
            self.ram_logical_commit_count
                .fetch_add(1, Ordering::Relaxed);
            self.ram_total_group_size.fetch_add(1, Ordering::Relaxed);
            self.ram_state_apply_duration_ns
                .fetch_add(elapsed_nanos(started.elapsed()), Ordering::Relaxed);
            return Ok(());
        }

        let (response, result) = mpsc::channel();
        let encoded_bytes = encoded_frame_capacity(&operations);
        let request = CommitRequest {
            encoded_bytes,
            operations,
            submitted_at: Instant::now(),
            fault_point: current_fault_point(),
            response,
        };
        let sender = self
            .writer
            .sender
            .lock()
            .map_err(|_| Error::message("ThingDB writer state poisoned"))?
            .clone()
            .ok_or_else(|| Error::message("ThingDB writer is unavailable"))?;
        sender
            .send(request)
            .map_err(|_| Error::message("ThingDB writer is unavailable"))?;
        result
            .recv()
            .map_err(|_| Error::message("ThingDB writer stopped unexpectedly"))?
    }
}

fn flush_table_locked(
    inner: &mut Inner,
    sync: bool,
    fault_point: Option<&'static str>,
) -> Result<()> {
    let started = Instant::now();
    if inner.recovery_required {
        return Err(Error::message(
            "ThingDB requires reopen and recovery before writing",
        ));
    }
    if inner.pending_table.is_empty() {
        let _wal_lock = inner
            .wal_io_lock
            .lock()
            .map_err(|_| Error::message("ThingDB WAL lock poisoned"))?;
        let wal = inner
            .wal
            .as_mut()
            .ok_or_else(|| Error::message("ThingDB WAL is unavailable"))?;
        sync_wal(wal, inner.wal_sync_mode)?;
        return Ok(());
    }
    let updates = std::mem::take(&mut inner.pending_table);
    let update_bytes = inner.pending_table_bytes;
    let result = (|| {
        // Every operation reaches this path only after execute_group has
        // synced its WAL frame before acknowledging the write. Keep the WAL
        // mutex for append/truncate ordering, but do not issue a second
        // physical sync before writing the immutable table. The final sync
        // below still makes the table publication durable for SyncAll.
        let _wal_lock = inner
            .wal_io_lock
            .lock()
            .map_err(|_| Error::message("ThingDB WAL lock poisoned"))?;
        let next_sequence = inner.sequence;
        let table_name = format!(
            "table-{next_sequence:020}-{:04}.tdb",
            inner.table_files.len()
        );
        let table_path = inner.path.join(&table_name);
        let temp_path = inner.path.join(format!(".{table_name}.tmp"));
        maybe_fail("before-table-write", fault_point)?;
        write_table(&temp_path, next_sequence, &updates, sync)?;
        inner.diagnostics.table_bytes_written = inner
            .diagnostics
            .table_bytes_written
            .saturating_add(fs::metadata(&temp_path)?.len());
        maybe_fail("after-table-sync-before-rename", fault_point)?;
        maybe_fail("before-table-rename", fault_point)?;
        fs::rename(&temp_path, &table_path)?;
        maybe_fail("after-table-rename-before-manifest", fault_point)?;
        let mut table_files = inner.table_files.clone();
        table_files.push(table_name.clone());
        let manifest = Manifest {
            format_version: FORMAT_VERSION,
            table_file: Some(table_name),
            table_files: table_files.clone(),
            table_sequence: next_sequence,
        };
        maybe_fail("before-manifest-write", fault_point)?;
        write_manifest(&inner.path, &manifest, sync)?;
        maybe_fail("after-manifest-rename-before-wal-truncate", fault_point)?;
        inner.table_files = table_files;
        let (_, entries, is_v2, blocks) = read_table_index(&table_path)?;
        inner.table_layers.push(TableLayer {
            file: Arc::new(File::open(&table_path)?),
            path: table_path,
            entries: Arc::new(OnceLock::from(Arc::new(entries))),
            blocks: Arc::new(blocks),
            is_v2,
        });
        inner.diagnostics.table_layer_count = inner.table_layers.len() as u64;
        inner.table_sequence = next_sequence;
        inner.state.clear();
        inner.pending_table_bytes = 0;
        {
            let wal = inner
                .wal
                .as_mut()
                .ok_or_else(|| Error::message("ThingDB WAL is unavailable"))?;
            wal.set_len(0)?;
            if sync {
                sync_wal(wal, inner.wal_sync_mode)?;
            }
            inner.diagnostics.journal_bytes = 0;
        }
        inner.diagnostics.frame_count = 0;
        inner.diagnostics.memtable_bytes = 0;
        inner.diagnostics.memtable_over_budget = false;
        inner.diagnostics.flush_count = inner.diagnostics.flush_count.saturating_add(1);
        Ok(())
    })();
    if result.is_err() {
        inner.pending_table = updates;
        inner.pending_table_bytes = update_bytes;
    }
    inner.diagnostics.flush_duration_ns = inner
        .diagnostics
        .flush_duration_ns
        .saturating_add(elapsed_nanos(started.elapsed()));
    result
}

impl Database {
    fn compact_tables(&self, sync: bool) -> Result<()> {
        let _gate = self
            .commit_gate
            .lock()
            .map_err(|_| Error::message("ThingDB commit gate poisoned"))?;
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        compact_tables_locked(&mut inner, sync, current_fault_point())
    }
}

fn compact_tables_locked(
    inner: &mut Inner,
    sync: bool,
    fault_point: Option<&'static str>,
) -> Result<()> {
    let started = Instant::now();
    if inner.recovery_required {
        return Err(Error::message(
            "ThingDB requires reopen and recovery before writing",
        ));
    }
    {
        let _wal_lock = inner
            .wal_io_lock
            .lock()
            .map_err(|_| Error::message("ThingDB WAL lock poisoned"))?;
        let wal = inner
            .wal
            .as_mut()
            .ok_or_else(|| Error::message("ThingDB WAL is unavailable"))?;
        sync_wal(wal, inner.wal_sync_mode)?;
    }
    let next_sequence = inner.sequence;
    let table_name = format!("table-{next_sequence:020}-compact.tdb");
    let table_path = inner.path.join(&table_name);
    let temp_path = inner.path.join(format!(".{table_name}.tmp"));
    let mut input_bytes: u64 = 0;
    for layer in &inner.table_layers {
        input_bytes = input_bytes.saturating_add(
            layer
                .loaded_entries()?
                .iter()
                .map(|entry| entry.length)
                .sum::<u64>(),
        );
    }
    let entries: BTreeMap<_, _> = inner
        .materialize_state()?
        .into_iter()
        .map(|(key, value)| (key, Some(value)))
        .collect();
    maybe_fail("before-table-write", fault_point)?;
    write_table(&temp_path, next_sequence, &entries, sync)?;
    inner.diagnostics.table_bytes_written = inner
        .diagnostics
        .table_bytes_written
        .saturating_add(fs::metadata(&temp_path)?.len());
    maybe_fail("after-table-sync-before-rename", fault_point)?;
    maybe_fail("before-table-rename", fault_point)?;
    fs::rename(&temp_path, &table_path)?;
    maybe_fail("after-table-rename-before-manifest", fault_point)?;
    let manifest = Manifest {
        format_version: FORMAT_VERSION,
        table_file: Some(table_name.clone()),
        table_files: vec![table_name.clone()],
        table_sequence: next_sequence,
    };
    maybe_fail("before-manifest-write", fault_point)?;
    write_manifest(&inner.path, &manifest, sync)?;
    maybe_fail("after-manifest-rename-before-wal-truncate", fault_point)?;
    for old_table in &inner.table_files {
        if old_table != &table_name {
            let _ = fs::remove_file(inner.path.join(old_table));
        }
    }
    inner.table_files = vec![table_name];
    let (_, entries, is_v2, blocks) = read_table_index(&table_path)?;
    inner.table_layers = vec![TableLayer {
        file: Arc::new(File::open(&table_path)?),
        path: table_path.clone(),
        entries: Arc::new(OnceLock::from(Arc::new(entries))),
        blocks: Arc::new(blocks),
        is_v2,
    }];
    inner.diagnostics.table_layer_count = 1;
    inner.table_sequence = next_sequence;
    inner.state.clear();
    inner.pending_table.clear();
    inner.pending_table_bytes = 0;
    {
        let wal = inner
            .wal
            .as_mut()
            .ok_or_else(|| Error::message("ThingDB WAL is unavailable"))?;
        wal.set_len(0)?;
        if sync {
            sync_wal(wal, inner.wal_sync_mode)?;
        }
        inner.diagnostics.journal_bytes = 0;
    }
    inner.diagnostics.frame_count = 0;
    inner.diagnostics.memtable_bytes = 0;
    inner.diagnostics.memtable_over_budget = false;
    inner.diagnostics.compaction_count = inner.diagnostics.compaction_count.saturating_add(1);
    inner.diagnostics.compaction_duration_ns = inner
        .diagnostics
        .compaction_duration_ns
        .saturating_add(elapsed_nanos(started.elapsed()));
    inner.diagnostics.compaction_input_bytes = input_bytes;
    inner.diagnostics.compaction_output_bytes = fs::metadata(&table_path)?.len();
    Ok(())
}

impl DatabaseBuilder {
    /// Use full-file synchronization for benchmark-only cross-backend
    /// comparisons. This is not enabled in normal builds or runtime paths.
    #[cfg(feature = "benchmark")]
    #[doc(hidden)]
    pub fn benchmark_common_fsync(mut self) -> Self {
        self.wal_sync_mode = WalSyncMode::All;
        self
    }

    /// Set the soft WAL budget used for diagnostics and future backpressure.
    pub fn max_journaling_size(mut self, bytes: u64) -> Self {
        self.max_journaling_size = bytes;
        self
    }

    /// Set the maximum mutable table size before an automatic durable flush.
    ///
    /// A single commit may exceed this bound when its operation set is larger
    /// than the configured limit. Normal commits acknowledge after WAL sync
    /// and state application while background maintenance catches up; a hard
    /// upper bound applies backpressure when maintenance falls behind.
    pub fn max_memtable_bytes(mut self, bytes: u64) -> Self {
        self.max_memtable_bytes = bytes.max(1);
        self
    }

    /// Set the maximum number of immutable table layers before compaction.
    pub fn max_table_layers(mut self, layers: usize) -> Self {
        self.max_table_layers = layers.max(1);
        self
    }

    /// Open or create the database.
    pub fn open(self) -> Result<Database> {
        fs::create_dir_all(&self.path)?;
        let lock_path = self.path.join(LOCK_FILE);
        let lock = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .map_err(|error| {
                if error.kind() == ErrorKind::AlreadyExists {
                    Error::message(format!("database is locked: {}", self.path.display()))
                } else {
                    error.into()
                }
            })?;
        let result = Self::open_locked(self, lock);
        if result.is_err() {
            let _ = fs::remove_file(lock_path);
        }
        result
    }

    #[allow(clippy::too_many_lines)]
    fn open_locked(self, lock: File) -> Result<Database> {
        cleanup_temporary_files(&self.path)?;
        let manifest_path = self.path.join(MANIFEST_FILE);
        let manifest = if manifest_path.exists() {
            let manifest: Manifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
            if manifest.format_version != FORMAT_VERSION {
                return Err(Error::message(format!(
                    "unsupported ThingDB format version {}",
                    manifest.format_version
                )));
            }
            validate_manifest(&manifest)?;
            Some(manifest)
        } else {
            None
        };
        let mut state = BTreeMap::new();
        let mut table_sequence = 0;
        let mut table_layers = Vec::new();
        let mut table_open_duration_ns: u64 = 0;
        let table_files = manifest
            .as_ref()
            .map(|manifest| {
                if manifest.table_files.is_empty() {
                    manifest.table_file.iter().cloned().collect::<Vec<_>>()
                } else {
                    manifest.table_files.clone()
                }
            })
            .unwrap_or_default();
        if let Some(manifest) = &manifest {
            let mut previous_sequence = 0;
            for table_file in &table_files {
                let table_path = self.path.join(table_file);
                if !table_path.is_file() {
                    return Err(Error::message(format!(
                        "ThingDB manifest references missing table: {table_file}"
                    )));
                }
                let (sequence, entries, is_v2, blocks) = read_table_metadata(&table_path)?;
                if sequence > manifest.table_sequence {
                    return Err(Error::message("table sequence exceeds manifest sequence"));
                }
                if sequence < previous_sequence {
                    return Err(Error::message(
                        "ThingDB table layers are not in sequence order",
                    ));
                }
                previous_sequence = sequence;
                let open_started = Instant::now();
                let file = File::open(&table_path)?;
                table_open_duration_ns =
                    table_open_duration_ns.saturating_add(elapsed_nanos(open_started.elapsed()));
                table_layers.push(TableLayer {
                    file: Arc::new(file),
                    path: table_path.clone(),
                    entries: Arc::new(if blocks.is_empty() {
                        OnceLock::from(Arc::new(entries))
                    } else {
                        OnceLock::new()
                    }),
                    blocks: Arc::new(blocks),
                    is_v2,
                });
            }
            table_sequence = manifest.table_sequence;
        }
        let wal_path = self.path.join(WAL_FILE);
        let mut wal = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&wal_path)?;
        let recovery_started = Instant::now();
        let (last_sequence, valid_offset, frame_count) =
            replay_wal(&mut wal, table_sequence, &mut state)?;
        let recovery_duration_ns = elapsed_nanos(recovery_started.elapsed());
        let wal_len = wal.metadata()?.len();
        if valid_offset < wal_len {
            wal.set_len(valid_offset)?;
            wal.seek(SeekFrom::End(0))?;
        }
        let journal_bytes = wal.metadata()?.len();
        let sequence = last_sequence.max(table_sequence);
        let table_layer_count = table_layers.len() as u64;
        if manifest.is_none() {
            write_manifest(
                &self.path,
                &Manifest {
                    format_version: FORMAT_VERSION,
                    table_file: None,
                    table_files: Vec::new(),
                    table_sequence: 0,
                },
                true,
            )?;
        }
        let commit_gate = Arc::new(Mutex::new(()));
        let ram_diagnostics = Arc::new(Mutex::new(RamDiagnostics::default()));
        let ram_hot_diagnostics = Arc::new(RamHotDiagnostics::default());
        let inner = Arc::new(Mutex::new(Inner {
            path: self.path,
            wal: Some(Arc::new(wal)),
            lock: Some(lock),
            in_memory: false,
            state,
            sequence,
            table_sequence,
            table_files,
            table_layers,
            pending_table: BTreeMap::new(),
            pending_table_bytes: 0,
            max_journaling_size: self.max_journaling_size,
            max_memtable_bytes: self.max_memtable_bytes,
            max_table_layers: self.max_table_layers,
            diagnostics: WalDiagnostics {
                journal_bytes,
                frame_count,
                recovery_bytes: valid_offset,
                recovery_duration_ns,
                table_layer_count,
                table_open_duration_ns,
                ..WalDiagnostics::default()
            },
            recovery_required: false,
            commit_gate: Arc::clone(&commit_gate),
            wal_io_lock: Arc::new(Mutex::new(())),
            state_generation: 0,
            wal_sync_mode: self.wal_sync_mode,
            flush_scheduled: Arc::new(AtomicBool::new(false)),
        }));
        let writer = WriterCoordinator::new(Arc::downgrade(&inner))?;
        Ok(Database {
            inner,
            ram_keyspaces: None,
            #[cfg(feature = "benchmark")]
            ram_rcu_keyspaces: None,
            ram_diagnostics,
            ram_hot_diagnostics,
            ram_logical_commit_count: Arc::new(AtomicU64::new(0)),
            ram_total_group_size: Arc::new(AtomicU64::new(0)),
            ram_state_apply_duration_ns: Arc::new(AtomicU64::new(0)),
            writer,
            commit_gate,
        })
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(lock) = self.lock.as_ref() {
            let _ = lock.sync_all();
            let _ = fs::remove_file(self.path.join(LOCK_FILE));
        }
    }
}

impl Database {
    #[allow(clippy::too_many_lines)]
    fn get_durable_value(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let lock_started = Instant::now();
        let (resolved, immediate, table_layers) = {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| Error::message("database lock poisoned"))?;
            let lock_wait = elapsed_nanos(lock_started.elapsed());
            let lookup_started = Instant::now();
            let mut resolved = false;
            let mut immediate = None;
            let mut table_layers = Vec::new();

            inner.diagnostics.pending_table_lookup_count = inner
                .diagnostics
                .pending_table_lookup_count
                .saturating_add(1);
            if let Some(value) = inner.pending_table.get(key) {
                resolved = true;
                immediate.clone_from(value);
            } else {
                inner.diagnostics.mutable_state_lookup_count = inner
                    .diagnostics
                    .mutable_state_lookup_count
                    .saturating_add(1);
                if let Some(value) = inner.state.get(key) {
                    resolved = true;
                    immediate = Some(value.clone());
                } else {
                    inner.diagnostics.table_lookup_count =
                        inner.diagnostics.table_lookup_count.saturating_add(1);
                    table_layers.clone_from(&inner.table_layers);
                    let layers_examined = table_layers.len();
                    inner.diagnostics.immutable_layer_lookup_count = inner
                        .diagnostics
                        .immutable_layer_lookup_count
                        .saturating_add(layers_examined as u64);
                    inner.diagnostics.table_layers_consulted = inner
                        .diagnostics
                        .table_layers_consulted
                        .saturating_add(layers_examined as u64);
                }
            }
            let lookup_duration = elapsed_nanos(lookup_started.elapsed());
            let held_duration = elapsed_nanos(lock_started.elapsed());
            inner.diagnostics.read_lock_wait_duration_ns = inner
                .diagnostics
                .read_lock_wait_duration_ns
                .saturating_add(lock_wait);
            inner.diagnostics.read_lock_held_duration_ns = inner
                .diagnostics
                .read_lock_held_duration_ns
                .saturating_add(held_duration);
            inner.diagnostics.table_lookup_duration_ns = inner
                .diagnostics
                .table_lookup_duration_ns
                .saturating_add(lookup_duration);
            (resolved, immediate, table_layers)
        };

        if resolved {
            return Ok(immediate);
        }
        for layer in table_layers.iter().rev() {
            if let Some(block) = layer.candidate_block(key) {
                let (found, bytes_read, read_duration) =
                    read_table_block_value(&layer.file, block, key, layer.is_v2)?;
                let mut inner = self
                    .inner
                    .lock()
                    .map_err(|_| Error::message("database lock poisoned"))?;
                inner.diagnostics.table_bytes_read = inner
                    .diagnostics
                    .table_bytes_read
                    .saturating_add(bytes_read);
                inner.diagnostics.table_read_duration_ns = inner
                    .diagnostics
                    .table_read_duration_ns
                    .saturating_add(read_duration);
                inner.diagnostics.file_read_duration_ns = inner
                    .diagnostics
                    .file_read_duration_ns
                    .saturating_add(read_duration);
                if let Some(value) = found {
                    return Ok(value);
                }
                continue;
            }
            let entries = layer.loaded_entries()?;
            let candidate_range = layer.candidate_entry_range(key);
            let Ok(relative_index) = entries[candidate_range.clone()]
                .binary_search_by(|entry| entry.key.as_slice().cmp(key))
            else {
                continue;
            };
            let index = candidate_range.start + relative_index;
            let entry = &entries[index];
            let (result, reader_wait, file_read) =
                read_table_value_handle(&layer.file, entry, layer.is_v2)?;
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| Error::message("database lock poisoned"))?;
            inner.diagnostics.table_reader_wait_duration_ns = inner
                .diagnostics
                .table_reader_wait_duration_ns
                .saturating_add(reader_wait);
            inner.diagnostics.file_read_duration_ns = inner
                .diagnostics
                .file_read_duration_ns
                .saturating_add(file_read);
            inner.diagnostics.table_bytes_read = inner
                .diagnostics
                .table_bytes_read
                .saturating_add(entry.length);
            inner.diagnostics.table_read_duration_ns = inner
                .diagnostics
                .table_read_duration_ns
                .saturating_add(file_read);
            return Ok(result);
        }
        Ok(None)
    }
}

impl Keyspace {
    /// Read a value through a callback without cloning it in RAM-only mode.
    pub fn with_value<T>(
        &self,
        key: impl AsRef<[u8]>,
        callback: impl FnOnce(Option<&[u8]>) -> std::result::Result<T, String>,
    ) -> Result<T> {
        let key = key.as_ref();
        #[cfg(feature = "benchmark")]
        if let Some(rcu) = &self.db.ram_rcu_keyspaces {
            let started = Instant::now();
            let generation = rcu.load_full();
            let value = generation
                .get(self.name.as_ref())
                .and_then(|keyspace| keyspace.get(key))
                .map(|value| value.as_slice());
            let result = callback(value).map_err(Error::message);
            self.db
                .record_ram_lookup_timing(0, 0, elapsed_nanos(started.elapsed()), 0, 0);
            return result;
        }
        if self.db.ram_keyspaces.is_some() {
            let lookup_started = Instant::now();
            let ram_keyspaces = self
                .db
                .ram_keyspaces
                .as_ref()
                .ok_or_else(|| Error::message("ThingDB RAM state is unavailable"))?;
            let state_lock_started = Instant::now();
            let keyspaces = ram_keyspaces
                .read()
                .map_err(|_| Error::message("ThingDB RAM state lock poisoned"))?;
            let lock_wait = elapsed_nanos(state_lock_started.elapsed());
            let value = keyspaces
                .get(self.name.as_ref())
                .and_then(|keyspace| keyspace.get(key))
                .map(|value| value.as_slice());
            let result = callback(value).map_err(Error::message);
            let lookup_duration = elapsed_nanos(lookup_started.elapsed());
            drop(keyspaces);
            self.db.record_ram_lookup_timing(
                lock_wait,
                lookup_duration.saturating_sub(lock_wait),
                lookup_duration,
                0,
                0,
            );
            return result;
        }
        let lock_started = Instant::now();
        let inner = self
            .db
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        let lock_wait = elapsed_nanos(lock_started.elapsed());
        if inner.in_memory {
            drop(inner);
            let lookup_started = Instant::now();
            let ram_keyspaces = self
                .db
                .ram_keyspaces
                .as_ref()
                .ok_or_else(|| Error::message("ThingDB RAM state is unavailable"))?;
            let keyspaces = ram_keyspaces
                .read()
                .map_err(|_| Error::message("ThingDB RAM state lock poisoned"))?;
            let value = keyspaces
                .get(self.name.as_ref())
                .and_then(|keyspace| keyspace.get(key))
                .map(|value| value.as_slice());
            let lookup_duration = elapsed_nanos(lookup_started.elapsed());
            let result = callback(value).map_err(Error::message);
            let held_duration = lookup_duration;
            drop(keyspaces);
            self.db
                .record_ram_lookup_timing(lock_wait, held_duration, lookup_duration, 0, 0);
            return result;
        }
        drop(inner);
        let value = self.db.get_durable_value(key)?;
        callback(value.as_deref()).map_err(Error::message)
    }

    /// Read a RAM value through a shared ownership boundary.
    ///
    /// The database lock is released before the caller processes the value.
    /// This is useful for semantic adapters that need to deserialize a value:
    /// the bytes remain alive through the returned `Arc`, without blocking
    /// unrelated readers while deserialization runs.
    pub fn get_shared(&self, key: impl AsRef<[u8]>) -> Result<Option<Arc<Vec<u8>>>> {
        let key = key.as_ref();
        #[cfg(feature = "benchmark")]
        if let Some(rcu) = &self.db.ram_rcu_keyspaces {
            let started = Instant::now();
            let generation = rcu.load_full();
            let value = generation
                .get(self.name.as_ref())
                .and_then(|keyspace| keyspace.get(key))
                .cloned();
            self.db
                .record_ram_lookup_timing(0, elapsed_nanos(started.elapsed()), 0, 0, 0);
            return Ok(value);
        }
        if self.db.ram_keyspaces.is_some() {
            let lookup_started = Instant::now();
            let ram_keyspaces = self
                .db
                .ram_keyspaces
                .as_ref()
                .ok_or_else(|| Error::message("ThingDB RAM state is unavailable"))?;
            let state_lock_started = Instant::now();
            let keyspaces = ram_keyspaces
                .read()
                .map_err(|_| Error::message("ThingDB RAM state lock poisoned"))?;
            let lock_wait = elapsed_nanos(state_lock_started.elapsed());
            let value_ref = keyspaces
                .get(self.name.as_ref())
                .and_then(|keyspace| keyspace.get(key));
            let acquire_started = Instant::now();
            let value = value_ref.cloned();
            let acquire_duration = elapsed_nanos(acquire_started.elapsed());
            let lookup_duration = elapsed_nanos(lookup_started.elapsed());
            drop(keyspaces);
            self.db.record_ram_lookup_timing(
                lock_wait,
                lookup_duration.saturating_sub(lock_wait),
                lookup_duration,
                0,
                acquire_duration,
            );
            return Ok(value);
        }
        let lock_started = Instant::now();
        let inner = self
            .db
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        let lock_wait = elapsed_nanos(lock_started.elapsed());
        if !inner.in_memory {
            return Err(Error::message(
                "ThingDB shared reads are available only in memory mode",
            ));
        }
        drop(inner);
        let lookup_started = Instant::now();
        let ram_keyspaces = self
            .db
            .ram_keyspaces
            .as_ref()
            .ok_or_else(|| Error::message("ThingDB RAM state is unavailable"))?;
        let keyspaces = ram_keyspaces
            .read()
            .map_err(|_| Error::message("ThingDB RAM state lock poisoned"))?;
        let value_ref = keyspaces
            .get(self.name.as_ref())
            .and_then(|keyspace| keyspace.get(key));
        let lookup_duration = elapsed_nanos(lookup_started.elapsed());
        let acquire_started = Instant::now();
        let value = value_ref.cloned();
        let acquire_duration = elapsed_nanos(acquire_started.elapsed());
        let held_duration = lookup_duration.saturating_add(acquire_duration);
        drop(keyspaces);
        self.db.record_ram_lookup_timing(
            lock_wait,
            held_duration,
            lookup_duration,
            0,
            acquire_duration,
        );
        Ok(value)
    }

    /// Read a value by user key.
    pub fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Vec<u8>>> {
        let key = key.as_ref();
        #[cfg(feature = "benchmark")]
        if let Some(rcu) = &self.db.ram_rcu_keyspaces {
            let started = Instant::now();
            let generation = rcu.load_full();
            let value = generation
                .get(self.name.as_ref())
                .and_then(|keyspace| keyspace.get(key))
                .map(|value| value.as_ref().clone());
            let elapsed = elapsed_nanos(started.elapsed());
            self.db
                .record_ram_lookup_timing(0, elapsed, elapsed, elapsed, 0);
            return Ok(value);
        }
        if self.db.ram_keyspaces.is_some() {
            let lookup_started = Instant::now();
            let ram_keyspaces = self
                .db
                .ram_keyspaces
                .as_ref()
                .ok_or_else(|| Error::message("ThingDB RAM state is unavailable"))?;
            let state_lock_started = Instant::now();
            let keyspaces = ram_keyspaces
                .read()
                .map_err(|_| Error::message("ThingDB RAM state lock poisoned"))?;
            let lock_wait = elapsed_nanos(state_lock_started.elapsed());
            let value = keyspaces
                .get(self.name.as_ref())
                .and_then(|keyspace| keyspace.get(key));
            let clone_started = Instant::now();
            let value = value.map(|value| value.as_ref().clone());
            let clone_duration = elapsed_nanos(clone_started.elapsed());
            let lookup_duration = elapsed_nanos(lookup_started.elapsed());
            drop(keyspaces);
            self.db.record_ram_lookup_timing(
                lock_wait,
                lookup_duration.saturating_sub(lock_wait),
                lookup_duration,
                clone_duration,
                0,
            );
            return Ok(value);
        }
        let lock_started = Instant::now();
        let inner = self
            .db
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        let lock_wait = elapsed_nanos(lock_started.elapsed());
        if inner.in_memory {
            drop(inner);
            let lookup_started = Instant::now();
            let ram_keyspaces = self
                .db
                .ram_keyspaces
                .as_ref()
                .ok_or_else(|| Error::message("ThingDB RAM state is unavailable"))?;
            let keyspaces = ram_keyspaces
                .read()
                .map_err(|_| Error::message("ThingDB RAM state lock poisoned"))?;
            let value = keyspaces
                .get(self.name.as_ref())
                .and_then(|keyspace| keyspace.get(key));
            let lookup_duration = elapsed_nanos(lookup_started.elapsed());
            let clone_started = Instant::now();
            let value = value.map(|value| value.as_ref().clone());
            let clone_duration = elapsed_nanos(clone_started.elapsed());
            let held_duration = lookup_duration.saturating_add(clone_duration);
            drop(keyspaces);
            self.db.record_ram_lookup_timing(
                lock_wait,
                held_duration,
                lookup_duration,
                clone_duration,
                0,
            );
            return Ok(value);
        }
        let key_started = Instant::now();
        let physical = physical_key_from_namespace(&self.namespace, key);
        let key_duration = elapsed_nanos(key_started.elapsed());
        drop(inner);
        let result = self.db.get_durable_value(&physical);
        self.db.update_ram_diagnostics(|diagnostics| {
            diagnostics.key_encode_duration_ns = diagnostics
                .key_encode_duration_ns
                .saturating_add(key_duration);
        });
        result
    }

    /// Insert or replace a value durably.
    pub fn insert(&self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> Result<()> {
        self.db.batch().put(self, key, value).commit()
    }

    /// Delete a value durably.
    pub fn remove(&self, key: impl AsRef<[u8]>) -> Result<()> {
        self.db.batch().delete(self, key).commit()
    }

    /// Iterate all entries in key order.
    pub fn iter(&self) -> Iter {
        self.iter_bounds(None, None, None)
    }

    /// Iterate entries whose user keys start with `prefix`.
    pub fn prefix(&self, prefix: impl AsRef<[u8]>) -> Iter {
        self.iter_bounds(Some(prefix.as_ref()), None, None)
    }

    /// Return the first entry whose user key starts with `prefix`.
    pub fn first_prefix(&self, prefix: impl AsRef<[u8]>) -> Result<Option<Entry>> {
        self.first_prefix_after(prefix, None)
    }

    /// Return the first entry whose user key starts with `prefix` and is after
    /// `after`, when supplied.
    pub fn first_prefix_after(
        &self,
        prefix: impl AsRef<[u8]>,
        after: Option<&[u8]>,
    ) -> Result<Option<Entry>> {
        let prefix = prefix.as_ref().to_vec();
        let start = after.map(|key| (key.to_vec(), false));
        self.iter_bounds_limited(Some(&prefix), start, None, Some(1))
            .into_result()
            .map(|entries| entries.into_iter().next())
    }

    /// Iterate entries within a user-key range.
    pub fn range<K, R>(&self, range: R) -> Iter
    where
        K: AsRef<[u8]>,
        R: RangeBounds<K>,
    {
        let start = match range.start_bound() {
            Bound::Included(value) => Some((value.as_ref().to_vec(), true)),
            Bound::Excluded(value) => Some((value.as_ref().to_vec(), false)),
            Bound::Unbounded => None,
        };
        let end = match range.end_bound() {
            Bound::Included(value) => Some((value.as_ref().to_vec(), true)),
            Bound::Excluded(value) => Some((value.as_ref().to_vec(), false)),
            Bound::Unbounded => None,
        };
        self.range_bounds(
            start
                .as_ref()
                .map(|(key, inclusive)| (key.as_slice(), *inclusive)),
            end.as_ref()
                .map(|(key, inclusive)| (key.as_slice(), *inclusive)),
        )
    }

    /// Iterate within optional borrowed user-key bounds without scanning
    /// entries outside the requested range.
    pub fn range_bounds(&self, start: Option<(&[u8], bool)>, end: Option<(&[u8], bool)>) -> Iter {
        self.iter_bounds(
            None,
            start.map(|(key, inclusive)| (key.to_vec(), inclusive)),
            end.map(|(key, inclusive)| (key.to_vec(), inclusive)),
        )
    }

    #[allow(clippy::too_many_lines)]
    fn iter_bounds(
        &self,
        prefix: Option<&[u8]>,
        start: Option<(Vec<u8>, bool)>,
        end: Option<(Vec<u8>, bool)>,
    ) -> Iter {
        self.iter_bounds_limited(prefix, start, end, None)
    }

    #[allow(clippy::too_many_lines)]
    fn iter_bounds_limited(
        &self,
        prefix: Option<&[u8]>,
        start: Option<(Vec<u8>, bool)>,
        end: Option<(Vec<u8>, bool)>,
        limit: Option<usize>,
    ) -> Iter {
        #[cfg(feature = "benchmark")]
        if let Some(rcu) = &self.db.ram_rcu_keyspaces {
            return iter_memory_rcu_bounds(
                &self.db,
                rcu.load_full(),
                &self.name,
                prefix,
                start,
                end,
                limit,
            );
        }
        if self.db.ram_keyspaces.is_some() {
            let ram_keyspaces = self
                .db
                .ram_keyspaces
                .as_ref()
                .ok_or_else(|| Error::message("ThingDB RAM state is unavailable"));
            return match ram_keyspaces {
                Ok(ram_keyspaces) => iter_memory_bounds(
                    &self.db,
                    ram_keyspaces,
                    &self.name,
                    prefix,
                    start,
                    end,
                    limit,
                ),
                Err(error) => Iter {
                    entries: Vec::new().into_iter(),
                    error: Some(error),
                },
            };
        }
        let scan_lock_started = Instant::now();
        let Ok(inner) = self.db.inner.lock() else {
            return Iter {
                entries: Vec::new().into_iter(),
                error: None,
            };
        };
        let scan_lock_wait = elapsed_nanos(scan_lock_started.elapsed());
        if inner.in_memory {
            drop(inner);
            let ram_keyspaces = self
                .db
                .ram_keyspaces
                .as_ref()
                .ok_or_else(|| Error::message("ThingDB RAM state is unavailable"));
            return match ram_keyspaces {
                Ok(ram_keyspaces) => iter_memory_bounds(
                    &self.db,
                    ram_keyspaces,
                    &self.name,
                    prefix,
                    start,
                    end,
                    limit,
                ),
                Err(error) => Iter {
                    entries: Vec::new().into_iter(),
                    error: Some(error),
                },
            };
        }
        if limit == Some(1) {
            drop(inner);
            return self.iter_durable_first(prefix, start, end);
        }
        let state = inner.state.clone();
        let pending_table = inner.pending_table.clone();
        let table_layers = inner.table_layers.clone();
        let scan_lock_held =
            elapsed_nanos(scan_lock_started.elapsed()).saturating_sub(scan_lock_wait);
        drop(inner);
        let namespace = self.namespace.clone();
        let scan_started = Instant::now();
        let cursor_started = Instant::now();
        let lower = start
            .as_ref()
            .map(|(key, _)| physical_key_from_namespace(&namespace, key))
            .unwrap_or_else(|| namespace.to_vec());
        let upper = end
            .as_ref()
            .map(|(key, _)| physical_key_from_namespace(&namespace, key))
            .and_then(|key| successor(&key))
            .or_else(|| successor(&namespace));
        let cursor_after = start.as_ref().and_then(|(key, inclusive)| {
            (!inclusive).then(|| physical_key_from_namespace(&namespace, key))
        });
        let layer_entries = match table_layers
            .iter()
            .map(TableLayer::loaded_entries)
            .collect::<Result<Vec<_>>>()
        {
            Ok(entries) => entries,
            Err(error) => {
                return Iter {
                    entries: Vec::new().into_iter(),
                    error: Some(error),
                };
            },
        };
        let mut entries = Vec::new();
        let mut scan_error = None;
        let mut scan_keys_examined = 0u64;
        let mut scan_layers_consulted = 0u64;
        let mut table_bytes_read = 0u64;
        let mut table_read_duration_ns = 0u64;
        let mut state_key = next_map_key(&state, &lower, upper.as_deref(), cursor_after.as_deref());
        let mut pending_key = next_map_key(
            &pending_table,
            &lower,
            upper.as_deref(),
            cursor_after.as_deref(),
        );
        let mut layer_indices = layer_entries
            .iter()
            .map(|entries| {
                let index = entries
                    .binary_search_by(|entry| entry.key.as_slice().cmp(lower.as_slice()))
                    .unwrap_or_else(|index| index);
                if cursor_after.is_some()
                    && entries.get(index).is_some_and(|entry| {
                        cursor_after
                            .as_deref()
                            .is_some_and(|after| entry.key == after)
                    })
                {
                    index + 1
                } else {
                    index
                }
            })
            .collect::<Vec<_>>();
        let cursor_duration = elapsed_nanos(cursor_started.elapsed());
        let merge_started = Instant::now();

        loop {
            let mut next_key = state_key.clone();
            if pending_key.as_deref().is_some_and(|candidate| {
                next_key
                    .as_deref()
                    .is_none_or(|current| candidate < current)
            }) {
                next_key.clone_from(&pending_key);
            }
            for (entries, index) in layer_entries.iter().zip(&layer_indices) {
                if let Some(candidate) = entries.get(*index).map(|entry| &entry.key)
                    && next_key
                        .as_deref()
                        .is_none_or(|current| candidate.as_slice() < current)
                {
                    next_key = Some(candidate.clone());
                }
            }
            let Some(key) = next_key else {
                break;
            };
            if upper.as_ref().is_some_and(|upper| key >= *upper) {
                break;
            }
            if state_key.as_deref() == Some(key.as_slice()) {
                state_key = next_map_key(&state, &lower, upper.as_deref(), Some(&key));
            }
            if pending_key.as_deref() == Some(key.as_slice()) {
                pending_key = next_map_key(&pending_table, &lower, upper.as_deref(), Some(&key));
            }
            for (entries, index) in layer_entries.iter().zip(&mut layer_indices) {
                if entries.get(*index).is_some_and(|entry| entry.key == key) {
                    *index += 1;
                }
            }
            scan_keys_examined = scan_keys_examined.saturating_add(1);
            let value = match get_scan_value_snapshot(&state, &pending_table, &table_layers, &key) {
                Ok((value, layers, bytes, duration)) => {
                    scan_layers_consulted = scan_layers_consulted.saturating_add(layers);
                    table_bytes_read = table_bytes_read.saturating_add(bytes);
                    table_read_duration_ns = table_read_duration_ns.saturating_add(duration);
                    value
                },
                Err(error) => {
                    scan_error = Some(error);
                    break;
                },
            };
            let Some(value) = value else {
                continue;
            };
            let Some(user_key) = key.strip_prefix(namespace.as_ref()) else {
                continue;
            };
            if let Some(prefix) = prefix
                && !user_key.starts_with(prefix)
            {
                continue;
            }
            if let Some((start, inclusive)) = &start {
                let matches = if *inclusive {
                    user_key >= start.as_slice()
                } else {
                    user_key > start.as_slice()
                };
                if !matches {
                    continue;
                }
            }
            if let Some((end, inclusive)) = &end {
                let matches = if *inclusive {
                    user_key <= end.as_slice()
                } else {
                    user_key < end.as_slice()
                };
                if !matches {
                    continue;
                }
            }
            entries.push(Entry {
                key: user_key.to_vec(),
                value,
            });
            if limit.is_some_and(|limit| entries.len() >= limit) {
                break;
            }
        }
        let Ok(mut inner) = self.db.inner.lock() else {
            return Iter {
                entries: entries.into_iter(),
                error: Some(Error::message("database lock poisoned")),
            };
        };
        inner.diagnostics.scan_keys_examined = inner
            .diagnostics
            .scan_keys_examined
            .saturating_add(scan_keys_examined);
        inner.diagnostics.scan_layers_consulted = inner
            .diagnostics
            .scan_layers_consulted
            .saturating_add(scan_layers_consulted);
        inner.diagnostics.scan_lock_wait_duration_ns = inner
            .diagnostics
            .scan_lock_wait_duration_ns
            .saturating_add(scan_lock_wait);
        inner.diagnostics.scan_lock_held_duration_ns = inner
            .diagnostics
            .scan_lock_held_duration_ns
            .saturating_add(scan_lock_held);
        inner.diagnostics.scan_cursor_init_duration_ns = inner
            .diagnostics
            .scan_cursor_init_duration_ns
            .saturating_add(cursor_duration);
        inner.diagnostics.scan_merge_duration_ns = inner
            .diagnostics
            .scan_merge_duration_ns
            .saturating_add(elapsed_nanos(merge_started.elapsed()));
        inner.diagnostics.scan_returned_entries = inner
            .diagnostics
            .scan_returned_entries
            .saturating_add(entries.len() as u64);
        if scan_error.is_some() {
            inner.diagnostics.scan_error_count =
                inner.diagnostics.scan_error_count.saturating_add(1);
        }
        inner.diagnostics.table_bytes_read = inner
            .diagnostics
            .table_bytes_read
            .saturating_add(table_bytes_read);
        inner.diagnostics.table_read_duration_ns = inner
            .diagnostics
            .table_read_duration_ns
            .saturating_add(table_read_duration_ns);
        inner.diagnostics.scan_count = inner.diagnostics.scan_count.saturating_add(1);
        inner.diagnostics.scan_duration_ns = inner
            .diagnostics
            .scan_duration_ns
            .saturating_add(elapsed_nanos(scan_started.elapsed()));
        Iter {
            entries: entries.into_iter(),
            error: scan_error,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn iter_durable_first(
        &self,
        prefix: Option<&[u8]>,
        start: Option<(Vec<u8>, bool)>,
        end: Option<(Vec<u8>, bool)>,
    ) -> Iter {
        let namespace = self.namespace.clone();
        let mut after = start.as_ref().and_then(|(key, inclusive)| {
            (!inclusive).then(|| physical_key_from_namespace(&namespace, key))
        });
        let mut retries = 0;
        loop {
            let lock_started = Instant::now();
            let snapshot = match self.db.inner.lock() {
                Ok(inner) if !inner.in_memory => {
                    let lock_wait = elapsed_nanos(lock_started.elapsed());
                    let lower = durable_scan_lower(&namespace, prefix, start.as_ref());
                    let upper = durable_scan_upper(&namespace, prefix, end.as_ref());
                    let state_candidate =
                        next_map_key(&inner.state, &lower, upper.as_deref(), after.as_deref())
                            .and_then(|key| {
                                inner.state.get(&key).map(|value| (key, value.clone()))
                            });
                    let pending_candidate = next_map_key(
                        &inner.pending_table,
                        &lower,
                        upper.as_deref(),
                        after.as_deref(),
                    )
                    .and_then(|key| {
                        inner
                            .pending_table
                            .get(&key)
                            .map(|value| (key, value.clone()))
                    });
                    let layers = inner.table_layers.clone();
                    let generation = inner.state_generation;
                    let lock_held = elapsed_nanos(lock_started.elapsed());
                    (
                        lower,
                        upper,
                        state_candidate,
                        pending_candidate,
                        layers,
                        generation,
                        lock_wait,
                        lock_held,
                    )
                },
                Ok(_) => {
                    return Iter {
                        entries: Vec::new().into_iter(),
                        error: Some(Error::message("ThingDB durable scan used for RAM state")),
                    };
                },
                Err(_) => {
                    return Iter {
                        entries: Vec::new().into_iter(),
                        error: Some(Error::message("database lock poisoned")),
                    };
                },
            };
            let (
                lower,
                upper,
                state_candidate,
                pending_candidate,
                layers,
                generation,
                lock_wait,
                lock_held,
            ) = snapshot;
            let table_candidates = layers
                .iter()
                .map(|layer| {
                    let entries = layer.loaded_entries()?;
                    let seek = after
                        .as_deref()
                        .filter(|key| *key > lower.as_slice())
                        .unwrap_or(&lower);
                    let index = entries
                        .binary_search_by(|entry| entry.key.as_slice().cmp(seek))
                        .unwrap_or_else(|index| index);
                    let index = if after.as_deref().is_some_and(|after| {
                        entries
                            .get(index)
                            .is_some_and(|entry| entry.key.as_slice() == after)
                    }) {
                        index + 1
                    } else {
                        index
                    };
                    Ok(entries
                        .get(index)
                        .filter(|entry| {
                            upper
                                .as_deref()
                                .is_none_or(|bound| entry.key.as_slice() < bound)
                        })
                        .map(|entry| entry.key.clone()))
                })
                .collect::<Result<Vec<_>>>();
            let table_candidates = match table_candidates {
                Ok(candidates) => candidates,
                Err(error) => {
                    return Iter {
                        entries: Vec::new().into_iter(),
                        error: Some(error),
                    };
                },
            };
            let mut key = state_candidate.as_ref().map(|(key, _)| key.clone());
            if pending_candidate.as_ref().is_some_and(|(candidate, _)| {
                key.as_deref()
                    .is_none_or(|current| candidate.as_slice() < current)
            }) {
                key = pending_candidate.as_ref().map(|(key, _)| key.clone());
            }
            for candidate in table_candidates.iter().flatten() {
                if key
                    .as_deref()
                    .is_none_or(|current| candidate.as_slice() < current)
                {
                    key = Some(candidate.clone());
                }
            }
            let Some(key) = key else {
                return self.finish_durable_first_diagnostics(
                    lock_wait,
                    lock_held,
                    0,
                    0,
                    Vec::new(),
                );
            };
            let value = if pending_candidate
                .as_ref()
                .is_some_and(|(candidate, _)| candidate == &key)
            {
                pending_candidate.and_then(|(_, value)| value)
            } else if state_candidate
                .as_ref()
                .is_some_and(|(candidate, _)| candidate == &key)
            {
                state_candidate.map(|(_, value)| value)
            } else {
                match get_scan_value_snapshot(&BTreeMap::new(), &BTreeMap::new(), &layers, &key) {
                    Ok((value, _, _, _)) => value,
                    Err(error) => {
                        return Iter {
                            entries: Vec::new().into_iter(),
                            error: Some(error),
                        };
                    },
                }
            };
            let generation_unchanged = self
                .db
                .inner
                .lock()
                .map(|inner| inner.state_generation == generation)
                .unwrap_or(false);
            if !generation_unchanged {
                retries += 1;
                if retries >= 8 {
                    return Iter {
                        entries: Vec::new().into_iter(),
                        error: Some(Error::message(
                            "ThingDB durable scan changed during point lookup",
                        )),
                    };
                }
                continue;
            }
            after = Some(key.clone());
            let Some(value) = value else {
                continue;
            };
            let Some(user_key) = key.strip_prefix(namespace.as_ref()) else {
                return self.finish_durable_first_diagnostics(
                    lock_wait,
                    lock_held,
                    layers.len() as u64,
                    0,
                    Vec::new(),
                );
            };
            return self.finish_durable_first_diagnostics(
                lock_wait,
                lock_held,
                layers.len() as u64,
                1,
                vec![Entry {
                    key: user_key.to_vec(),
                    value,
                }],
            );
        }
    }

    fn finish_durable_first_diagnostics(
        &self,
        lock_wait: u64,
        lock_held: u64,
        layers: u64,
        returned: u64,
        entries: Vec<Entry>,
    ) -> Iter {
        if let Ok(mut inner) = self.db.inner.lock() {
            inner.diagnostics.scan_count = inner.diagnostics.scan_count.saturating_add(1);
            inner.diagnostics.scan_lock_wait_duration_ns = inner
                .diagnostics
                .scan_lock_wait_duration_ns
                .saturating_add(lock_wait);
            inner.diagnostics.scan_lock_held_duration_ns = inner
                .diagnostics
                .scan_lock_held_duration_ns
                .saturating_add(lock_held);
            inner.diagnostics.scan_layers_consulted = inner
                .diagnostics
                .scan_layers_consulted
                .saturating_add(layers);
            inner.diagnostics.scan_returned_entries = inner
                .diagnostics
                .scan_returned_entries
                .saturating_add(returned);
        }
        Iter {
            entries: entries.into_iter(),
            error: None,
        }
    }
}

fn get_scan_value_snapshot(
    state: &BTreeMap<Vec<u8>, Vec<u8>>,
    pending_table: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    table_layers: &[TableLayer],
    key: &[u8],
) -> Result<(Option<Vec<u8>>, u64, u64, u64)> {
    if let Some(value) = pending_table.get(key) {
        return Ok((value.clone(), 0, 0, 0));
    }
    if let Some(value) = state.get(key) {
        return Ok((Some(value.clone()), 0, 0, 0));
    }
    let mut layers_consulted = 0;
    for layer in table_layers.iter().rev() {
        layers_consulted += 1;
        let entries = layer.loaded_entries()?;
        let candidate_range = layer.candidate_entry_range(key);
        let Ok(relative_index) = entries[candidate_range.clone()]
            .binary_search_by(|entry| entry.key.as_slice().cmp(key))
        else {
            continue;
        };
        let index = candidate_range.start + relative_index;
        return read_table_value_handle(&layer.file, &entries[index], layer.is_v2).map(
            |(value, _, duration)| (value, layers_consulted, entries[index].length, duration),
        );
    }
    Ok((None, layers_consulted, 0, 0))
}

fn next_map_key<V>(
    map: &BTreeMap<Vec<u8>, V>,
    lower: &[u8],
    upper: Option<&[u8]>,
    after: Option<&[u8]>,
) -> Option<Vec<u8>> {
    let start = after.map_or_else(
        || Bound::Included(lower.to_vec()),
        |key| Bound::Excluded(key.to_vec()),
    );
    let end = upper.map_or(Bound::Unbounded, |key| Bound::Excluded(key.to_vec()));
    map.range((start, end)).next().map(|(key, _)| key.clone())
}

fn durable_scan_lower(
    namespace: &[u8],
    prefix: Option<&[u8]>,
    start: Option<&(Vec<u8>, bool)>,
) -> Vec<u8> {
    let user_key = match (prefix, start) {
        (Some(prefix), Some((start, _))) if start.as_slice() >= prefix => start.as_slice(),
        (Some(prefix), _) => prefix,
        (None, Some((start, _))) => start.as_slice(),
        (None, None) => &[],
    };
    physical_key_from_namespace(namespace, user_key)
}

#[allow(clippy::tuple_array_conversions)]
fn durable_scan_upper(
    namespace: &[u8],
    prefix: Option<&[u8]>,
    end: Option<&(Vec<u8>, bool)>,
) -> Option<Vec<u8>> {
    let namespace_end = successor(namespace);
    let prefix_end = prefix
        .and_then(successor)
        .map(|prefix| physical_key_from_namespace(namespace, &prefix));
    let explicit_end = end.and_then(|(end, inclusive)| {
        let physical = physical_key_from_namespace(namespace, end);
        if *inclusive {
            successor(&physical)
        } else {
            Some(physical)
        }
    });
    match (namespace_end, prefix_end, explicit_end) {
        (namespace_end, None, None) => namespace_end,
        (namespace_end, prefix_end, explicit_end) => [namespace_end, prefix_end, explicit_end]
            .into_iter()
            .flatten()
            .min(),
    }
}

fn iter_memory_bounds(
    db: &Database,
    ram_keyspaces: &Arc<RwLock<HashMap<Arc<str>, BTreeMap<Vec<u8>, Arc<Vec<u8>>>>>>,
    name: &str,
    prefix: Option<&[u8]>,
    start: Option<(Vec<u8>, bool)>,
    end: Option<(Vec<u8>, bool)>,
    limit: Option<usize>,
) -> Iter {
    let iteration_started = Instant::now();
    let mut entries = Vec::new();
    let mut entries_examined = 0u64;
    let mut entries_returned = 0u64;
    if let Ok(keyspaces) = ram_keyspaces.read()
        && let Some(keyspace) = keyspaces.get(name)
    {
        let lower = match (start.as_ref(), prefix) {
            (Some((start, inclusive)), Some(prefix)) if start.as_slice() >= prefix => {
                if *inclusive {
                    Bound::Included(start.as_slice())
                } else {
                    Bound::Excluded(start.as_slice())
                }
            },
            (_, Some(prefix)) => Bound::Included(prefix),
            (Some((start, inclusive)), None) => {
                if *inclusive {
                    Bound::Included(start.as_slice())
                } else {
                    Bound::Excluded(start.as_slice())
                }
            },
            (None, None) => Bound::Unbounded,
        };
        let prefix_upper = prefix.and_then(successor);
        let upper = match (end.as_ref(), prefix_upper.as_deref()) {
            (Some((end, inclusive)), Some(prefix_upper)) if end.as_slice() <= prefix_upper => {
                if *inclusive {
                    Bound::Included(end.as_slice())
                } else {
                    Bound::Excluded(end.as_slice())
                }
            },
            (Some((end, inclusive)), None) => {
                if *inclusive {
                    Bound::Included(end.as_slice())
                } else {
                    Bound::Excluded(end.as_slice())
                }
            },
            (_, Some(prefix_upper)) => Bound::Excluded(prefix_upper),
            (None, None) => Bound::Unbounded,
        };
        for (key, value) in keyspace.range::<[u8], _>((lower, upper)) {
            entries_examined = entries_examined.saturating_add(1);
            if let Some(prefix) = prefix
                && !key.starts_with(prefix)
            {
                break;
            }
            entries.push(Entry {
                key: key.clone(),
                value: value.as_ref().clone(),
            });
            entries_returned = entries_returned.saturating_add(1);
            if limit.is_some_and(|limit| entries.len() >= limit) {
                break;
            }
        }
    }
    let elapsed = elapsed_nanos(iteration_started.elapsed());
    db.update_ram_diagnostics(|diagnostics| {
        diagnostics.iteration_count = diagnostics.iteration_count.saturating_add(1);
        diagnostics.iteration_duration_ns =
            diagnostics.iteration_duration_ns.saturating_add(elapsed);
        diagnostics.iteration_entries_examined = diagnostics
            .iteration_entries_examined
            .saturating_add(entries_examined);
        diagnostics.iteration_entries_returned = diagnostics
            .iteration_entries_returned
            .saturating_add(entries_returned);
    });
    Iter {
        entries: entries.into_iter(),
        error: None,
    }
}

#[cfg(feature = "benchmark")]
fn iter_memory_rcu_bounds(
    db: &Database,
    generation: Arc<RamGeneration>,
    name: &str,
    prefix: Option<&[u8]>,
    start: Option<(Vec<u8>, bool)>,
    end: Option<(Vec<u8>, bool)>,
    limit: Option<usize>,
) -> Iter {
    let started = Instant::now();
    let mut entries = Vec::new();
    let mut examined = 0u64;
    if let Some(keyspace) = generation.get(name) {
        let lower = match (start.as_ref(), prefix) {
            (Some((key, inclusive)), Some(prefix)) if key.as_slice() >= prefix => {
                if *inclusive {
                    Bound::Included(key.as_slice())
                } else {
                    Bound::Excluded(key.as_slice())
                }
            },
            (_, Some(prefix)) => Bound::Included(prefix),
            (Some((key, inclusive)), None) => {
                if *inclusive {
                    Bound::Included(key.as_slice())
                } else {
                    Bound::Excluded(key.as_slice())
                }
            },
            (None, None) => Bound::Unbounded,
        };
        let prefix_upper = prefix.and_then(successor);
        let upper = match (end.as_ref(), prefix_upper.as_deref()) {
            (Some((key, inclusive)), Some(prefix_upper)) if key.as_slice() <= prefix_upper => {
                if *inclusive {
                    Bound::Included(key.as_slice())
                } else {
                    Bound::Excluded(key.as_slice())
                }
            },
            (Some((key, inclusive)), None) => {
                if *inclusive {
                    Bound::Included(key.as_slice())
                } else {
                    Bound::Excluded(key.as_slice())
                }
            },
            (_, Some(prefix_upper)) => Bound::Excluded(prefix_upper),
            (None, None) => Bound::Unbounded,
        };
        for (key, value) in keyspace.range::<[u8], _>((lower, upper)) {
            examined = examined.saturating_add(1);
            if prefix.is_some_and(|prefix| !key.starts_with(prefix)) {
                break;
            }
            entries.push(Entry {
                key: key.clone(),
                value: value.as_ref().clone(),
            });
            if limit.is_some_and(|limit| entries.len() >= limit) {
                break;
            }
        }
    }
    let elapsed = elapsed_nanos(started.elapsed());
    db.update_ram_diagnostics(|diagnostics| {
        diagnostics.iteration_count = diagnostics.iteration_count.saturating_add(1);
        diagnostics.iteration_duration_ns =
            diagnostics.iteration_duration_ns.saturating_add(elapsed);
        diagnostics.iteration_entries_examined = diagnostics
            .iteration_entries_examined
            .saturating_add(examined);
        diagnostics.iteration_entries_returned = diagnostics
            .iteration_entries_returned
            .saturating_add(entries.len() as u64);
    });
    Iter {
        entries: entries.into_iter(),
        error: None,
    }
}

impl Batch {
    /// Add an insertion to the batch.
    pub fn put(
        mut self,
        keyspace: &Keyspace,
        key: impl AsRef<[u8]>,
        value: impl AsRef<[u8]>,
    ) -> Self {
        self.operations.push(BatchOperation {
            keyspace: Arc::clone(&keyspace.name),
            key: self.batch_key(keyspace, key.as_ref()),
            value: Some(value.as_ref().to_vec()),
        });
        self
    }

    /// Add an insertion using owned key and value buffers.
    ///
    /// This is equivalent to [`Batch::put`] but avoids copying the supplied
    /// value before the operation is committed. The key is still prefixed
    /// with the keyspace namespace so keyspace isolation is unchanged.
    pub fn put_owned(mut self, keyspace: &Keyspace, key: Vec<u8>, value: Vec<u8>) -> Self {
        self.operations.push(BatchOperation {
            keyspace: Arc::clone(&keyspace.name),
            key: self.batch_key(keyspace, &key),
            value: Some(value),
        });
        self
    }

    /// Add a deletion to the batch.
    pub fn delete(mut self, keyspace: &Keyspace, key: impl AsRef<[u8]>) -> Self {
        self.operations.push(BatchOperation {
            keyspace: Arc::clone(&keyspace.name),
            key: self.batch_key(keyspace, key.as_ref()),
            value: None,
        });
        self
    }

    /// Add a deletion using an owned key buffer.
    ///
    /// This is equivalent to [`Batch::delete`] but avoids copying the supplied
    /// key before the namespace prefix is applied.
    pub fn delete_owned(mut self, keyspace: &Keyspace, key: Vec<u8>) -> Self {
        self.operations.push(BatchOperation {
            keyspace: Arc::clone(&keyspace.name),
            key: self.batch_key(keyspace, &key),
            value: None,
        });
        self
    }

    /// Commit all operations atomically and durably.
    pub fn commit(self) -> Result<()> {
        self.db.commit_batch(self.operations)
    }

    fn batch_key(&self, keyspace: &Keyspace, key: &[u8]) -> Vec<u8> {
        if self.memory_mode {
            key.to_vec()
        } else {
            physical_key_from_namespace(&keyspace.namespace, key)
        }
    }
}

impl Iterator for Iter {
    type Item = Entry;

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.next()
    }
}

impl Snapshot {
    /// Read all entries in a snapshot keyspace.
    pub fn keyspace(&self, name: &str) -> SnapshotKeyspace {
        SnapshotKeyspace {
            state: Arc::clone(&self.state),
            name: name.to_string(),
        }
    }
}

/// A keyspace view over a consistent snapshot.
pub struct SnapshotKeyspace {
    state: Arc<BTreeMap<Vec<u8>, Vec<u8>>>,
    name: String,
}

impl SnapshotKeyspace {
    /// Read a value from the snapshot.
    pub fn get(&self, key: impl AsRef<[u8]>) -> Option<Vec<u8>> {
        self.state
            .get(&physical_key(&self.name, key.as_ref()))
            .cloned()
    }
}

fn namespace(name: &str) -> Vec<u8> {
    let mut bytes = name.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

fn physical_key(name: &str, key: &[u8]) -> Vec<u8> {
    physical_key_from_namespace(&namespace(name), key)
}

fn physical_key_from_namespace(namespace: &[u8], key: &[u8]) -> Vec<u8> {
    let mut physical = namespace.to_vec();
    physical.extend_from_slice(key);
    physical
}

fn successor(key: &[u8]) -> Option<Vec<u8>> {
    let mut result = key.to_vec();
    for index in (0..result.len()).rev() {
        if result[index] != u8::MAX {
            result[index] += 1;
            result.truncate(index + 1);
            return Some(result);
        }
    }
    None
}

fn encoded_frame_capacity(operations: &[Operation]) -> usize {
    WAL_MAGIC
        .len()
        .saturating_add(8)
        .saturating_add(8)
        .saturating_add(4)
        .saturating_add(
            operations
                .iter()
                .map(|operation| match operation {
                    Operation::Put { key, value } => 1usize
                        .saturating_add(8)
                        .saturating_add(key.len())
                        .saturating_add(8)
                        .saturating_add(value.len()),
                    Operation::Delete { key } => 1usize
                        .saturating_add(8)
                        .saturating_add(key.len())
                        .saturating_add(8),
                })
                .sum(),
        )
        .saturating_add(4)
}

#[cfg(test)]
fn encode_frame(sequence: u64, operations: &[Operation]) -> Result<Vec<u8>> {
    let mut frame = Vec::with_capacity(encoded_frame_capacity(operations));
    encode_frame_into(&mut frame, sequence, operations)?;
    Ok(frame)
}

fn encode_frame_into(output: &mut Vec<u8>, sequence: u64, operations: &[Operation]) -> Result<()> {
    output.extend_from_slice(WAL_MAGIC);
    let length_offset = output.len();
    write_u64(output, 0);
    let body_start = output.len();
    write_u64(output, sequence);
    write_u32(
        output,
        operations
            .len()
            .try_into()
            .map_err(|_| Error::message("too many operations in batch"))?,
    );
    for operation in operations {
        match operation {
            Operation::Put { key, value } => {
                output.push(1);
                write_bytes(output, key)?;
                write_bytes(output, value)?;
            },
            Operation::Delete { key } => {
                output.push(2);
                write_bytes(output, key)?;
                write_u64(output, 0);
            },
        }
    }
    let checksum_offset = output.len();
    let frame_checksum = checksum(&output[body_start..checksum_offset]);
    write_u32(output, frame_checksum);
    let frame_len: u64 = output
        .len()
        .saturating_sub(body_start)
        .try_into()
        .ok()
        .ok_or_else(|| Error::message("WAL frame is too large"))?;
    output[length_offset..length_offset + 8].copy_from_slice(&frame_len.to_be_bytes());
    Ok(())
}

fn replay_wal(
    wal: &mut File,
    table_sequence: u64,
    state: &mut BTreeMap<Vec<u8>, Vec<u8>>,
) -> Result<(u64, u64, u64)> {
    wal.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    wal.read_to_end(&mut bytes)?;
    let mut cursor = 0usize;
    let mut last_sequence = table_sequence;
    let mut frame_count = 0;
    while cursor < bytes.len() {
        let frame_start = cursor;
        if bytes.len() - cursor < WAL_MAGIC.len() + 8 {
            return Ok((last_sequence, frame_start as u64, frame_count));
        }
        if &bytes[cursor..cursor + WAL_MAGIC.len()] != WAL_MAGIC {
            return Err(Error::message("invalid ThingDB WAL magic"));
        }
        cursor += WAL_MAGIC.len();
        let frame_len = read_u64(&bytes, &mut cursor)? as usize;
        if frame_len < 12 {
            return Err(Error::message("invalid ThingDB WAL frame length"));
        }
        if frame_len > bytes.len().saturating_sub(cursor) {
            return Ok((last_sequence, frame_start as u64, frame_count));
        }
        let frame_end = cursor + frame_len;
        let sequence = read_u64(&bytes, &mut cursor)?;
        let count = read_u32(&bytes, &mut cursor)? as usize;
        let mut operations = Vec::with_capacity(count);
        for _ in 0..count {
            let kind = read_byte(&bytes, &mut cursor)?;
            let key = read_bytes(&bytes, &mut cursor)?;
            let value = read_bytes(&bytes, &mut cursor)?;
            operations.push(match kind {
                1 => Operation::Put { key, value },
                2 => Operation::Delete { key },
                _ => return Err(Error::message("invalid ThingDB WAL operation")),
            });
        }
        let checksum_offset = frame_end
            .checked_sub(4)
            .ok_or_else(|| Error::message("invalid ThingDB WAL frame"))?;
        if cursor != checksum_offset {
            return Err(Error::message("invalid ThingDB WAL payload length"));
        }
        let stored_checksum = u32::from_be_bytes(
            bytes[checksum_offset..frame_end]
                .try_into()
                .map_err(|_| Error::message("invalid ThingDB WAL checksum"))?,
        );
        let actual_checksum = checksum(&bytes[frame_start + WAL_MAGIC.len() + 8..checksum_offset]);
        if stored_checksum != actual_checksum {
            return Err(Error::message("ThingDB WAL checksum mismatch"));
        }
        if sequence > last_sequence {
            for operation in operations {
                apply_operation(state, operation);
            }
            last_sequence = sequence;
        }
        frame_count += 1;
        cursor = frame_end;
    }
    Ok((last_sequence, cursor as u64, frame_count))
}

fn elapsed_nanos(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn apply_operation(state: &mut BTreeMap<Vec<u8>, Vec<u8>>, operation: Operation) {
    match operation {
        Operation::Put { key, value } => {
            state.insert(key, value);
        },
        Operation::Delete { key } => {
            state.remove(&key);
        },
    }
}

fn apply_memory_operation(
    keyspaces: &mut HashMap<Arc<str>, BTreeMap<Vec<u8>, Arc<Vec<u8>>>>,
    operation: Operation,
) -> Result<()> {
    let (key, value) = match operation {
        Operation::Put { key, value } => (key, Some(value)),
        Operation::Delete { key } => (key, None),
    };
    let separator = key
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| Error::message("invalid ThingDB in-memory key"))?;
    let name = std::str::from_utf8(&key[..separator])
        .map_err(|_| Error::message("invalid ThingDB in-memory keyspace"))?;
    let user_key = &key[separator + 1..];
    let entries = keyspaces.entry(Arc::from(name)).or_default();
    match value {
        Some(value) => {
            entries.insert(user_key.to_vec(), Arc::new(value));
        },
        None => {
            entries.remove(user_key);
        },
    }
    Ok(())
}

fn validate_memory_batch_operation(operation: &BatchOperation) -> Result<()> {
    if operation.keyspace.is_empty() || operation.keyspace.as_bytes().contains(&0) {
        return Err(Error::message("invalid ThingDB in-memory keyspace"));
    }
    Ok(())
}

fn apply_memory_batch_operation(
    keyspaces: &mut HashMap<Arc<str>, BTreeMap<Vec<u8>, Arc<Vec<u8>>>>,
    operation: BatchOperation,
) {
    let entries = keyspaces.entry(operation.keyspace.clone()).or_default();
    match operation.value {
        Some(value) => {
            entries.insert(operation.key, Arc::new(value));
        },
        None => {
            entries.remove(&operation.key);
        },
    }
}

fn validate_memory_operation(operation: &Operation) -> Result<()> {
    let key = match operation {
        Operation::Put { key, .. } | Operation::Delete { key } => key,
    };
    let separator = key
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| Error::message("invalid ThingDB in-memory key"))?;
    std::str::from_utf8(&key[..separator])
        .map_err(|_| Error::message("invalid ThingDB in-memory keyspace"))?;
    Ok(())
}

fn apply_operation_with_pending(
    state: &mut BTreeMap<Vec<u8>, Vec<u8>>,
    pending: &mut BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    pending_bytes: &mut u64,
    operation: Operation,
) {
    match operation {
        Operation::Put { key, value } => {
            let key_bytes = key.len() as u64;
            let value_bytes = value.len() as u64;
            state.insert(key.clone(), value.clone());
            if let Some(previous) = pending.insert(key, Some(value)) {
                *pending_bytes = pending_bytes.saturating_sub(
                    key_bytes
                        .saturating_add(1)
                        .saturating_add(previous.as_ref().map_or(0, |value| value.len() as u64)),
                );
            }
            *pending_bytes = pending_bytes
                .saturating_add(key_bytes.saturating_add(1).saturating_add(value_bytes));
        },
        Operation::Delete { key } => {
            let key_bytes = key.len() as u64;
            state.remove(&key);
            if let Some(previous) = pending.insert(key, None) {
                *pending_bytes = pending_bytes.saturating_sub(
                    key_bytes
                        .saturating_add(1)
                        .saturating_add(previous.as_ref().map_or(0, |value| value.len() as u64)),
                );
            }
            *pending_bytes = pending_bytes.saturating_add(key_bytes.saturating_add(1));
        },
    }
}

fn write_table(
    path: &Path,
    sequence: u64,
    entries: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    sync: bool,
) -> Result<()> {
    const TABLE_WRITE_BUFFER_BYTES: usize = 64 * 1024;
    let mut file = File::create(path)?;
    let mut buffer = Vec::with_capacity(TABLE_WRITE_BUFFER_BYTES);
    let mut block_first_key = None;
    let mut block_last_key = None;
    let mut block_offset = (TABLE_MAGIC_V3.len() + 16) as u64;
    let mut blocks = Vec::new();
    let entry_count: u64 = entries
        .len()
        .try_into()
        .map_err(|_| Error::message("too many entries in ThingDB table"))?;
    file.write_all(TABLE_MAGIC_V3)?;
    file.write_all(&sequence.to_be_bytes())?;
    file.write_all(&entry_count.to_be_bytes())?;
    for (key, value) in entries {
        let mut record = Vec::new();
        write_bytes(&mut record, key)?;
        match value {
            Some(value) => {
                record.push(1);
                write_bytes(&mut record, value)?;
            },
            None => record.push(2),
        }
        let record_checksum = checksum(&record);
        write_u32(&mut record, record_checksum);
        if block_first_key.is_none() {
            block_first_key = Some(key.clone());
        }
        block_last_key = Some(key.clone());
        if buffer.len().saturating_add(record.len()) > TABLE_WRITE_BUFFER_BYTES
            && !buffer.is_empty()
        {
            file.write_all(&buffer)?;
            blocks.push((
                block_offset,
                buffer.len() as u64,
                block_first_key
                    .take()
                    .ok_or_else(|| Error::message("ThingDB table block has no first key"))?,
                block_last_key
                    .take()
                    .ok_or_else(|| Error::message("ThingDB table block has no last key"))?,
                checksum(&buffer),
            ));
            block_offset = block_offset.saturating_add(buffer.len() as u64);
            buffer.clear();
        }
        buffer.extend_from_slice(&record);
    }
    if !buffer.is_empty() {
        file.write_all(&buffer)?;
        blocks.push((
            block_offset,
            buffer.len() as u64,
            block_first_key
                .take()
                .ok_or_else(|| Error::message("ThingDB table block has no first key"))?,
            block_last_key
                .take()
                .ok_or_else(|| Error::message("ThingDB table block has no last key"))?,
            checksum(&buffer),
        ));
    }
    let footer_start = block_offset.saturating_add(buffer.len() as u64);
    let mut footer = Vec::new();
    footer.extend_from_slice(TABLE_FOOTER_MAGIC);
    footer.extend_from_slice(&(blocks.len() as u64).to_be_bytes());
    for (offset, length, first_key, last_key, block_checksum) in &blocks {
        footer.extend_from_slice(&offset.to_be_bytes());
        footer.extend_from_slice(&length.to_be_bytes());
        write_bytes(&mut footer, first_key)?;
        write_bytes(&mut footer, last_key)?;
        write_u32(&mut footer, *block_checksum);
    }
    let footer_checksum = checksum(&footer);
    write_u32(&mut footer, footer_checksum);
    debug_assert_eq!(footer_start, file.metadata()?.len());
    file.write_all(&footer)?;
    if sync {
        file.sync_all()?;
    }
    Ok(())
}

fn read_table_index(path: &Path) -> Result<(u64, Vec<TableIndexEntry>, bool, Vec<TableBlock>)> {
    let bytes = fs::read(path)?;
    if bytes.len() < TABLE_MAGIC.len() + 16
        || (&bytes[..TABLE_MAGIC.len()] != TABLE_MAGIC
            && &bytes[..TABLE_MAGIC_V2.len()] != TABLE_MAGIC_V2
            && &bytes[..TABLE_MAGIC_V3.len()] != TABLE_MAGIC_V3)
    {
        return Err(Error::message("invalid ThingDB table"));
    }
    let version = &bytes[..TABLE_MAGIC.len()];
    let is_v2 = version != TABLE_MAGIC;
    let is_v3 = version == TABLE_MAGIC_V3;
    let mut cursor = TABLE_MAGIC.len();
    let sequence = read_u64(&bytes, &mut cursor)?;
    let count = read_u64(&bytes, &mut cursor)? as usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        if is_v3
            && bytes.get(cursor..cursor + TABLE_FOOTER_MAGIC.len())
                == Some(TABLE_FOOTER_MAGIC.as_slice())
        {
            return Err(Error::message(
                "ThingDB table footer appears before all records",
            ));
        }
        let record_start = cursor;
        let key = read_bytes(&bytes, &mut cursor)?;
        let value = if is_v2 {
            match read_byte(&bytes, &mut cursor)? {
                1 => Some(read_bytes(&bytes, &mut cursor)?),
                2 => None,
                _ => return Err(Error::message("invalid ThingDB table operation")),
            }
        } else {
            Some(read_bytes(&bytes, &mut cursor)?)
        };
        let checksum_end = cursor;
        let stored = read_u32(&bytes, &mut cursor)?;
        let actual = checksum(&bytes[record_start..checksum_end]);
        if stored != actual {
            return Err(Error::message("ThingDB table checksum mismatch"));
        }
        entries.push(TableIndexEntry {
            key,
            offset: record_start as u64,
            length: (cursor - record_start) as u64,
        });
        let _ = value;
    }
    if entries
        .windows(2)
        .any(|entries| entries[0].key >= entries[1].key)
    {
        return Err(Error::message(
            "ThingDB table keys are not strictly ordered",
        ));
    }
    if is_v3 {
        let footer_start = bytes
            .get(cursor..cursor + TABLE_FOOTER_MAGIC.len())
            .is_some_and(|magic| magic == TABLE_FOOTER_MAGIC.as_slice())
            .then_some(cursor)
            .ok_or_else(|| Error::message("ThingDB table is missing its validated footer"))?;
        let blocks = validate_table_footer(&bytes, footer_start)?;
        return Ok((sequence, entries, is_v2, blocks));
    }
    if cursor != bytes.len() {
        return Err(Error::message("trailing bytes in ThingDB table"));
    }
    Ok((sequence, entries, is_v2, Vec::new()))
}

fn read_table_metadata(path: &Path) -> Result<(u64, Vec<TableIndexEntry>, bool, Vec<TableBlock>)> {
    let bytes = fs::read(path)?;
    if bytes.len() < TABLE_MAGIC.len() + 16
        || (&bytes[..TABLE_MAGIC.len()] != TABLE_MAGIC
            && &bytes[..TABLE_MAGIC_V2.len()] != TABLE_MAGIC_V2
            && &bytes[..TABLE_MAGIC_V3.len()] != TABLE_MAGIC_V3)
    {
        return Err(Error::message("invalid ThingDB table"));
    }
    let version = &bytes[..TABLE_MAGIC.len()];
    let mut cursor = TABLE_MAGIC.len();
    let sequence = read_u64(&bytes, &mut cursor)?;
    if version != TABLE_MAGIC_V3 {
        let (sequence, entries, is_v2, blocks) = read_table_index(path)?;
        return Ok((sequence, entries, is_v2, blocks));
    }
    // Validate record checksums/order during open while discarding the full
    // index. The durable read path loads the index on first access, keeping
    // the retained open-state bounded by block metadata.
    let (_, _, _, blocks) = read_table_index(path)?;
    Ok((sequence, Vec::new(), true, blocks))
}

fn validate_table_footer(bytes: &[u8], footer_start: usize) -> Result<Vec<TableBlock>> {
    let mut cursor = footer_start;
    if bytes.get(cursor..cursor + TABLE_FOOTER_MAGIC.len()) != Some(TABLE_FOOTER_MAGIC.as_slice()) {
        return Err(Error::message("invalid ThingDB table footer"));
    }
    cursor += TABLE_FOOTER_MAGIC.len();
    let block_count = read_u64(bytes, &mut cursor)? as usize;
    let mut previous_offset = None;
    let mut previous_end = None;
    let data_start = (TABLE_MAGIC_V3.len() + 16) as u64;
    let mut blocks = Vec::with_capacity(block_count);
    for _ in 0..block_count {
        let offset = read_u64(bytes, &mut cursor)?;
        let length = read_u64(bytes, &mut cursor)?;
        let first_key = read_bytes(bytes, &mut cursor)?;
        let last_key = read_bytes(bytes, &mut cursor)?;
        let block_checksum = read_u32(bytes, &mut cursor)?;
        let block_end = offset
            .checked_add(length)
            .ok_or_else(|| Error::message("ThingDB table block range overflow"))?;
        let footer_start_u64 = footer_start as u64;
        if offset < data_start
            || block_end > footer_start_u64
            || previous_offset.is_some_and(|previous| offset <= previous)
            || previous_end.is_some_and(|previous| offset != previous)
            || first_key > last_key
        {
            return Err(Error::message("invalid ThingDB table block metadata"));
        }
        let start: usize = offset
            .try_into()
            .map_err(|_| Error::message("ThingDB table block offset is too large"))?;
        let end: usize = block_end
            .try_into()
            .map_err(|_| Error::message("ThingDB table block end is too large"))?;
        if checksum(&bytes[start..end]) != block_checksum {
            return Err(Error::message("ThingDB table block checksum mismatch"));
        }
        previous_offset = Some(offset);
        previous_end = Some(block_end);
        blocks.push(TableBlock {
            offset,
            length,
            first_key,
            last_key,
            checksum: block_checksum,
        });
    }
    if block_count > 0 && (previous_end != Some(footer_start as u64)) {
        return Err(Error::message(
            "ThingDB table blocks do not cover the data area",
        ));
    }
    let stored_checksum = read_u32(bytes, &mut cursor)?;
    if stored_checksum != checksum(&bytes[footer_start..cursor - 4]) {
        return Err(Error::message("ThingDB table footer checksum mismatch"));
    }
    if cursor != bytes.len() {
        return Err(Error::message("trailing bytes after ThingDB table footer"));
    }
    Ok(blocks)
}

#[cfg(not(unix))]
fn read_table_value(
    file: &mut File,
    entry: &TableIndexEntry,
    is_v2: bool,
) -> Result<Option<Vec<u8>>> {
    file.seek(SeekFrom::Start(entry.offset))?;
    let length: usize = entry
        .length
        .try_into()
        .map_err(|_| Error::message("ThingDB table record is too large"))?;
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes)?;
    let mut cursor = 0;
    let key = read_bytes(&bytes, &mut cursor)?;
    if key != entry.key {
        return Err(Error::message("ThingDB table index key mismatch"));
    }
    let value = if !is_v2 {
        Some(read_bytes(&bytes, &mut cursor)?)
    } else if bytes.get(cursor) == Some(&1) {
        cursor += 1;
        Some(read_bytes(&bytes, &mut cursor)?)
    } else if bytes.get(cursor) == Some(&2) {
        cursor += 1;
        None
    } else {
        return Err(Error::message("invalid ThingDB table operation"));
    };
    let checksum_offset = cursor;
    let stored = read_u32(&bytes, &mut cursor)?;
    if stored != checksum(&bytes[..checksum_offset]) {
        return Err(Error::message("ThingDB table checksum mismatch"));
    }
    if cursor != bytes.len() {
        return Err(Error::message("trailing bytes in ThingDB table record"));
    }
    Ok(value)
}

fn read_table_value_handle(
    file: &Arc<File>,
    entry: &TableIndexEntry,
    is_v2: bool,
) -> Result<(Option<Vec<u8>>, u64, u64)> {
    let reader_wait = 0;
    let read_started = Instant::now();
    #[cfg(unix)]
    let value = {
        let length: usize = entry
            .length
            .try_into()
            .map_err(|_| Error::message("ThingDB table record is too large"))?;
        let mut bytes = vec![0; length];
        file.read_exact_at(&mut bytes, entry.offset)?;
        decode_table_value(&bytes, entry, is_v2)
    };
    #[cfg(not(unix))]
    let value = {
        let mut reader = file.try_clone()?;
        read_table_value(&mut reader, entry, is_v2)
    };
    let read_duration = elapsed_nanos(read_started.elapsed());
    Ok((value?, reader_wait, read_duration))
}

fn read_table_block_value(
    file: &Arc<File>,
    block: &TableBlock,
    key: &[u8],
    is_v2: bool,
) -> Result<(Option<Option<Vec<u8>>>, u64, u64)> {
    let started = Instant::now();
    let length: usize = block
        .length
        .try_into()
        .map_err(|_| Error::message("ThingDB table block is too large"))?;
    let mut bytes = vec![0; length];
    #[cfg(unix)]
    file.read_exact_at(&mut bytes, block.offset)?;
    #[cfg(not(unix))]
    {
        let mut reader = file.try_clone()?;
        reader.seek(SeekFrom::Start(block.offset))?;
        reader.read_exact(&mut bytes)?;
    }
    if checksum(&bytes) != block.checksum {
        return Err(Error::message("ThingDB table block checksum mismatch"));
    }
    let mut cursor = 0;
    while cursor < bytes.len() {
        let record_start = cursor;
        let record_key = read_bytes(&bytes, &mut cursor)?;
        if is_v2 {
            match read_byte(&bytes, &mut cursor)? {
                1 | 2 => {},
                _ => return Err(Error::message("invalid ThingDB table operation")),
            }
            if bytes.get(cursor - 1) == Some(&1) {
                let _ = read_bytes(&bytes, &mut cursor)?;
            }
        } else {
            let _ = read_bytes(&bytes, &mut cursor)?;
        }
        let checksum_end = cursor;
        let stored = read_u32(&bytes, &mut cursor)?;
        if stored != checksum(&bytes[record_start..checksum_end]) {
            return Err(Error::message("ThingDB table checksum mismatch"));
        }
        if record_key == key {
            let mut decode_cursor = 0;
            let _ = read_bytes(&bytes[record_start..cursor], &mut decode_cursor)?;
            let value = if !is_v2 {
                Some(read_bytes(
                    &bytes[record_start..cursor],
                    &mut decode_cursor,
                )?)
            } else if bytes[record_start + decode_cursor] == 1 {
                decode_cursor += 1;
                Some(read_bytes(
                    &bytes[record_start..cursor],
                    &mut decode_cursor,
                )?)
            } else {
                None
            };
            return Ok((Some(value), block.length, elapsed_nanos(started.elapsed())));
        }
    }
    Ok((None, block.length, elapsed_nanos(started.elapsed())))
}

#[cfg(unix)]
fn decode_table_value(
    bytes: &[u8],
    entry: &TableIndexEntry,
    is_v2: bool,
) -> Result<Option<Vec<u8>>> {
    let mut cursor = 0;
    let key = read_bytes(bytes, &mut cursor)?;
    if key != entry.key {
        return Err(Error::message("ThingDB table index key mismatch"));
    }
    let value = if !is_v2 {
        Some(read_bytes(bytes, &mut cursor)?)
    } else if bytes.get(cursor) == Some(&1) {
        cursor += 1;
        Some(read_bytes(bytes, &mut cursor)?)
    } else if bytes.get(cursor) == Some(&2) {
        cursor += 1;
        None
    } else {
        return Err(Error::message("invalid ThingDB table operation"));
    };
    let checksum_offset = cursor;
    let stored = read_u32(bytes, &mut cursor)?;
    if stored != checksum(&bytes[..checksum_offset]) {
        return Err(Error::message("ThingDB table checksum mismatch"));
    }
    if cursor != bytes.len() {
        return Err(Error::message("trailing bytes in ThingDB table record"));
    }
    Ok(value)
}

fn write_manifest(path: &Path, manifest: &Manifest, sync: bool) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(manifest)?;
    let temp = path.join(MANIFEST_TEMP_FILE);
    let final_path = path.join(MANIFEST_FILE);
    let mut file = File::create(&temp)?;
    file.write_all(&bytes)?;
    if sync {
        file.sync_all()?;
    }
    maybe_fail("after-manifest-sync-before-rename", current_fault_point())?;
    fs::rename(temp, final_path)?;
    if sync {
        sync_directory(path)?;
    }
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    OpenOptions::new().read(true).open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}

fn checksum(bytes: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    hasher.finalize()
}

fn write_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn write_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn write_bytes(output: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    let length: u64 = bytes
        .len()
        .try_into()
        .map_err(|_| Error::message("ThingDB value is too large"))?;
    write_u64(output, length);
    output.extend_from_slice(bytes);
    Ok(())
}

fn read_byte(bytes: &[u8], cursor: &mut usize) -> Result<u8> {
    if *cursor >= bytes.len() {
        return Err(Error::message("truncated ThingDB record"));
    }
    let value = bytes[*cursor];
    *cursor += 1;
    Ok(value)
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32> {
    let end = cursor
        .checked_add(4)
        .ok_or_else(|| Error::message("ThingDB length overflow"))?;
    let value = bytes
        .get(*cursor..end)
        .ok_or_else(|| Error::message("truncated ThingDB record"))?;
    *cursor = end;
    Ok(u32::from_be_bytes(
        value
            .try_into()
            .map_err(|_| Error::message("invalid ThingDB integer"))?,
    ))
}

fn read_u64(bytes: &[u8], cursor: &mut usize) -> Result<u64> {
    let end = cursor
        .checked_add(8)
        .ok_or_else(|| Error::message("ThingDB length overflow"))?;
    let value = bytes
        .get(*cursor..end)
        .ok_or_else(|| Error::message("truncated ThingDB record"))?;
    *cursor = end;
    Ok(u64::from_be_bytes(
        value
            .try_into()
            .map_err(|_| Error::message("invalid ThingDB integer"))?,
    ))
}

fn read_bytes(bytes: &[u8], cursor: &mut usize) -> Result<Vec<u8>> {
    let length = read_u64(bytes, cursor)?;
    let length: usize = length
        .try_into()
        .map_err(|_| Error::message("ThingDB value length does not fit platform"))?;
    let end = cursor
        .checked_add(length)
        .ok_or_else(|| Error::message("ThingDB value length overflow"))?;
    let value = bytes
        .get(*cursor..end)
        .ok_or_else(|| Error::message("truncated ThingDB value"))?
        .to_vec();
    *cursor = end;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;

    #[test]
    fn in_memory_database_is_ordered_atomic_and_non_durable() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::in_memory().unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        let events = db
            .keyspace("events", KeyspaceCreateOptions::default)
            .unwrap();

        db.batch()
            .put(&objects, b"b", b"two")
            .put(&events, b"1", b"event")
            .put(&objects, b"a", b"one")
            .commit()
            .unwrap();

        let snapshot = db.snapshot().unwrap();
        objects.insert(b"a", b"updated").unwrap();
        assert_eq!(
            snapshot.keyspace("objects").get(b"a"),
            Some(b"one".to_vec())
        );
        assert_eq!(objects.get(b"a").unwrap(), Some(b"updated".to_vec()));
        assert_eq!(
            objects
                .prefix(b"")
                .map(|entry| entry.key)
                .collect::<Vec<_>>(),
            vec![b"a".to_vec(), b"b".to_vec()]
        );
        assert_eq!(
            objects
                .range_bounds(Some((b"a", true)), Some((b"a", true)))
                .count(),
            1
        );
        assert_eq!(events.get(b"1").unwrap(), Some(b"event".to_vec()));

        let diagnostics = db.wal_diagnostics().unwrap();
        assert_eq!(diagnostics.journal_bytes, 0);
        assert_eq!(diagnostics.frame_count, 0);
        assert_eq!(diagnostics.physical_sync_count, 0);
        assert_eq!(diagnostics.recovery_bytes, 0);
        assert_eq!(diagnostics.flush_count, 0);
        assert_eq!(db.journal_disk_space().unwrap(), 0);
        assert_eq!(db.journal_count(), 0);
        assert!(db.persist(PersistMode::SyncAll).is_err());
        assert!(db.compact().is_err());
        assert!(directory.path().read_dir().unwrap().next().is_none());
    }

    #[test]
    fn in_memory_instances_are_isolated_and_batches_share_one_state_boundary() {
        let first = Database::in_memory().unwrap();
        let second = Database::in_memory().unwrap();
        let first_objects = first
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        let second_objects = second
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();

        first_objects.insert(b"only-first", b"value").unwrap();
        first
            .batch()
            .put(&first_objects, b"a", b"one")
            .put(&first_objects, b"b", b"two")
            .commit()
            .unwrap();

        assert!(second_objects.get(b"only-first").unwrap().is_none());
        assert_eq!(first_objects.iter().count(), 3);
        let diagnostics = first.wal_diagnostics().unwrap();
        assert_eq!(diagnostics.logical_commit_count, 2);
        assert_eq!(diagnostics.total_group_size, 2);
        assert_eq!(diagnostics.physical_sync_count, 0);
    }

    #[test]
    fn in_memory_commits_bypass_durable_coordination() {
        let db = Database::in_memory().unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();

        for index in 0..128u16 {
            objects
                .insert(index.to_be_bytes(), index.to_be_bytes())
                .unwrap();
        }

        let diagnostics = db.wal_diagnostics().unwrap();
        assert_eq!(diagnostics.physical_sync_count, 0);
        assert_eq!(diagnostics.queue_wait_duration_ns, 0);
        assert_eq!(diagnostics.journal_bytes, 0);
        assert_eq!(diagnostics.frame_count, 0);
        assert_eq!(objects.iter().count(), 128);
    }

    #[test]
    fn in_memory_keyspaces_use_isolated_fast_lookup_state() {
        let db = Database::in_memory().unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        let events = db
            .keyspace("events", KeyspaceCreateOptions::default)
            .unwrap();

        objects.insert(b"same-key", b"object").unwrap();
        events.insert(b"same-key", b"event").unwrap();
        assert_eq!(objects.get(b"same-key").unwrap(), Some(b"object".to_vec()));
        assert_eq!(events.get(b"same-key").unwrap(), Some(b"event".to_vec()));

        let diagnostics = db.ram_diagnostics().unwrap();
        assert_eq!(diagnostics.lookup_count, 2);
        assert!(diagnostics.lock_held_duration_ns > 0);
        assert_eq!(db.journal_disk_space().unwrap(), 0);
        assert_eq!(db.journal_count(), 0);
    }

    #[test]
    fn in_memory_with_value_reads_through_borrowed_value_boundary() {
        let db = Database::in_memory().unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        objects.insert(b"key", b"value").unwrap();

        let value = objects
            .with_value(b"key", |value| {
                Ok(value.map(|value| String::from_utf8_lossy(value).into_owned()))
            })
            .unwrap();
        assert_eq!(value.as_deref(), Some("value"));
    }

    #[test]
    fn in_memory_bounded_iterations_use_ordered_bounds() {
        let db = Database::in_memory().unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        for key in [b"a-1".as_slice(), b"a-2", b"b-1", b"c-1"] {
            objects.insert(key, key).unwrap();
        }

        let entries = objects
            .range_bounds(Some((b"a-2", true)), Some((b"c-1", false)))
            .into_result()
            .unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.key.as_slice())
                .collect::<Vec<_>>(),
            vec![b"a-2".as_slice(), b"b-1"]
        );
        let prefix = objects.prefix(b"b-").into_result().unwrap();
        assert_eq!(prefix.len(), 1);

        let diagnostics = db.ram_diagnostics().unwrap();
        assert_eq!(diagnostics.iteration_count, 2);
        assert_eq!(diagnostics.iteration_entries_examined, 3);
        assert_eq!(diagnostics.iteration_entries_returned, 3);
    }

    #[test]
    fn in_memory_first_prefix_after_honors_result_limit() {
        let db = Database::in_memory().unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        for index in 0..1024u16 {
            let key = format!("queue\0{index:04}");
            objects.insert(key.as_bytes(), b"job").unwrap();
        }

        let entry = objects
            .first_prefix_after(b"queue\0", None)
            .unwrap()
            .expect("prefix should contain an entry");
        let expected = "queue\0".to_string() + "0000";
        assert_eq!(entry.key, expected.as_bytes());

        let diagnostics = db.ram_diagnostics().unwrap();
        assert_eq!(diagnostics.iteration_entries_returned, 1);
        assert_eq!(diagnostics.iteration_entries_examined, 1);
    }

    #[test]
    fn in_memory_shared_reads_survive_mutation_after_lookup() {
        let db = Database::in_memory().unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        objects.insert(b"key", b"before").unwrap();

        let shared = objects.get_shared(b"key").unwrap().unwrap();
        objects.insert(b"key", b"after").unwrap();

        assert_eq!(shared.as_slice(), b"before");
        assert_eq!(objects.get(b"key").unwrap(), Some(b"after".to_vec()));
        assert!(db.ram_diagnostics().unwrap().lookup_count >= 1);
    }

    #[test]
    fn in_memory_snapshots_remain_atomic_during_concurrent_batches() {
        let db = Database::in_memory().unwrap();
        let left = db.keyspace("left", KeyspaceCreateOptions::default).unwrap();
        let right = db
            .keyspace("right", KeyspaceCreateOptions::default)
            .unwrap();
        let writer_db = db.clone();
        let writer_left = left;
        let writer_right = right;
        let writer = std::thread::spawn(move || {
            for index in 0..256u16 {
                let key = index.to_be_bytes();
                writer_db
                    .batch()
                    .put(&writer_left, key, b"left")
                    .put(&writer_right, key, b"right")
                    .commit()
                    .unwrap();
            }
        });

        for _ in 0..64 {
            let snapshot = db.snapshot().unwrap();
            for index in 0..256u16 {
                let key = index.to_be_bytes();
                let left_value = snapshot.keyspace("left").get(key);
                let right_value = snapshot.keyspace("right").get(key);
                assert_eq!(left_value.is_some(), right_value.is_some());
            }
        }
        writer.join().unwrap();
    }

    #[test]
    fn in_memory_diagnostics_are_zero_for_durable_databases() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::builder(directory.path()).open().unwrap();
        assert_eq!(db.ram_diagnostics().unwrap(), RamDiagnostics::default());
    }

    #[test]
    fn persists_and_reopens() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let keyspace = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        keyspace.insert(b"b", b"two").unwrap();
        keyspace.insert(b"a", b"one").unwrap();
        db.persist(PersistMode::SyncAll).unwrap();
        drop(keyspace);
        drop(db);

        let db = Database::open(directory.path()).unwrap();
        let keyspace = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        assert_eq!(keyspace.get(b"a").unwrap(), Some(b"one".to_vec()));
        let entries: Vec<_> = keyspace.iter().collect();
        assert_eq!(entries[0].key, b"a");
        assert_eq!(entries[1].key, b"b");
    }

    #[test]
    fn durable_table_reads_record_reader_and_file_timings() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        objects.insert(b"key", b"value").unwrap();
        db.persist(PersistMode::SyncAll).unwrap();

        assert_eq!(objects.get(b"key").unwrap(), Some(b"value".to_vec()));
        let diagnostics = db.wal_diagnostics().unwrap();
        assert!(diagnostics.table_lookup_count > 0);
        assert_eq!(diagnostics.table_reader_wait_duration_ns, 0);
        assert!(diagnostics.file_read_duration_ns > 0);
        assert!(diagnostics.read_lock_held_duration_ns > 0);
    }

    #[test]
    fn incremental_tables_preserve_updates_and_tombstones() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        objects.insert(b"a", b"one").unwrap();
        db.persist(PersistMode::SyncAll).unwrap();
        objects.insert(b"b", b"two").unwrap();
        db.persist(PersistMode::SyncAll).unwrap();
        objects.remove(b"a").unwrap();
        db.persist(PersistMode::SyncAll).unwrap();
        drop(objects);
        drop(db);

        let manifest: Manifest =
            serde_json::from_slice(&std::fs::read(directory.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        assert_eq!(manifest.table_files.len(), 3);

        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        assert_eq!(objects.get(b"a").unwrap(), None);
        assert_eq!(objects.get(b"b").unwrap(), Some(b"two".to_vec()));
        db.compact().unwrap();
        drop(objects);
        drop(db);

        let manifest: Manifest =
            serde_json::from_slice(&std::fs::read(directory.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        assert_eq!(manifest.table_files.len(), 1);
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        assert_eq!(objects.get(b"a").unwrap(), None);
        assert_eq!(objects.get(b"b").unwrap(), Some(b"two".to_vec()));
    }

    #[test]
    fn layered_reads_and_scans_use_newest_values_and_report_table_work() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();

        objects.insert(b"a", b"old").unwrap();
        objects.insert(b"b", b"two").unwrap();
        db.persist(PersistMode::SyncAll).unwrap();
        objects.insert(b"a", b"new").unwrap();
        objects.remove(b"b").unwrap();
        db.persist(PersistMode::SyncAll).unwrap();

        assert_eq!(objects.get(b"a").unwrap(), Some(b"new".to_vec()));
        assert_eq!(objects.get(b"b").unwrap(), None);
        let entries: Vec<_> = objects
            .range_bounds(Some((b"a", true)), Some((b"z", true)))
            .collect();
        assert_eq!(
            entries,
            vec![Entry {
                key: b"a".to_vec(),
                value: b"new".to_vec()
            }]
        );

        let diagnostics = db.wal_diagnostics().unwrap();
        assert_eq!(diagnostics.table_layer_count, 2);
        assert!(diagnostics.table_lookup_count >= 2);
        assert!(diagnostics.table_layers_consulted >= 2);
        assert!(diagnostics.table_bytes_read > 0);
        assert!(diagnostics.table_read_duration_ns > 0);
        assert_eq!(diagnostics.scan_count, 1);
        assert_eq!(diagnostics.scan_keys_examined, 2);
        assert!(diagnostics.scan_layers_consulted >= 2);
        assert!(diagnostics.scan_duration_ns > 0);
    }

    #[test]
    fn bounded_scans_merge_overlapping_layers_without_materializing_outside_range() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();

        objects.insert(b"aa-1", b"old").unwrap();
        objects.insert(b"ab-1", b"keep").unwrap();
        objects.insert(b"ba-1", b"outside").unwrap();
        db.persist(PersistMode::SyncAll).unwrap();
        objects.insert(b"aa-1", b"new").unwrap();
        objects.insert(b"ac-1", b"added").unwrap();
        db.persist(PersistMode::SyncAll).unwrap();
        objects.remove(b"ab-1").unwrap();
        objects.insert(b"ad-1", b"latest").unwrap();
        db.persist(PersistMode::SyncAll).unwrap();

        let range: Vec<_> = objects
            .range_bounds(Some((b"aa-1", true)), Some((b"ad-1", false)))
            .map(|entry| (entry.key, entry.value))
            .collect();
        assert_eq!(
            range,
            vec![
                (b"aa-1".to_vec(), b"new".to_vec()),
                (b"ac-1".to_vec(), b"added".to_vec()),
            ]
        );

        let prefix: Vec<_> = objects.prefix(b"a").map(|entry| entry.key).collect();
        assert_eq!(
            prefix,
            vec![b"aa-1".to_vec(), b"ac-1".to_vec(), b"ad-1".to_vec()]
        );

        let diagnostics = db.wal_diagnostics().unwrap();
        assert_eq!(diagnostics.scan_count, 2);
        assert!(diagnostics.scan_keys_examined >= 4);
        assert!(diagnostics.scan_layers_consulted >= 3);

        drop(objects);
        drop(db);

        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        let reopened_range: Vec<_> = objects
            .range_bounds(Some((b"aa-1", true)), Some((b"ad-1", false)))
            .map(|entry| (entry.key, entry.value))
            .collect();
        assert_eq!(reopened_range, range);
        let reopened_prefix: Vec<_> = objects.prefix(b"a").map(|entry| entry.key).collect();
        assert_eq!(
            reopened_prefix,
            vec![b"aa-1".to_vec(), b"ac-1".to_vec(), b"ad-1".to_vec()]
        );
    }

    #[test]
    fn automatic_layer_threshold_compacts_after_bounded_flush() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::builder(directory.path())
            .max_memtable_bytes(1)
            .max_table_layers(2)
            .open()
            .unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();

        objects.insert(b"a", b"one").unwrap();
        objects.insert(b"b", b"two").unwrap();

        assert_eq!(objects.get(b"a").unwrap(), Some(b"one".to_vec()));
        assert_eq!(objects.get(b"b").unwrap(), Some(b"two".to_vec()));
        assert_eq!(objects.iter().count(), 2);
        let manifest: Manifest =
            serde_json::from_slice(&std::fs::read(directory.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        assert_eq!(manifest.table_files.len(), 1);
    }

    #[test]
    fn interrupted_flush_recovers_from_previous_manifest_and_wal() {
        for point in [
            "before-table-write",
            "after-table-sync-before-rename",
            "before-table-rename",
            "after-table-rename-before-manifest",
            "before-manifest-write",
            "after-manifest-sync-before-rename",
            "after-manifest-rename-before-wal-truncate",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let db = Database::open(directory.path()).unwrap();
            let objects = db
                .keyspace("objects", KeyspaceCreateOptions::default)
                .unwrap();
            objects.insert(b"a", b"one").unwrap();
            db.persist(PersistMode::SyncAll).unwrap();
            objects.insert(b"b", b"two").unwrap();

            set_fault_point(Some(point));
            let result = db.persist(PersistMode::SyncAll);
            set_fault_point(None);
            assert!(result.is_err(), "fault point {point} did not fail");
            drop(objects);
            drop(db);

            let db = Database::open(directory.path()).unwrap();
            let objects = db
                .keyspace("objects", KeyspaceCreateOptions::default)
                .unwrap();
            assert_eq!(objects.get(b"a").unwrap(), Some(b"one".to_vec()), "{point}");
            assert_eq!(objects.get(b"b").unwrap(), Some(b"two".to_vec()), "{point}");
            assert!(
                !directory.path().join(MANIFEST_TEMP_FILE).exists(),
                "temporary manifest remained after {point}"
            );
        }
    }

    #[test]
    fn interrupted_compaction_recovers_from_old_or_new_manifest() {
        for point in [
            "before-table-write",
            "after-table-sync-before-rename",
            "before-table-rename",
            "after-table-rename-before-manifest",
            "before-manifest-write",
            "after-manifest-sync-before-rename",
            "after-manifest-rename-before-wal-truncate",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let db = Database::open(directory.path()).unwrap();
            let objects = db
                .keyspace("objects", KeyspaceCreateOptions::default)
                .unwrap();
            objects.insert(b"a", b"one").unwrap();
            db.persist(PersistMode::SyncAll).unwrap();
            objects.insert(b"b", b"two").unwrap();

            set_fault_point(Some(point));
            let result = db.compact();
            set_fault_point(None);
            assert!(result.is_err(), "fault point {point} did not fail");
            drop(objects);
            drop(db);

            let db = Database::open(directory.path()).unwrap();
            let objects = db
                .keyspace("objects", KeyspaceCreateOptions::default)
                .unwrap();
            assert_eq!(objects.get(b"a").unwrap(), Some(b"one".to_vec()), "{point}");
            assert_eq!(objects.get(b"b").unwrap(), Some(b"two".to_vec()), "{point}");
            assert!(
                !directory.path().join(MANIFEST_TEMP_FILE).exists(),
                "temporary manifest remained after {point}"
            );
        }
    }

    #[test]
    fn invalid_manifest_table_names_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        drop(db);
        let manifest_path = directory.path().join(MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        manifest["table_file"] = serde_json::Value::String("../outside.tdb".to_string());
        manifest["table_files"] = serde_json::json!(["../outside.tdb"]);
        std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

        let Err(error) = Database::open(directory.path()) else {
            panic!("manifest with path traversal unexpectedly opened")
        };
        assert!(error.to_string().contains("invalid ThingDB table filename"));
    }

    #[test]
    fn invalid_manifest_state_is_rejected_before_recovery() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        drop(db);
        let manifest_path = directory.path().join(MANIFEST_FILE);

        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        manifest["table_sequence"] = serde_json::json!(1);
        std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let Err(error) = Database::open(directory.path()) else {
            panic!("manifest without a table unexpectedly opened")
        };
        assert!(error.to_string().contains("table sequence without a table"));

        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        objects.insert(b"a", b"one").unwrap();
        db.compact().unwrap();
        drop(objects);
        drop(db);

        let manifest_path = directory.path().join(MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        manifest["table_files"] = serde_json::json!(["table-99999999999999999999-missing.tdb"]);
        manifest["table_file"] = serde_json::Value::Null;
        std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let Err(error) = Database::open(directory.path()) else {
            panic!("manifest with a missing table unexpectedly opened")
        };
        assert!(
            error
                .to_string()
                .contains("manifest references missing table")
        );
    }

    #[test]
    fn unsupported_manifest_version_is_rejected_before_recovery() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        drop(db);

        let manifest_path = directory.path().join(MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        manifest["format_version"] = serde_json::json!(FORMAT_VERSION + 1);
        std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

        let Err(error) = Database::open(directory.path()) else {
            panic!("unsupported manifest version unexpectedly opened")
        };
        assert!(
            error
                .to_string()
                .contains("unsupported ThingDB format version")
        );
    }

    #[test]
    fn manifest_rejects_table_sequence_older_than_referenced_table() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        objects.insert(b"a", b"one").unwrap();
        db.compact().unwrap();
        drop(objects);
        drop(db);

        let manifest_path = directory.path().join(MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        manifest["table_sequence"] = serde_json::json!(0);
        std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

        let Err(error) = Database::open(directory.path()) else {
            panic!("manifest with an older table sequence unexpectedly opened")
        };
        assert!(
            error
                .to_string()
                .contains("table sequence exceeds manifest sequence")
        );
    }

    #[test]
    fn table_key_order_corruption_is_reported() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        objects.insert(b"a", b"one").unwrap();
        objects.insert(b"b", b"two").unwrap();
        db.compact().unwrap();
        drop(objects);
        drop(db);

        let manifest: Manifest =
            serde_json::from_slice(&std::fs::read(directory.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        let table_path = directory.path().join(manifest.table_file.unwrap());
        let mut bytes = std::fs::read(&table_path).unwrap();
        let first_record_start = TABLE_MAGIC.len() + 16;
        let key_length = usize::try_from(u64::from_be_bytes(
            bytes[first_record_start..first_record_start + 8]
                .try_into()
                .unwrap(),
        ))
        .unwrap();
        let first_key_offset = first_record_start + 8;
        bytes[first_key_offset] = b'z';
        // Keep the first record internally valid so recovery reaches the
        // sorted-key invariant instead of stopping at its checksum.
        let value_length_offset = first_key_offset + key_length + 1;
        let value_length = usize::try_from(u64::from_be_bytes(
            bytes[value_length_offset..value_length_offset + 8]
                .try_into()
                .unwrap(),
        ))
        .unwrap();
        let first_record_end = value_length_offset + 8 + value_length;
        let record_checksum = checksum(&bytes[first_record_start..first_record_end]);
        bytes[first_record_end..first_record_end + 4]
            .copy_from_slice(&record_checksum.to_be_bytes());
        std::fs::write(&table_path, bytes).unwrap();

        let Err(error) = Database::open(directory.path()) else {
            panic!("table with unsorted keys unexpectedly opened")
        };
        assert!(
            error
                .to_string()
                .contains("table keys are not strictly ordered")
        );
    }

    #[test]
    fn batch_is_atomic_after_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        db.batch()
            .put(&objects, b"a", b"one")
            .put(&objects, b"b", b"two")
            .commit()
            .unwrap();
        drop(objects);
        drop(db);
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        assert_eq!(objects.get(b"a").unwrap(), Some(b"one".to_vec()));
        assert_eq!(objects.get(b"b").unwrap(), Some(b"two".to_vec()));
    }

    #[test]
    fn cross_keyspace_batch_is_atomic_after_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        let events = db
            .keyspace("events", KeyspaceCreateOptions::default)
            .unwrap();
        db.batch()
            .put(&objects, b"a", b"one")
            .put(&events, b"1", b"created")
            .commit()
            .unwrap();
        drop(events);
        drop(objects);
        drop(db);

        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        let events = db
            .keyspace("events", KeyspaceCreateOptions::default)
            .unwrap();
        assert_eq!(objects.get(b"a").unwrap(), Some(b"one".to_vec()));
        assert_eq!(events.get(b"1").unwrap(), Some(b"created".to_vec()));
    }

    #[test]
    fn grouped_writes_replay_completely_after_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        let writers = 16;
        let barrier = Arc::new(std::sync::Barrier::new(writers));
        let handles: Vec<_> = (0..writers)
            .map(|index| {
                let objects = objects.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    objects
                        .insert(format!("key-{index}").as_bytes(), b"value")
                        .unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        drop(objects);
        drop(db);

        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        for index in 0..writers {
            assert_eq!(
                objects.get(format!("key-{index}").as_bytes()).unwrap(),
                Some(b"value".to_vec())
            );
        }
    }

    #[test]
    fn injected_wal_failures_preserve_batch_atomicity() {
        for point in [
            "before-wal-append",
            "after-wal-write-before-sync",
            "after-wal-sync-before-state-apply",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let db = Database::open(directory.path()).unwrap();
            let objects = db
                .keyspace("objects", KeyspaceCreateOptions::default)
                .unwrap();
            set_fault_point(Some(point));
            let result = db
                .batch()
                .put(&objects, b"a", b"one")
                .put(&objects, b"b", b"two")
                .commit();
            set_fault_point(None);
            assert!(result.is_err(), "fault point {point} did not fail");
            if point == "after-wal-sync-before-state-apply" {
                let rejected = db.batch().put(&objects, b"rejected", b"write").commit();
                assert!(rejected.is_err());
                drop(objects);
                drop(db);
                let db = Database::open(directory.path()).unwrap();
                let objects = db
                    .keyspace("objects", KeyspaceCreateOptions::default)
                    .unwrap();
                assert_eq!(objects.get(b"a").unwrap(), Some(b"one".to_vec()));
                assert_eq!(objects.get(b"b").unwrap(), Some(b"two".to_vec()));
                continue;
            }
            db.batch()
                .put(&objects, b"a", b"one")
                .put(&objects, b"b", b"two")
                .commit()
                .unwrap();
            drop(objects);
            drop(db);

            let db = Database::open(directory.path()).unwrap();
            let objects = db
                .keyspace("objects", KeyspaceCreateOptions::default)
                .unwrap();
            let first = objects.get(b"a").unwrap();
            let second = objects.get(b"b").unwrap();
            assert_eq!(first, Some(b"one".to_vec()), "fault point {point}");
            assert_eq!(second, Some(b"two".to_vec()), "fault point {point}");
        }
    }

    #[test]
    fn truncating_wal_at_every_boundary_recovers_only_complete_frames() {
        let source = tempfile::tempdir().unwrap();
        let db = Database::open(source.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        objects.insert(b"a", b"one").unwrap();
        let first_frame_len = std::fs::metadata(source.path().join(WAL_FILE))
            .unwrap()
            .len();
        objects.insert(b"b", b"two").unwrap();
        drop(objects);
        drop(db);
        let complete = std::fs::read(source.path().join(WAL_FILE)).unwrap();
        let second_frame_len = complete.len() as u64 - first_frame_len;

        for cut in 0..=complete.len() {
            let directory = tempfile::tempdir().unwrap();
            let db = Database::open(directory.path()).unwrap();
            drop(db);
            std::fs::write(directory.path().join(WAL_FILE), &complete[..cut]).unwrap();
            let db = Database::open(directory.path()).unwrap();
            let objects = db
                .keyspace("objects", KeyspaceCreateOptions::default)
                .unwrap();
            let expected = if cut < first_frame_len as usize {
                0
            } else if cut < (first_frame_len + second_frame_len) as usize {
                1
            } else {
                2
            };
            let actual = usize::from(objects.get(b"a").unwrap().is_some())
                + usize::from(objects.get(b"b").unwrap().is_some());
            assert_eq!(actual, expected, "unexpected recovery at WAL byte {cut}");
        }
    }

    #[test]
    fn malformed_wal_length_and_operation_are_reported() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        drop(db);
        let mut malformed = Vec::from(WAL_MAGIC.as_slice());
        malformed.extend_from_slice(&1_u64.to_be_bytes());
        std::fs::write(directory.path().join(WAL_FILE), malformed).unwrap();
        let Err(error) = Database::open(directory.path()) else {
            panic!("malformed WAL length unexpectedly opened")
        };
        assert!(error.to_string().contains("frame length"));

        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        drop(db);
        let mut invalid = encode_frame(
            1,
            &[Operation::Put {
                key: b"objects\0a".to_vec(),
                value: b"one".to_vec(),
            }],
        )
        .unwrap();
        invalid[28] = 9;
        std::fs::write(directory.path().join(WAL_FILE), invalid).unwrap();
        let Err(error) = Database::open(directory.path()) else {
            panic!("invalid WAL operation unexpectedly opened")
        };
        assert!(error.to_string().contains("invalid ThingDB WAL operation"));
    }

    #[test]
    fn diagnostics_report_wal_timings_and_recovery() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        objects.insert(b"a", b"one").unwrap();
        let diagnostics = db.wal_diagnostics().unwrap();
        assert_eq!(diagnostics.frame_count, 1);
        assert_eq!(diagnostics.logical_commit_count, 1);
        assert_eq!(diagnostics.physical_sync_count, 1);
        assert_eq!(diagnostics.max_group_size, 1);
        assert!(diagnostics.journal_bytes > 0);
        assert!(diagnostics.wal_bytes_appended > 0);
        assert!(diagnostics.buffer_reserved_bytes >= diagnostics.wal_bytes_appended);
        assert!(diagnostics.sync_duration_ns > 0);
        drop(objects);
        drop(db);

        let db = Database::open(directory.path()).unwrap();
        let diagnostics = db.wal_diagnostics().unwrap();
        assert!(diagnostics.recovery_bytes > 0);
        assert!(diagnostics.recovery_duration_ns > 0);
    }

    #[test]
    fn bounded_memtable_flushes_before_acknowledgement() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::builder(directory.path())
            .max_memtable_bytes(1)
            .open()
            .unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();

        objects.insert(b"a", b"one").unwrap();

        let diagnostics = db.wal_diagnostics().unwrap();
        assert_eq!(diagnostics.memtable_bytes, 0);
        assert!(!diagnostics.memtable_over_budget);
        assert_eq!(diagnostics.flush_count, 1);
        assert_eq!(diagnostics.automatic_flush_count, 1);
        assert_eq!(diagnostics.journal_bytes, 0);

        drop(objects);
        drop(db);
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        assert_eq!(objects.get(b"a").unwrap(), Some(b"one".to_vec()));
    }

    #[test]
    fn durable_memtable_flushes_in_background_after_acknowledgement() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::builder(directory.path())
            .max_memtable_bytes(1024)
            .open()
            .unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();

        objects.insert(b"a", vec![b'x'; 2048]).unwrap();
        assert_eq!(objects.get(b"a").unwrap(), Some(vec![b'x'; 2048]));

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let diagnostics = db.wal_diagnostics().unwrap();
            if diagnostics.flush_count >= 1 {
                assert_eq!(diagnostics.memtable_bytes, 0);
                assert_eq!(diagnostics.journal_bytes, 0);
                assert!(!diagnostics.recovery_required);
                break;
            }
            assert!(Instant::now() < deadline, "background flush did not finish");
            std::thread::sleep(Duration::from_millis(1));
        }

        drop(objects);
        drop(db);
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        assert_eq!(objects.get(b"a").unwrap(), Some(vec![b'x'; 2048]));
    }

    #[test]
    fn wal_budget_schedules_background_checkpoint() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::builder(directory.path())
            .max_journaling_size(1)
            .max_memtable_bytes(1024 * 1024)
            .open()
            .unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();

        objects.insert(b"a", b"one").unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let diagnostics = db.wal_diagnostics().unwrap();
            if diagnostics.flush_count >= 1 {
                assert_eq!(diagnostics.journal_bytes, 0);
                assert_eq!(diagnostics.memtable_bytes, 0);
                assert!(!diagnostics.recovery_required);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "WAL-budget checkpoint did not finish"
            );
            std::thread::sleep(Duration::from_millis(1));
        }

        assert_eq!(objects.get(b"a").unwrap(), Some(b"one".to_vec()));
    }

    #[test]
    fn failed_bounded_memtable_flush_requires_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::builder(directory.path())
            .max_memtable_bytes(1)
            .open()
            .unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();

        set_fault_point(Some("before-table-write"));
        let result = objects.insert(b"a", b"one");
        set_fault_point(None);
        assert!(result.is_err());
        assert!(db.wal_diagnostics().unwrap().recovery_required);
        assert!(objects.insert(b"b", b"two").is_err());

        drop(objects);
        drop(db);
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        assert_eq!(objects.get(b"a").unwrap(), Some(b"one".to_vec()));
    }

    #[test]
    fn concurrent_writes_share_physical_syncs() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        let writers = 32;
        let barrier = Arc::new(std::sync::Barrier::new(writers));
        let handles: Vec<_> = (0..writers)
            .map(|index| {
                let objects = objects.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    objects
                        .insert(format!("key-{index}").as_bytes(), b"value")
                        .unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }

        let diagnostics = db.wal_diagnostics().unwrap();
        assert_eq!(diagnostics.logical_commit_count, writers as u64);
        assert_eq!(diagnostics.frame_count, writers as u64);
        assert!(diagnostics.physical_sync_count < diagnostics.logical_commit_count);
        assert!(diagnostics.max_group_size > 1);
        for index in 0..writers {
            assert_eq!(
                objects.get(format!("key-{index}").as_bytes()).unwrap(),
                Some(b"value".to_vec())
            );
        }
    }

    #[test]
    fn incomplete_wal_frame_is_truncated() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        objects.insert(b"a", b"one").unwrap();
        drop(objects);
        drop(db);
        let wal_path = directory.path().join(WAL_FILE);
        let length = std::fs::metadata(&wal_path).unwrap().len();
        let file = OpenOptions::new().write(true).open(wal_path).unwrap();
        file.set_len(length.saturating_sub(2)).unwrap();
        drop(file);
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        assert!(objects.get(b"a").unwrap().is_none());
    }

    #[test]
    fn wal_checksum_corruption_is_reported() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        objects.insert(b"a", b"one").unwrap();
        drop(objects);
        drop(db);

        let wal_path = directory.path().join(WAL_FILE);
        let mut bytes = std::fs::read(&wal_path).unwrap();
        *bytes.last_mut().unwrap() ^= 0xff;
        std::fs::write(&wal_path, bytes).unwrap();

        let Err(error) = Database::open(directory.path()) else {
            panic!("corrupted WAL unexpectedly opened")
        };
        assert!(error.to_string().contains("WAL checksum mismatch"));
    }

    #[test]
    fn table_checksum_corruption_is_reported() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        objects.insert(b"a", b"one").unwrap();
        db.compact().unwrap();
        drop(objects);
        drop(db);

        let manifest: Manifest =
            serde_json::from_slice(&std::fs::read(directory.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        let table_path = directory.path().join(manifest.table_file.unwrap());
        let mut bytes = std::fs::read(&table_path).unwrap();
        let first_record_start = TABLE_MAGIC.len() + 16;
        let key_length = usize::try_from(u64::from_be_bytes(
            bytes[first_record_start..first_record_start + 8]
                .try_into()
                .unwrap(),
        ))
        .unwrap();
        let value_length_offset = first_record_start + 8 + key_length + 1;
        let value_length = usize::try_from(u64::from_be_bytes(
            bytes[value_length_offset..value_length_offset + 8]
                .try_into()
                .unwrap(),
        ))
        .unwrap();
        let first_record_end = value_length_offset + 8 + value_length + 4;
        bytes[first_record_end - 1] ^= 0xff;
        std::fs::write(table_path, bytes).unwrap();

        let Err(error) = Database::open(directory.path()) else {
            panic!("corrupted table unexpectedly opened")
        };
        assert!(error.to_string().contains("table checksum mismatch"));
    }

    #[test]
    fn table_v3_footer_corruption_is_reported() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        objects.insert(b"a", b"one").unwrap();
        db.compact().unwrap();
        drop(objects);
        drop(db);

        let manifest: Manifest =
            serde_json::from_slice(&std::fs::read(directory.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        let table_path = directory.path().join(manifest.table_file.unwrap());
        let mut bytes = std::fs::read(&table_path).unwrap();
        assert_eq!(&bytes[..TABLE_MAGIC_V3.len()], TABLE_MAGIC_V3);
        *bytes.last_mut().unwrap() ^= 0xff;
        std::fs::write(table_path, bytes).unwrap();

        let Err(error) = Database::open(directory.path()) else {
            panic!("corrupted table footer unexpectedly opened")
        };
        assert!(error.to_string().contains("table footer checksum mismatch"));
    }

    #[test]
    fn table_v3_reopens_and_scans_multiple_sorted_blocks() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        for index in 0..2_000u32 {
            let key = format!("key-{index:04}");
            let value = format!("value-{index:04}-with-padding");
            objects.insert(key.as_bytes(), value.as_bytes()).unwrap();
        }
        db.compact().unwrap();
        drop(objects);
        drop(db);

        let manifest: Manifest =
            serde_json::from_slice(&std::fs::read(directory.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        let table_path = directory.path().join(manifest.table_file.unwrap());
        let bytes = std::fs::read(&table_path).unwrap();
        let footer_start = bytes
            .windows(TABLE_FOOTER_MAGIC.len())
            .position(|window| window == TABLE_FOOTER_MAGIC)
            .unwrap();
        let mut cursor = footer_start + TABLE_FOOTER_MAGIC.len();
        let block_count = read_u64(&bytes, &mut cursor).unwrap();
        assert!(block_count > 1);

        let reopened = Database::open(directory.path()).unwrap();
        let objects = reopened
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        let start: &[u8] = b"key-0100";
        let end: &[u8] = b"key-0109";
        let entries = objects
            .range::<&[u8], _>(start..=end)
            .into_result()
            .unwrap();
        assert_eq!(entries.len(), 10);
        assert_eq!(entries[0].value, b"value-0100-with-padding");
    }

    #[test]
    fn scan_checksum_corruption_is_not_treated_as_an_empty_result() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        objects.insert(b"a", b"one").unwrap();
        objects.insert(b"b", b"two").unwrap();
        db.compact().unwrap();

        let manifest: Manifest =
            serde_json::from_slice(&std::fs::read(directory.path().join(MANIFEST_FILE)).unwrap())
                .unwrap();
        let table_path = directory.path().join(manifest.table_file.unwrap());
        let mut bytes = std::fs::read(&table_path).unwrap();
        let first_record_start = TABLE_MAGIC.len() + 16;
        let key_length = usize::try_from(u64::from_be_bytes(
            bytes[first_record_start..first_record_start + 8]
                .try_into()
                .unwrap(),
        ))
        .unwrap();
        let value_length_offset = first_record_start + 8 + key_length + 1;
        let value_length = usize::try_from(u64::from_be_bytes(
            bytes[value_length_offset..value_length_offset + 8]
                .try_into()
                .unwrap(),
        ))
        .unwrap();
        let first_record_end = value_length_offset + 8 + value_length + 4;
        bytes[first_record_end - 1] ^= 0xff;
        std::fs::write(&table_path, bytes).unwrap();

        let error = objects.iter().into_result().unwrap_err();
        assert!(error.to_string().contains("table checksum mismatch"));
        let error = objects.prefix(b"a").into_result().unwrap_err();
        assert!(error.to_string().contains("table checksum mismatch"));
        let error = objects
            .range_bounds(Some((b"a", true)), Some((b"b", true)))
            .into_result()
            .unwrap_err();
        assert!(error.to_string().contains("table checksum mismatch"));
        let error = objects.first_prefix_after(b"a", None).unwrap_err();
        assert!(error.to_string().contains("table checksum mismatch"));
    }

    #[test]
    fn repeated_open_close_preserves_durable_state() {
        let directory = tempfile::tempdir().unwrap();
        for value in 0..8 {
            let db = Database::open(directory.path()).unwrap();
            let objects = db
                .keyspace("objects", KeyspaceCreateOptions::default)
                .unwrap();
            objects
                .insert(b"counter", value.to_string().as_bytes())
                .unwrap();
            if value % 2 == 1 {
                db.compact().unwrap();
            }
            drop(objects);
            drop(db);
        }

        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        assert_eq!(objects.get(b"counter").unwrap(), Some(b"7".to_vec()));
    }

    #[test]
    fn prefix_and_range_are_ordered() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        for key in [b"aa".as_slice(), b"ab", b"ba"] {
            objects.insert(key, b"value").unwrap();
        }
        let prefix: Vec<_> = objects.prefix(b"a").map(|entry| entry.key).collect();
        assert_eq!(prefix, vec![b"aa".to_vec(), b"ab".to_vec()]);
        let start: &[u8] = b"aa";
        let end: &[u8] = b"ab";
        let range: Vec<_> = objects
            .range::<&[u8], _>(start..=end)
            .map(|entry| entry.key)
            .collect();
        assert_eq!(range, vec![b"aa".to_vec(), b"ab".to_vec()]);
    }

    #[test]
    fn first_prefix_after_excludes_the_cursor_key() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        for key in [b"aa".as_slice(), b"ab", b"ac"] {
            objects.insert(key, b"value").unwrap();
        }
        db.compact().unwrap();

        assert_eq!(
            objects
                .first_prefix_after(b"a", Some(b"aa"))
                .unwrap()
                .map(|entry| entry.key),
            Some(b"ab".to_vec())
        );
        assert_eq!(
            objects
                .first_prefix_after(b"a", Some(b"ab"))
                .unwrap()
                .map(|entry| entry.key),
            Some(b"ac".to_vec())
        );
    }

    #[test]
    fn snapshot_is_consistent() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(directory.path()).unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        objects.insert(b"a", b"one").unwrap();
        let snapshot = db.snapshot().unwrap();
        objects.insert(b"a", b"two").unwrap();
        assert_eq!(
            snapshot.keyspace("objects").get(b"a"),
            Some(b"one".to_vec())
        );
    }

    #[cfg(feature = "benchmark")]
    #[test]
    fn rcu_memory_mode_preserves_atomic_batches_and_snapshots() {
        let db = Database::in_memory_with_rcu().unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        let events = db
            .keyspace("events", KeyspaceCreateOptions::default)
            .unwrap();
        db.batch()
            .put(&objects, b"a", b"one")
            .put(&events, b"e1", b"event")
            .commit()
            .unwrap();

        let snapshot = db.snapshot().unwrap();
        db.batch()
            .put(&objects, b"a", b"two")
            .delete(&events, b"e1")
            .commit()
            .unwrap();

        assert_eq!(objects.get(b"a").unwrap(), Some(b"two".to_vec()));
        assert_eq!(events.get(b"e1").unwrap(), None);
        assert_eq!(
            snapshot.keyspace("objects").get(b"a"),
            Some(b"one".to_vec())
        );
        assert_eq!(
            snapshot.keyspace("events").get(b"e1"),
            Some(b"event".to_vec())
        );
    }

    #[cfg(feature = "benchmark")]
    #[test]
    fn rcu_memory_mode_keeps_ordered_scans_and_shared_reads() {
        let db = Database::in_memory_with_rcu().unwrap();
        let objects = db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        for key in [b"aa".as_slice(), b"ab", b"ba"] {
            objects.insert(key, b"value").unwrap();
        }

        assert_eq!(
            objects
                .prefix(b"a")
                .into_result()
                .unwrap()
                .into_iter()
                .map(|entry| entry.key)
                .collect::<Vec<_>>(),
            vec![b"aa".to_vec(), b"ab".to_vec()]
        );
        assert_eq!(
            objects.get_shared(b"aa").unwrap().as_deref(),
            Some(&b"value".to_vec())
        );
    }
}
