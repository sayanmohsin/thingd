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

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Seek, SeekFrom, Write};
use std::ops::{Bound, RangeBounds};
use std::path::{Component, Path, PathBuf};
use std::sync::{
    Arc, Mutex, Weak,
    mpsc::{self, RecvTimeoutError, SyncSender},
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
}

/// A shared ThingDB database handle.
#[derive(Clone)]
pub struct Database {
    inner: Arc<Mutex<Inner>>,
    writer: Arc<WriterCoordinator>,
}

struct Inner {
    path: PathBuf,
    wal: File,
    lock: File,
    state: BTreeMap<Vec<u8>, Vec<u8>>,
    sequence: u64,
    table_sequence: u64,
    table_files: Vec<String>,
    table_layers: Vec<TableLayer>,
    pending_table: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    pending_table_bytes: u64,
    max_journaling_size: u64,
    max_memtable_bytes: u64,
    diagnostics: WalDiagnostics,
    recovery_required: bool,
}

struct TableLayer {
    path: PathBuf,
    entries: Vec<TableIndexEntry>,
    is_v2: bool,
}

struct TableIndexEntry {
    key: Vec<u8>,
    offset: u64,
    length: u64,
}

impl Inner {
    fn get_value(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        if let Some(value) = self.pending_table.get(key) {
            return Ok(value.clone());
        }
        if let Some(value) = self.state.get(key) {
            return Ok(Some(value.clone()));
        }
        for layer in self.table_layers.iter().rev() {
            let Ok(index) = layer
                .entries
                .binary_search_by(|entry| entry.key.as_slice().cmp(key))
            else {
                continue;
            };
            return read_table_value(&layer.path, &layer.entries[index], layer.is_v2);
        }
        Ok(None)
    }

    fn materialize_state(&self) -> Result<BTreeMap<Vec<u8>, Vec<u8>>> {
        let mut state = BTreeMap::new();
        for layer in &self.table_layers {
            for entry in &layer.entries {
                match read_table_value(&layer.path, entry, layer.is_v2)? {
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
        let deadline = Instant::now() + GROUP_COMMIT_WINDOW;

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
                    if !group.is_empty()
                        && (operation_count + request_operations > MAX_GROUP_OPERATIONS
                            || operation_bytes + request_bytes > MAX_GROUP_BYTES)
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

    let wal_start = match inner.wal.metadata() {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            drop(inner);
            drop(inner_arc);
            finish_group_with_error(&mut requests, &error.to_string());
            return;
        },
    };
    let (result, synced) = execute_group(&mut inner, &mut requests, wal_start, group_size);

    if let Err(error) = &result {
        inner.diagnostics.last_error = Some(error.to_string());
        if synced {
            inner.recovery_required = true;
            inner.diagnostics.recovery_required = true;
        }
    }
    if let Ok(bytes) = inner.wal.metadata().map(|metadata| metadata.len()) {
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
        let started = Instant::now();
        for frame in &frames {
            inner.wal.write_all(frame)?;
        }
        inner.diagnostics.append_duration_ns = inner
            .diagnostics
            .append_duration_ns
            .saturating_add(elapsed_nanos(started.elapsed()));

        if let Some(point) = request_has_fault(requests, "after-wal-write-before-sync") {
            maybe_fail(point, Some(point))?;
        }
        let started = Instant::now();
        inner.diagnostics.physical_sync_count =
            inner.diagnostics.physical_sync_count.saturating_add(1);
        inner.wal.sync_data()?;
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
        }
        Ok(())
    })();

    if result.is_err() && !synced && inner.wal.set_len(wal_start).is_ok() {
        let _ = inner.wal.seek(SeekFrom::End(0));
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
    /// Start building a database at `path`.
    pub fn builder(path: impl AsRef<Path>) -> DatabaseBuilder {
        DatabaseBuilder {
            path: path.as_ref().to_path_buf(),
            max_journaling_size: 32 * 1024 * 1024,
            max_memtable_bytes: 64 * 1024 * 1024,
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
        match mode {
            PersistMode::SyncAll => self.flush_table(true),
        }
    }

    /// Compact the database into a new immutable table.
    pub fn compact(&self) -> Result<()> {
        self.compact_tables(true)
    }

    /// Return a consistent snapshot of all keyspaces.
    pub fn snapshot(&self) -> Result<Snapshot> {
        let inner = self
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
        Ok(inner.wal.metadata()?.len())
    }

    /// Return the number of WAL files currently present.
    pub fn journal_count(&self) -> usize {
        usize::from(self.inner.lock().is_ok())
    }

    /// Return bounded WAL and recovery diagnostics.
    pub fn wal_diagnostics(&self) -> Result<WalDiagnostics> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        let mut diagnostics = inner.diagnostics.clone();
        diagnostics.journal_bytes = inner.wal.metadata()?.len();
        Ok(diagnostics)
    }

    fn flush_table(&self, sync: bool) -> Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        flush_table_locked(&mut inner, sync, current_fault_point())
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
        inner.wal.sync_data()?;
        return Ok(());
    }
    let updates = std::mem::take(&mut inner.pending_table);
    let update_bytes = inner.pending_table_bytes;
    let result = (|| {
        inner.wal.sync_data()?;
        let next_sequence = inner.sequence;
        let table_name = format!(
            "table-{next_sequence:020}-{:04}.tdb",
            inner.table_files.len()
        );
        let table_path = inner.path.join(&table_name);
        let temp_path = inner.path.join(format!(".{table_name}.tmp"));
        maybe_fail("before-table-write", fault_point)?;
        write_table(&temp_path, next_sequence, &updates, sync)?;
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
            path: table_path,
            entries,
            is_v2,
        });
        inner.table_sequence = next_sequence;
        inner.state.clear();
        inner.pending_table_bytes = 0;
        inner.wal.set_len(0)?;
        inner.wal.seek(SeekFrom::End(0))?;
        if sync {
            inner.wal.sync_data()?;
        }
        inner.diagnostics.journal_bytes = inner.wal.metadata()?.len();
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
        if inner.recovery_required {
            return Err(Error::message(
                "ThingDB requires reopen and recovery before writing",
            ));
        }
        inner.wal.sync_data()?;
        let next_sequence = inner.sequence;
        let table_name = format!("table-{next_sequence:020}-compact.tdb");
        let table_path = inner.path.join(&table_name);
        let temp_path = inner.path.join(format!(".{table_name}.tmp"));
        let entries: BTreeMap<_, _> = inner
            .materialize_state()?
            .into_iter()
            .map(|(key, value)| (key, Some(value)))
            .collect();
        maybe_fail("before-table-write", current_fault_point())?;
        write_table(&temp_path, next_sequence, &entries, sync)?;
        maybe_fail("after-table-sync-before-rename", current_fault_point())?;
        maybe_fail("before-table-rename", current_fault_point())?;
        fs::rename(&temp_path, &table_path)?;
        maybe_fail("after-table-rename-before-manifest", current_fault_point())?;
        let manifest = Manifest {
            format_version: FORMAT_VERSION,
            table_file: Some(table_name.clone()),
            table_files: vec![table_name.clone()],
            table_sequence: next_sequence,
        };
        maybe_fail("before-manifest-write", current_fault_point())?;
        write_manifest(&inner.path, &manifest, sync)?;
        maybe_fail(
            "after-manifest-rename-before-wal-truncate",
            current_fault_point(),
        )?;
        for old_table in &inner.table_files {
            if old_table != &table_name {
                let _ = fs::remove_file(inner.path.join(old_table));
            }
        }
        inner.table_files = vec![table_name];
        let (_, entries, is_v2) = read_table_index(&table_path)?;
        inner.table_layers = vec![TableLayer {
            path: table_path,
            entries,
            is_v2,
        }];
        inner.table_sequence = next_sequence;
        inner.state.clear();
        inner.pending_table.clear();
        inner.pending_table_bytes = 0;
        inner.wal.set_len(0)?;
        inner.wal.seek(SeekFrom::End(0))?;
        if sync {
            inner.wal.sync_data()?;
        }
        inner.diagnostics.journal_bytes = inner.wal.metadata()?.len();
        inner.diagnostics.frame_count = 0;
        inner.diagnostics.memtable_bytes = 0;
        inner.diagnostics.memtable_over_budget = false;
        Ok(())
    }
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
                table_layers.push(TableLayer {
                    path: table_path,
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
            wal,
            lock,
            state,
            sequence,
            table_sequence,
            table_files,
            table_layers,
            pending_table: BTreeMap::new(),
            pending_table_bytes: 0,
            max_journaling_size: self.max_journaling_size,
            max_memtable_bytes: self.max_memtable_bytes,
            diagnostics: WalDiagnostics {
                journal_bytes,
                frame_count,
                recovery_bytes: valid_offset,
                recovery_duration_ns,
                ..WalDiagnostics::default()
            },
            recovery_required: false,
        }));
        let writer = WriterCoordinator::new(Arc::downgrade(&inner))?;
        Ok(Database { inner, writer })
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let _ = self.lock.sync_all();
        let _ = fs::remove_file(self.path.join(LOCK_FILE));
    }
}

impl Keyspace {
    /// Read a value by user key.
    pub fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Vec<u8>>> {
        let inner = self
            .db
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        inner.get_value(&physical_key(&self.name, key.as_ref()))
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

    fn iter_bounds(
        &self,
        prefix: Option<&[u8]>,
        start: Option<(Vec<u8>, bool)>,
        end: Option<(Vec<u8>, bool)>,
    ) -> Iter {
        let Ok(inner) = self.db.inner.lock() else {
            return Iter {
                entries: Vec::new().into_iter(),
            };
        };
        let namespace = namespace(&self.name);
        let lower = start
            .as_ref()
            .map(|(key, _)| physical_key(&self.name, key))
            .unwrap_or_else(|| namespace.clone());
        let upper = end
            .as_ref()
            .map(|(key, _)| physical_key(&self.name, key))
            .and_then(|key| successor(&key))
            .or_else(|| successor(&namespace));
        let mut entries = Vec::new();
        let state = inner.materialize_state().unwrap_or_default();
        let range = match upper {
            Some(upper) => state.range(lower..upper),
            None => state.range(lower..),
        };
        for (key, value) in range {
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
                value: value.clone(),
            });
        }
        Iter {
            entries: entries.into_iter(),
        }
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
            key: physical_key(&keyspace.name, key.as_ref()),
            value: value.as_ref().to_vec(),
        });
        self
    }

    /// Add a deletion to the batch.
    pub fn delete(mut self, keyspace: &Keyspace, key: impl AsRef<[u8]>) -> Self {
        self.operations.push(Operation::Delete {
            key: physical_key(&keyspace.name, key.as_ref()),
        });
        self
    }

    /// Commit all operations atomically and durably.
    pub fn commit(self) -> Result<()> {
        if self.operations.is_empty() {
            return Ok(());
        }
        let (response, result) = mpsc::channel();
        let request = CommitRequest {
            operations: self.operations,
            submitted_at: Instant::now(),
            fault_point: current_fault_point(),
            response,
        };
        let sender = self
            .db
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
    let mut physical = namespace(name);
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
    if cursor != bytes.len() {
        return Err(Error::message("trailing bytes in ThingDB table"));
    }
    Ok((sequence, entries, is_v2))
}

fn read_table_value(path: &Path, entry: &TableIndexEntry, is_v2: bool) -> Result<Option<Vec<u8>>> {
    let mut file = File::open(path)?;
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
