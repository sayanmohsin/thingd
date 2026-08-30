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

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Seek, SeekFrom, Write};
use std::ops::{Bound, RangeBounds};
use std::path::{Component, Path, PathBuf};
use std::sync::{
    Arc, Mutex, Weak,
    mpsc::{self, RecvTimeoutError, SyncSender, TryRecvError},
};
use std::thread;
use std::time::{Duration, Instant};

use crc32fast::Hasher;
use serde::{Deserialize, Serialize};

const WAL_MAGIC: &[u8; 8] = b"TDBWAL01";
const TABLE_MAGIC: &[u8; 8] = b"TDBTAB01";
const TABLE_MAGIC_V2: &[u8; 8] = b"TDBTAB02";
const FORMAT_VERSION: u32 = 1;
const WAL_FILE: &str = "WAL";
const MANIFEST_FILE: &str = "MANIFEST.json";
const MANIFEST_TEMP_FILE: &str = ".MANIFEST.json.tmp";
const LOCK_FILE: &str = "LOCK";

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
    /// Nanoseconds spent appending WAL frames.
    pub append_duration_ns: u64,
    /// Total bytes appended to the WAL since opening the database.
    pub wal_bytes_appended: u64,
    /// Nanoseconds spent syncing WAL frames.
    pub sync_duration_ns: u64,
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
    /// Number of RAM mutations.
    pub mutation_count: u64,
    /// Nanoseconds spent applying RAM mutations.
    pub mutation_duration_ns: u64,
    /// Number of RAM iteration requests.
    pub iteration_count: u64,
    /// Nanoseconds spent materializing RAM iteration results.
    pub iteration_duration_ns: u64,
    /// Nanoseconds spent deserializing Thingd objects from RAM values.
    pub deserialization_duration_ns: u64,
    /// Number of RAM search operations.
    pub search_count: u64,
    /// Nanoseconds spent executing RAM searches.
    pub search_duration_ns: u64,
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
}

/// A shared ThingDB database handle.
#[derive(Clone)]
pub struct Database {
    inner: Arc<Mutex<Inner>>,
    writer: Arc<WriterCoordinator>,
}

struct Inner {
    path: PathBuf,
    wal: Option<File>,
    lock: Option<File>,
    in_memory: bool,
    state: BTreeMap<Vec<u8>, Vec<u8>>,
    memory_keyspaces: Option<BTreeMap<String, BTreeMap<Vec<u8>, Vec<u8>>>>,
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
    ram_diagnostics: RamDiagnostics,
    recovery_required: bool,
}

struct TableLayer {
    file: File,
    entries: Vec<TableIndexEntry>,
    is_v2: bool,
}

struct TableIndexEntry {
    key: Vec<u8>,
    offset: u64,
    length: u64,
}

impl Inner {
    fn get_value(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.diagnostics.pending_table_lookup_count = self
            .diagnostics
            .pending_table_lookup_count
            .saturating_add(1);
        if let Some(value) = self.pending_table.get(key) {
            return Ok(value.clone());
        }
        self.diagnostics.mutable_state_lookup_count = self
            .diagnostics
            .mutable_state_lookup_count
            .saturating_add(1);
        if let Some(value) = self.state.get(key) {
            return Ok(Some(value.clone()));
        }
        self.diagnostics.table_lookup_count = self.diagnostics.table_lookup_count.saturating_add(1);
        for layer in self.table_layers.iter_mut().rev() {
            self.diagnostics.immutable_layer_lookup_count = self
                .diagnostics
                .immutable_layer_lookup_count
                .saturating_add(1);
            self.diagnostics.table_layers_consulted =
                self.diagnostics.table_layers_consulted.saturating_add(1);
            let Ok(index) = layer
                .entries
                .binary_search_by(|entry| entry.key.as_slice().cmp(key))
            else {
                continue;
            };
            let started = Instant::now();
            let result = read_table_value(&mut layer.file, &layer.entries[index], layer.is_v2);
            self.diagnostics.table_bytes_read = self
                .diagnostics
                .table_bytes_read
                .saturating_add(layer.entries[index].length);
            self.diagnostics.table_read_duration_ns = self
                .diagnostics
                .table_read_duration_ns
                .saturating_add(elapsed_nanos(started.elapsed()));
            return result;
        }
        Ok(None)
    }

    /// Resolve one key while merging ordered scan sources.
    ///
    /// Scan resolution deliberately does not call `get_value`: the scan has
    /// already selected the next physical key from its source cursors, so a
    /// second point-lookup would repeat lookup work and make scan diagnostics
    /// indistinguishable from point-read diagnostics.
    fn get_scan_value(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        if let Some(value) = self.pending_table.get(key) {
            return value
                .as_ref()
                .map_or(Ok(None), |value| Ok(Some(value.clone())));
        }
        if let Some(value) = self.state.get(key) {
            return Ok(Some(value.clone()));
        }
        for layer in self.table_layers.iter_mut().rev() {
            self.diagnostics.scan_layers_consulted =
                self.diagnostics.scan_layers_consulted.saturating_add(1);
            let Ok(index) = layer
                .entries
                .binary_search_by(|entry| entry.key.as_slice().cmp(key))
            else {
                continue;
            };
            let value = read_table_value(&mut layer.file, &layer.entries[index], layer.is_v2)?;
            self.diagnostics.table_bytes_read = self
                .diagnostics
                .table_bytes_read
                .saturating_add(layer.entries[index].length);
            return Ok(value);
        }
        Ok(None)
    }

    fn materialize_state(&mut self) -> Result<BTreeMap<Vec<u8>, Vec<u8>>> {
        if let Some(keyspaces) = &self.memory_keyspaces {
            let mut state = BTreeMap::new();
            for (name, entries) in keyspaces {
                let namespace = namespace(name);
                for (key, value) in entries {
                    let mut physical = namespace.clone();
                    physical.extend_from_slice(key);
                    state.insert(physical, value.clone());
                }
            }
            return Ok(state);
        }
        let mut state = BTreeMap::new();
        for layer in &mut self.table_layers {
            for entry in &layer.entries {
                match read_table_value(&mut layer.file, entry, layer.is_v2)? {
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
    name: String,
    namespace: Vec<u8>,
}

/// A write batch applied atomically to all included keyspaces.
pub struct Batch {
    db: Database,
    operations: Vec<Operation>,
}

#[derive(Clone, Debug)]
enum Operation {
    Put { key: Vec<u8>, value: Vec<u8> },
    Delete { key: Vec<u8> },
}

struct CommitRequest {
    operations: Vec<Operation>,
    submitted_at: Instant,
    fault_point: Option<&'static str>,
    response: mpsc::Sender<Result<()>>,
}

struct WriterCoordinator {
    sender: Mutex<Option<SyncSender<CommitRequest>>>,
    handle: Mutex<Option<thread::JoinHandle<()>>>,
}

const WRITER_QUEUE_CAPACITY: usize = 1024;
const GROUP_COMMIT_WINDOW: Duration = Duration::from_millis(1);
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
    loop {
        let Some(first) = pending.take().or_else(|| receiver.recv().ok()) else {
            return;
        };
        let mut group = vec![first];
        let mut operation_count = group[0].operations.len();
        let mut operation_bytes = operations_bytes(&group[0].operations);
        // An isolated durable write should not pay the group-commit window.
        // Once another request is already queued, retain the bounded window
        // so concurrent writers can still share one WAL sync.
        let deadline = match receiver.try_recv() {
            Ok(request) => {
                let request_operations = request.operations.len();
                let request_bytes = operations_bytes(&request.operations);
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
                        let request_bytes = operations_bytes(&request.operations);
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

        process_group(&inner, group);
    }
}

fn operations_bytes(operations: &[Operation]) -> usize {
    operations
        .iter()
        .map(|operation| match operation {
            Operation::Put { key, value } => key.len().saturating_add(value.len()),
            Operation::Delete { key } => key.len(),
        })
        .sum()
}

fn request_has_fault(requests: &[CommitRequest], point: &'static str) -> Option<&'static str> {
    requests
        .iter()
        .find_map(|request| (request.fault_point == Some(point)).then_some(point))
}

fn process_group(inner: &Weak<Mutex<Inner>>, mut requests: Vec<CommitRequest>) {
    let Some(inner_arc) = inner.upgrade() else {
        for request in &mut *requests {
            let _ = request
                .response
                .send(Err(Error::message("ThingDB writer is unavailable")));
        }
        return;
    };
    let lock_started = Instant::now();
    let Ok(mut inner) = inner_arc.lock() else {
        drop(inner_arc);
        for request in requests {
            let _ = request
                .response
                .send(Err(Error::message("database lock poisoned")));
        }
        return;
    };

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
            .saturating_add(elapsed_nanos(lock_started.elapsed()));
        drop(inner);
        drop(inner_arc);
        finish_group_with_error(&mut requests, message);
        return;
    }

    let wal_start = if inner.in_memory {
        0
    } else if let Some(metadata) = inner.wal.as_ref().and_then(|wal| wal.metadata().ok()) {
        metadata.len()
    } else {
        let message = "ThingDB WAL is unavailable";
        inner.diagnostics.last_error = Some(message.to_string());
        inner.diagnostics.lock_duration_ns = inner
            .diagnostics
            .lock_duration_ns
            .saturating_add(elapsed_nanos(lock_started.elapsed()));
        drop(inner);
        drop(inner_arc);
        finish_group_with_error(&mut requests, message);
        return;
    };
    let (result, synced) = if inner.in_memory {
        execute_memory_group(&mut inner, &mut requests, group_size)
    } else {
        execute_group(&mut inner, &mut requests, wal_start, group_size)
    };

    if let Err(error) = &result {
        inner.diagnostics.last_error = Some(error.to_string());
        if synced {
            inner.recovery_required = true;
            inner.diagnostics.recovery_required = true;
        }
    }
    if let Some(wal) = inner.wal.as_ref()
        && let Ok(bytes) = wal.metadata().map(|metadata| metadata.len())
    {
        inner.diagnostics.journal_bytes = bytes;
        inner.diagnostics.wal_over_budget = bytes > inner.max_journaling_size;
    }
    inner.diagnostics.lock_duration_ns = inner
        .diagnostics
        .lock_duration_ns
        .saturating_add(elapsed_nanos(lock_started.elapsed()));
    drop(inner);
    drop(inner_arc);

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

fn execute_group(
    inner: &mut Inner,
    requests: &mut [CommitRequest],
    wal_start: u64,
    group_size: u64,
) -> (Result<()>, bool) {
    let mut next_sequence = inner.sequence.saturating_add(1);
    let mut frames = Vec::with_capacity(requests.len());
    let mut synced = false;
    let result: Result<()> = (|| {
        for request in requests.iter() {
            let started = Instant::now();
            let frame = encode_frame(next_sequence, &request.operations)?;
            inner.diagnostics.encode_duration_ns = inner
                .diagnostics
                .encode_duration_ns
                .saturating_add(elapsed_nanos(started.elapsed()));
            frames.push(frame);
            next_sequence = next_sequence.saturating_add(1);
        }

        if let Some(point) = request_has_fault(requests, "before-wal-append") {
            maybe_fail(point, Some(point))?;
        }
        let wal_bytes = frames.iter().map(Vec::len).sum::<usize>();
        let mut wal_payload = Vec::with_capacity(wal_bytes);
        for frame in &frames {
            wal_payload.extend_from_slice(frame);
        }
        let started = Instant::now();
        {
            let Some(wal) = inner.wal.as_mut() else {
                return Err(Error::message("ThingDB WAL is unavailable"));
            };
            wal.write_all(&wal_payload)?;
        }
        inner.diagnostics.append_duration_ns = inner
            .diagnostics
            .append_duration_ns
            .saturating_add(elapsed_nanos(started.elapsed()));
        inner.diagnostics.wal_bytes_appended = inner
            .diagnostics
            .wal_bytes_appended
            .saturating_add(wal_bytes as u64);

        if let Some(point) = request_has_fault(requests, "after-wal-write-before-sync") {
            maybe_fail(point, Some(point))?;
        }
        let started = Instant::now();
        inner.diagnostics.physical_sync_count =
            inner.diagnostics.physical_sync_count.saturating_add(1);
        {
            let Some(wal) = inner.wal.as_mut() else {
                return Err(Error::message("ThingDB WAL is unavailable"));
            };
            wal.sync_data()?;
        }
        synced = true;
        inner.diagnostics.sync_duration_ns = inner
            .diagnostics
            .sync_duration_ns
            .saturating_add(elapsed_nanos(started.elapsed()));

        if let Some(point) = request_has_fault(requests, "after-wal-sync-before-state-apply") {
            maybe_fail(point, Some(point))?;
        }
        let started = Instant::now();
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
        inner.diagnostics.frame_count = inner.diagnostics.frame_count.saturating_add(group_size);
        inner.diagnostics.memtable_bytes = inner.pending_table_bytes;
        inner.diagnostics.memtable_over_budget =
            inner.pending_table_bytes >= inner.max_memtable_bytes;
        if inner.pending_table_bytes >= inner.max_memtable_bytes {
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
        }
        Ok(())
    })();

    if result.is_err()
        && !synced
        && let Some(wal) = inner.wal.as_mut()
        && wal.set_len(wal_start).is_ok()
    {
        let _ = wal.seek(SeekFrom::End(0));
        inner.diagnostics.journal_bytes = wal_start;
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
}

/// A consistent read snapshot.
#[derive(Clone)]
pub struct Snapshot {
    state: Arc<BTreeMap<Vec<u8>, Vec<u8>>>,
}

impl Database {
    /// Create a true RAM-only ThingDB instance.
    ///
    /// This mode creates no files, WAL, manifest, table layers, or durable
    /// recovery state. All data is lost when the returned instance is dropped.
    pub fn in_memory() -> Result<Self> {
        let inner = Arc::new(Mutex::new(Inner {
            path: PathBuf::new(),
            wal: None,
            lock: None,
            in_memory: true,
            state: BTreeMap::new(),
            memory_keyspaces: Some(BTreeMap::new()),
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
            ram_diagnostics: RamDiagnostics::default(),
            recovery_required: false,
        }));
        let writer = WriterCoordinator::new(Arc::downgrade(&inner))?;
        Ok(Self { inner, writer })
    }

    /// Start building a database at `path`.
    pub fn builder(path: impl AsRef<Path>) -> DatabaseBuilder {
        DatabaseBuilder {
            path: path.as_ref().to_path_buf(),
            max_journaling_size: 32 * 1024 * 1024,
            max_memtable_bytes: 64 * 1024 * 1024,
            max_table_layers: 8,
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
            name: name.to_string(),
            namespace: namespace(name),
        })
    }

    /// Create an empty atomic batch.
    pub fn batch(&self) -> Batch {
        Batch {
            db: self.clone(),
            operations: Vec::new(),
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
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
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
        let inner = self
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        if inner.in_memory {
            Ok(inner.ram_diagnostics.clone())
        } else {
            Ok(RamDiagnostics::default())
        }
    }

    /// Record Thingd-layer deserialization time for RAM diagnostics.
    pub fn record_ram_deserialization(&self, duration_ns: u64) {
        if let Ok(mut inner) = self.inner.lock()
            && inner.in_memory
        {
            inner.ram_diagnostics.deserialization_duration_ns = inner
                .ram_diagnostics
                .deserialization_duration_ns
                .saturating_add(duration_ns);
        }
    }

    /// Record Thingd-layer search time for RAM diagnostics.
    pub fn record_ram_search(&self, duration_ns: u64) {
        if let Ok(mut inner) = self.inner.lock()
            && inner.in_memory
        {
            inner.ram_diagnostics.search_count =
                inner.ram_diagnostics.search_count.saturating_add(1);
            inner.ram_diagnostics.search_duration_ns = inner
                .ram_diagnostics
                .search_duration_ns
                .saturating_add(duration_ns);
        }
    }

    fn flush_table(&self, sync: bool) -> Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        flush_table_locked(&mut inner, sync, current_fault_point())
    }

    fn commit_operations(&self, operations: Vec<Operation>) -> Result<()> {
        if operations.is_empty() {
            return Ok(());
        }

        let in_memory = self
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?
            .in_memory;
        if in_memory {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| Error::message("database lock poisoned"))?;
            if inner.recovery_required {
                return Err(Error::message(
                    "ThingDB requires reopen and recovery before writing",
                ));
            }
            let started = Instant::now();
            let keyspaces = inner
                .memory_keyspaces
                .as_mut()
                .ok_or_else(|| Error::message("ThingDB RAM state is unavailable"))?;
            for operation in &operations {
                validate_memory_operation(operation)?;
            }
            let operation_count = operations.len() as u64;
            for operation in operations {
                apply_memory_operation(keyspaces, operation)?;
            }
            let elapsed = elapsed_nanos(started.elapsed());
            inner.ram_diagnostics.mutation_count = inner
                .ram_diagnostics
                .mutation_count
                .saturating_add(operation_count);
            inner.ram_diagnostics.mutation_duration_ns = inner
                .ram_diagnostics
                .mutation_duration_ns
                .saturating_add(elapsed);
            inner.sequence = inner.sequence.saturating_add(1);
            inner.diagnostics.logical_commit_count =
                inner.diagnostics.logical_commit_count.saturating_add(1);
            inner.diagnostics.total_group_size =
                inner.diagnostics.total_group_size.saturating_add(1);
            inner.diagnostics.max_group_size = inner.diagnostics.max_group_size.max(1);
            inner.diagnostics.state_apply_duration_ns = inner
                .diagnostics
                .state_apply_duration_ns
                .saturating_add(elapsed_nanos(started.elapsed()));
            return Ok(());
        }

        let (response, result) = mpsc::channel();
        let request = CommitRequest {
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
        let wal = inner
            .wal
            .as_mut()
            .ok_or_else(|| Error::message("ThingDB WAL is unavailable"))?;
        wal.sync_data()?;
        return Ok(());
    }
    let updates = std::mem::take(&mut inner.pending_table);
    let update_bytes = inner.pending_table_bytes;
    let result = (|| {
        let wal = inner
            .wal
            .as_mut()
            .ok_or_else(|| Error::message("ThingDB WAL is unavailable"))?;
        wal.sync_data()?;
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
        let (_, entries, is_v2) = read_table_index(&table_path)?;
        inner.table_layers.push(TableLayer {
            file: File::open(&table_path)?,
            entries,
            is_v2,
        });
        inner.diagnostics.table_layer_count = inner.table_layers.len() as u64;
        inner.table_sequence = next_sequence;
        inner.state.clear();
        inner.pending_table_bytes = 0;
        let wal = inner
            .wal
            .as_mut()
            .ok_or_else(|| Error::message("ThingDB WAL is unavailable"))?;
        wal.set_len(0)?;
        wal.seek(SeekFrom::End(0))?;
        if sync {
            wal.sync_data()?;
        }
        inner.diagnostics.journal_bytes = wal.metadata()?.len();
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
    let wal = inner
        .wal
        .as_mut()
        .ok_or_else(|| Error::message("ThingDB WAL is unavailable"))?;
    wal.sync_data()?;
    let next_sequence = inner.sequence;
    let table_name = format!("table-{next_sequence:020}-compact.tdb");
    let table_path = inner.path.join(&table_name);
    let temp_path = inner.path.join(format!(".{table_name}.tmp"));
    let input_bytes = inner
        .table_layers
        .iter()
        .flat_map(|layer| layer.entries.iter())
        .map(|entry| entry.length)
        .sum();
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
    let (_, entries, is_v2) = read_table_index(&table_path)?;
    inner.table_layers = vec![TableLayer {
        file: File::open(&table_path)?,
        entries,
        is_v2,
    }];
    inner.diagnostics.table_layer_count = 1;
    inner.table_sequence = next_sequence;
    inner.state.clear();
    inner.pending_table.clear();
    inner.pending_table_bytes = 0;
    let wal = inner
        .wal
        .as_mut()
        .ok_or_else(|| Error::message("ThingDB WAL is unavailable"))?;
    wal.set_len(0)?;
    wal.seek(SeekFrom::End(0))?;
    if sync {
        wal.sync_data()?;
    }
    inner.diagnostics.journal_bytes = wal.metadata()?.len();
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
    /// Set the soft WAL budget used for diagnostics and future backpressure.
    pub fn max_journaling_size(mut self, bytes: u64) -> Self {
        self.max_journaling_size = bytes;
        self
    }

    /// Set the maximum mutable table size before an automatic durable flush.
    ///
    /// A single commit may exceed this bound when its operation set is larger
    /// than the configured limit. The bound is otherwise enforced after the
    /// commit's WAL sync and before acknowledgement.
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
                let (sequence, entries, is_v2) = read_table_index(&table_path)?;
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
                    file,
                    entries,
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
        let inner = Arc::new(Mutex::new(Inner {
            path: self.path,
            wal: Some(wal),
            lock: Some(lock),
            in_memory: false,
            state,
            memory_keyspaces: None,
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
            ram_diagnostics: RamDiagnostics::default(),
            recovery_required: false,
        }));
        let writer = WriterCoordinator::new(Arc::downgrade(&inner))?;
        Ok(Database { inner, writer })
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

impl Keyspace {
    /// Read a value by user key.
    pub fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Vec<u8>>> {
        let key = key.as_ref();
        let lock_started = Instant::now();
        let mut inner = self
            .db
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        let lock_wait = elapsed_nanos(lock_started.elapsed());
        if inner.in_memory {
            let lookup_started = Instant::now();
            let value = inner
                .memory_keyspaces
                .as_ref()
                .and_then(|keyspaces| keyspaces.get(&self.name))
                .and_then(|keyspace| keyspace.get(key));
            let lookup_duration = elapsed_nanos(lookup_started.elapsed());
            let clone_started = Instant::now();
            let value = value.cloned();
            let clone_duration = elapsed_nanos(clone_started.elapsed());
            let held_duration = elapsed_nanos(lock_started.elapsed());
            let diagnostics = &mut inner.ram_diagnostics;
            diagnostics.lookup_count = diagnostics.lookup_count.saturating_add(1);
            diagnostics.lock_wait_duration_ns =
                diagnostics.lock_wait_duration_ns.saturating_add(lock_wait);
            diagnostics.lock_held_duration_ns = diagnostics
                .lock_held_duration_ns
                .saturating_add(held_duration);
            diagnostics.lookup_duration_ns = diagnostics
                .lookup_duration_ns
                .saturating_add(lookup_duration);
            diagnostics.value_clone_duration_ns = diagnostics
                .value_clone_duration_ns
                .saturating_add(clone_duration);
            return Ok(value);
        }
        let key_started = Instant::now();
        let physical = physical_key_from_namespace(&self.namespace, key);
        let key_duration = elapsed_nanos(key_started.elapsed());
        let result = inner.get_value(&physical);
        inner.ram_diagnostics.key_encode_duration_ns = inner
            .ram_diagnostics
            .key_encode_duration_ns
            .saturating_add(key_duration);
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
    #[allow(clippy::too_many_lines)]
    pub fn first_prefix_after(
        &self,
        prefix: impl AsRef<[u8]>,
        after: Option<&[u8]>,
    ) -> Result<Option<Entry>> {
        let prefix = prefix.as_ref();
        let mut inner = self
            .db
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        if inner.in_memory {
            let Some(keyspace) = inner
                .memory_keyspaces
                .as_ref()
                .and_then(|keyspaces| keyspaces.get(&self.name))
            else {
                return Ok(None);
            };
            let mut entries = after.map_or_else(
                || keyspace.range(prefix.to_vec()..),
                |after| keyspace.range((Bound::Excluded(after.to_vec()), Bound::Unbounded)),
            );
            return Ok(entries
                .next()
                .filter(|(key, _)| key.starts_with(prefix))
                .map(|(key, value)| Entry {
                    key: key.clone(),
                    value: value.clone(),
                }));
        }
        let namespace = self.namespace.clone();
        let lower = physical_key_from_namespace(&namespace, after.unwrap_or(prefix));
        let prefix_physical = physical_key_from_namespace(&namespace, prefix);
        let upper = successor(&prefix_physical);
        let mut state_key = next_map_key(
            &inner.state,
            &lower,
            upper.as_deref(),
            after.map(|_| lower.as_slice()),
        );
        let mut pending_key = next_map_key(
            &inner.pending_table,
            &lower,
            upper.as_deref(),
            after.map(|_| lower.as_slice()),
        );
        let mut layer_indices = inner
            .table_layers
            .iter()
            .map(|layer| {
                layer
                    .entries
                    .binary_search_by(|entry| entry.key.as_slice().cmp(lower.as_slice()))
                    .unwrap_or_else(|index| index)
            })
            .collect::<Vec<_>>();
        if after.is_some() {
            for (layer, index) in inner.table_layers.iter().zip(&mut layer_indices) {
                if layer
                    .entries
                    .get(*index)
                    .is_some_and(|entry| entry.key == lower)
                {
                    *index += 1;
                }
            }
        }

        loop {
            let mut next_key = state_key.clone();
            if pending_key.as_deref().is_some_and(|candidate| {
                next_key
                    .as_deref()
                    .is_none_or(|current| candidate < current)
            }) {
                next_key.clone_from(&pending_key);
            }
            for (layer, index) in inner.table_layers.iter().zip(&layer_indices) {
                if let Some(candidate) = layer.entries.get(*index).map(|entry| &entry.key)
                    && next_key
                        .as_deref()
                        .is_none_or(|current| candidate.as_slice() < current)
                {
                    next_key = Some(candidate.clone());
                }
            }
            let Some(key) = next_key else {
                return Ok(None);
            };
            if state_key.as_deref() == Some(key.as_slice()) {
                state_key = next_map_key(&inner.state, &lower, upper.as_deref(), Some(&key));
            }
            if pending_key.as_deref() == Some(key.as_slice()) {
                pending_key =
                    next_map_key(&inner.pending_table, &lower, upper.as_deref(), Some(&key));
            }
            for (layer, index) in inner.table_layers.iter().zip(&mut layer_indices) {
                if layer
                    .entries
                    .get(*index)
                    .is_some_and(|entry| entry.key == key)
                {
                    *index += 1;
                }
            }
            inner.diagnostics.scan_keys_examined =
                inner.diagnostics.scan_keys_examined.saturating_add(1);
            let Some(value) = inner.get_scan_value(&key)? else {
                continue;
            };
            let Some(user_key) = key.strip_prefix(namespace.as_slice()) else {
                continue;
            };
            return Ok(Some(Entry {
                key: user_key.to_vec(),
                value,
            }));
        }
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
        let Ok(mut inner) = self.db.inner.lock() else {
            return Iter {
                entries: Vec::new().into_iter(),
            };
        };
        if inner.in_memory {
            return iter_memory_bounds(&mut inner, &self.name, prefix, start, end);
        }
        let namespace = self.namespace.clone();
        let scan_started = Instant::now();
        let lower = start
            .as_ref()
            .map(|(key, _)| physical_key_from_namespace(&namespace, key))
            .unwrap_or_else(|| namespace.clone());
        let upper = end
            .as_ref()
            .map(|(key, _)| physical_key_from_namespace(&namespace, key))
            .and_then(|key| successor(&key))
            .or_else(|| successor(&namespace));
        let mut entries = Vec::new();
        let mut state_key = next_map_key(&inner.state, &lower, upper.as_deref(), None);
        let mut pending_key = next_map_key(&inner.pending_table, &lower, upper.as_deref(), None);
        let mut layer_indices = inner
            .table_layers
            .iter()
            .map(|layer| {
                layer
                    .entries
                    .binary_search_by(|entry| entry.key.as_slice().cmp(lower.as_slice()))
                    .unwrap_or_else(|index| index)
            })
            .collect::<Vec<_>>();

        loop {
            let mut next_key = state_key.clone();
            if pending_key.as_deref().is_some_and(|candidate| {
                next_key
                    .as_deref()
                    .is_none_or(|current| candidate < current)
            }) {
                next_key.clone_from(&pending_key);
            }
            for (layer, index) in inner.table_layers.iter().zip(&layer_indices) {
                if let Some(candidate) = layer.entries.get(*index).map(|entry| &entry.key)
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
                state_key = next_map_key(&inner.state, &lower, upper.as_deref(), Some(&key));
            }
            if pending_key.as_deref() == Some(key.as_slice()) {
                pending_key =
                    next_map_key(&inner.pending_table, &lower, upper.as_deref(), Some(&key));
            }
            for (layer, index) in inner.table_layers.iter().zip(&mut layer_indices) {
                if layer
                    .entries
                    .get(*index)
                    .is_some_and(|entry| entry.key == key)
                {
                    *index += 1;
                }
            }
            inner.diagnostics.scan_keys_examined =
                inner.diagnostics.scan_keys_examined.saturating_add(1);
            let Some(value) = inner.get_scan_value(&key).unwrap_or(None) else {
                continue;
            };
            let Some(user_key) = key.strip_prefix(namespace.as_slice()) else {
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
        }
        inner.diagnostics.scan_count = inner.diagnostics.scan_count.saturating_add(1);
        inner.diagnostics.scan_duration_ns = inner
            .diagnostics
            .scan_duration_ns
            .saturating_add(elapsed_nanos(scan_started.elapsed()));
        Iter {
            entries: entries.into_iter(),
        }
    }
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

fn iter_memory_bounds(
    inner: &mut Inner,
    name: &str,
    prefix: Option<&[u8]>,
    start: Option<(Vec<u8>, bool)>,
    end: Option<(Vec<u8>, bool)>,
) -> Iter {
    let iteration_started = Instant::now();
    let mut entries = Vec::new();
    if let Some(keyspace) = inner
        .memory_keyspaces
        .as_ref()
        .and_then(|keyspaces| keyspaces.get(name))
    {
        for (key, value) in keyspace {
            if let Some(prefix) = prefix
                && !key.starts_with(prefix)
            {
                continue;
            }
            if let Some((start, inclusive)) = &start
                && if *inclusive {
                    key.as_slice() < start.as_slice()
                } else {
                    key.as_slice() <= start.as_slice()
                }
            {
                continue;
            }
            if let Some((end, inclusive)) = &end
                && if *inclusive {
                    key.as_slice() > end.as_slice()
                } else {
                    key.as_slice() >= end.as_slice()
                }
            {
                continue;
            }
            entries.push(Entry {
                key: key.clone(),
                value: value.clone(),
            });
        }
    }
    inner.ram_diagnostics.iteration_count = inner.ram_diagnostics.iteration_count.saturating_add(1);
    inner.ram_diagnostics.iteration_duration_ns = inner
        .ram_diagnostics
        .iteration_duration_ns
        .saturating_add(elapsed_nanos(iteration_started.elapsed()));
    Iter {
        entries: entries.into_iter(),
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
        self.operations.push(Operation::Put {
            key: physical_key_from_namespace(&keyspace.namespace, key.as_ref()),
            value: value.as_ref().to_vec(),
        });
        self
    }

    /// Add a deletion to the batch.
    pub fn delete(mut self, keyspace: &Keyspace, key: impl AsRef<[u8]>) -> Self {
        self.operations.push(Operation::Delete {
            key: physical_key_from_namespace(&keyspace.namespace, key.as_ref()),
        });
        self
    }

    /// Commit all operations atomically and durably.
    pub fn commit(self) -> Result<()> {
        self.db.commit_operations(self.operations)
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

fn encode_frame(sequence: u64, operations: &[Operation]) -> Result<Vec<u8>> {
    let mut payload = Vec::new();
    write_u32(
        &mut payload,
        operations
            .len()
            .try_into()
            .map_err(|_| Error::message("too many operations in batch"))?,
    );
    for operation in operations {
        match operation {
            Operation::Put { key, value } => {
                payload.push(1);
                write_bytes(&mut payload, key)?;
                write_bytes(&mut payload, value)?;
            },
            Operation::Delete { key } => {
                payload.push(2);
                write_bytes(&mut payload, key)?;
                write_u64(&mut payload, 0);
            },
        }
    }
    let mut body = Vec::new();
    write_u64(&mut body, sequence);
    body.extend_from_slice(&payload);
    let checksum = checksum(&body);
    let frame_len: u64 = body
        .len()
        .checked_add(4)
        .and_then(|length| length.try_into().ok())
        .ok_or_else(|| Error::message("WAL frame is too large"))?;
    let mut frame = Vec::new();
    frame.extend_from_slice(WAL_MAGIC);
    write_u64(&mut frame, frame_len);
    frame.extend_from_slice(&body);
    write_u32(&mut frame, checksum);
    Ok(frame)
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
    keyspaces: &mut BTreeMap<String, BTreeMap<Vec<u8>, Vec<u8>>>,
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
    let entries = keyspaces.entry(name.to_string()).or_default();
    match value {
        Some(value) => {
            entries.insert(user_key.to_vec(), value);
        },
        None => {
            entries.remove(user_key);
        },
    }
    Ok(())
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
            state.insert(key.clone(), value.clone());
            if let Some(previous) = pending.insert(key.clone(), Some(value.clone())) {
                *pending_bytes =
                    pending_bytes.saturating_sub(pending_entry_bytes(&key, previous.as_ref()));
            }
            *pending_bytes = pending_bytes.saturating_add(pending_entry_bytes(&key, Some(&value)));
        },
        Operation::Delete { key } => {
            state.remove(&key);
            if let Some(previous) = pending.insert(key.clone(), None) {
                *pending_bytes =
                    pending_bytes.saturating_sub(pending_entry_bytes(&key, previous.as_ref()));
            }
            *pending_bytes = pending_bytes.saturating_add(pending_entry_bytes(&key, None));
        },
    }
}

fn pending_entry_bytes(key: &[u8], value: Option<&Vec<u8>>) -> u64 {
    (key.len() as u64)
        .saturating_add(1)
        .saturating_add(value.map_or(0, |value| value.len() as u64))
}

fn write_table(
    path: &Path,
    sequence: u64,
    entries: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    sync: bool,
) -> Result<()> {
    let mut file = File::create(path)?;
    file.write_all(TABLE_MAGIC_V2)?;
    write_u64_to(&mut file, sequence)?;
    write_u64_to(
        &mut file,
        entries
            .len()
            .try_into()
            .map_err(|_| Error::message("too many entries in ThingDB table"))?,
    )?;
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
        file.write_all(&record)?;
    }
    if sync {
        file.sync_all()?;
    }
    Ok(())
}

fn read_table_index(path: &Path) -> Result<(u64, Vec<TableIndexEntry>, bool)> {
    let bytes = fs::read(path)?;
    if bytes.len() < TABLE_MAGIC.len() + 16
        || (&bytes[..TABLE_MAGIC.len()] != TABLE_MAGIC
            && &bytes[..TABLE_MAGIC_V2.len()] != TABLE_MAGIC_V2)
    {
        return Err(Error::message("invalid ThingDB table"));
    }
    let version = &bytes[..TABLE_MAGIC.len()];
    let is_v2 = version == TABLE_MAGIC_V2;
    let mut cursor = TABLE_MAGIC.len();
    let sequence = read_u64(&bytes, &mut cursor)?;
    let count = read_u64(&bytes, &mut cursor)? as usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
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
    if cursor != bytes.len() {
        return Err(Error::message("trailing bytes in ThingDB table"));
    }
    Ok((sequence, entries, is_v2))
}

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

fn write_u64_to(output: &mut File, value: u64) -> Result<()> {
    output.write_all(&value.to_be_bytes())?;
    Ok(())
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
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        std::fs::write(table_path, bytes).unwrap();

        let Err(error) = Database::open(directory.path()) else {
            panic!("corrupted table unexpectedly opened")
        };
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
}
