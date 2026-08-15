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

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Seek, SeekFrom, Write};
use std::ops::{Bound, RangeBounds};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crc32fast::Hasher;
use serde::{Deserialize, Serialize};

const WAL_MAGIC: &[u8; 8] = b"TDBWAL01";
const TABLE_MAGIC: &[u8; 8] = b"TDBTAB01";
const FORMAT_VERSION: u32 = 1;
const WAL_FILE: &str = "WAL";
const MANIFEST_FILE: &str = "MANIFEST.json";
const LOCK_FILE: &str = "LOCK";

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
}

/// A shared ThingDB database handle.
#[derive(Clone)]
pub struct Database {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    path: PathBuf,
    wal: File,
    lock: File,
    state: BTreeMap<Vec<u8>, Vec<u8>>,
    sequence: u64,
    table_sequence: u64,
    max_journaling_size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    format_version: u32,
    table_file: Option<String>,
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
        self.flush_table(true)
    }

    /// Return a consistent snapshot of all keyspaces.
    pub fn snapshot(&self) -> Result<Snapshot> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        Ok(Snapshot {
            state: Arc::new(inner.state.clone()),
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

    fn flush_table(&self, sync: bool) -> Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        inner.wal.sync_data()?;
        let next_sequence = inner.sequence;
        let table_name = format!("table-{next_sequence:020}.tdb");
        let table_path = inner.path.join(&table_name);
        let temp_path = inner.path.join(format!(".{table_name}.tmp"));
        write_table(&temp_path, next_sequence, &inner.state, sync)?;
        fs::rename(&temp_path, &table_path)?;
        let manifest = Manifest {
            format_version: FORMAT_VERSION,
            table_file: Some(table_name),
            table_sequence: next_sequence,
        };
        write_manifest(&inner.path, &manifest, sync)?;
        inner.table_sequence = next_sequence;
        inner.wal.set_len(0)?;
        inner.wal.seek(SeekFrom::End(0))?;
        if sync {
            inner.wal.sync_data()?;
        }
        Ok(())
    }
}

impl DatabaseBuilder {
    /// Set the soft WAL budget used for diagnostics and future backpressure.
    pub fn max_journaling_size(mut self, bytes: u64) -> Self {
        self.max_journaling_size = bytes;
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

    fn open_locked(self, lock: File) -> Result<Database> {
        let manifest_path = self.path.join(MANIFEST_FILE);
        let manifest = if manifest_path.exists() {
            let manifest: Manifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
            if manifest.format_version != FORMAT_VERSION {
                return Err(Error::message(format!(
                    "unsupported ThingDB format version {}",
                    manifest.format_version
                )));
            }
            Some(manifest)
        } else {
            None
        };
        let mut state = BTreeMap::new();
        let mut table_sequence = 0;
        if let Some(manifest) = &manifest
            && let Some(table_file) = &manifest.table_file
        {
            let (sequence, table) = read_table(&self.path.join(table_file))?;
            if sequence != manifest.table_sequence {
                return Err(Error::message("table sequence does not match manifest"));
            }
            table_sequence = sequence;
            state = table;
        }
        let wal_path = self.path.join(WAL_FILE);
        let mut wal = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&wal_path)?;
        let (last_sequence, valid_offset) = replay_wal(&mut wal, table_sequence, &mut state)?;
        let wal_len = wal.metadata()?.len();
        if valid_offset < wal_len {
            wal.set_len(valid_offset)?;
            wal.seek(SeekFrom::End(0))?;
        }
        let sequence = last_sequence.max(table_sequence);
        if manifest.is_none() {
            write_manifest(
                &self.path,
                &Manifest {
                    format_version: FORMAT_VERSION,
                    table_file: None,
                    table_sequence: 0,
                },
                true,
            )?;
        }
        Ok(Database {
            inner: Arc::new(Mutex::new(Inner {
                path: self.path,
                wal,
                lock,
                state,
                sequence,
                table_sequence,
                max_journaling_size: self.max_journaling_size,
            })),
        })
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
        Ok(inner
            .state
            .get(&physical_key(&self.name, key.as_ref()))
            .cloned())
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
        let range = match upper {
            Some(upper) => inner.state.range(lower..upper),
            None => inner.state.range(lower..),
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
        let mut inner = self
            .db
            .inner
            .lock()
            .map_err(|_| Error::message("database lock poisoned"))?;
        let sequence = inner.sequence.saturating_add(1);
        let frame = encode_frame(sequence, &self.operations)?;
        inner.wal.write_all(&frame)?;
        inner.wal.sync_data()?;
        for operation in &self.operations {
            match operation {
                Operation::Put { key, value } => {
                    inner.state.insert(key.clone(), value.clone());
                },
                Operation::Delete { key } => {
                    inner.state.remove(key);
                },
            }
        }
        inner.sequence = sequence;
        if inner.wal.metadata()?.len() > inner.max_journaling_size {
            // The next explicit persist/compact performs the bounded table
            // rewrite. Do not compact synchronously on the write path.
        }
        Ok(())
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
) -> Result<(u64, u64)> {
    wal.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    wal.read_to_end(&mut bytes)?;
    let mut cursor = 0usize;
    let mut last_sequence = table_sequence;
    while cursor < bytes.len() {
        let frame_start = cursor;
        if bytes.len() - cursor < WAL_MAGIC.len() + 8 {
            return Ok((last_sequence, frame_start as u64));
        }
        if &bytes[cursor..cursor + WAL_MAGIC.len()] != WAL_MAGIC {
            return Err(Error::message("invalid ThingDB WAL magic"));
        }
        cursor += WAL_MAGIC.len();
        let frame_len = read_u64(&bytes, &mut cursor)? as usize;
        if frame_len < 12 || frame_len > bytes.len().saturating_sub(cursor) {
            return Ok((last_sequence, frame_start as u64));
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
        cursor = frame_end;
    }
    Ok((last_sequence, cursor as u64))
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

fn write_table(
    path: &Path,
    sequence: u64,
    state: &BTreeMap<Vec<u8>, Vec<u8>>,
    sync: bool,
) -> Result<()> {
    let mut file = File::create(path)?;
    file.write_all(TABLE_MAGIC)?;
    write_u64_to(&mut file, sequence)?;
    write_u64_to(
        &mut file,
        state
            .len()
            .try_into()
            .map_err(|_| Error::message("too many entries in ThingDB table"))?,
    )?;
    for (key, value) in state {
        let mut record = Vec::new();
        write_bytes(&mut record, key)?;
        write_bytes(&mut record, value)?;
        let record_checksum = checksum(&record);
        write_u32(&mut record, record_checksum);
        file.write_all(&record)?;
    }
    if sync {
        file.sync_all()?;
    }
    Ok(())
}

fn read_table(path: &Path) -> Result<(u64, BTreeMap<Vec<u8>, Vec<u8>>)> {
    let bytes = fs::read(path)?;
    if bytes.len() < TABLE_MAGIC.len() + 16 || &bytes[..TABLE_MAGIC.len()] != TABLE_MAGIC {
        return Err(Error::message("invalid ThingDB table"));
    }
    let mut cursor = TABLE_MAGIC.len();
    let sequence = read_u64(&bytes, &mut cursor)?;
    let count = read_u64(&bytes, &mut cursor)? as usize;
    let mut state = BTreeMap::new();
    for _ in 0..count {
        let key = read_bytes(&bytes, &mut cursor)?;
        let value = read_bytes(&bytes, &mut cursor)?;
        let stored = read_u32(&bytes, &mut cursor)?;
        let record_start = cursor
            .checked_sub(20 + key.len() + value.len())
            .ok_or_else(|| Error::message("invalid ThingDB table record"))?;
        let actual = checksum(&bytes[record_start..cursor - 4]);
        if stored != actual {
            return Err(Error::message("ThingDB table checksum mismatch"));
        }
        state.insert(key, value);
    }
    if cursor != bytes.len() {
        return Err(Error::message("trailing bytes in ThingDB table"));
    }
    Ok((sequence, state))
}

fn write_manifest(path: &Path, manifest: &Manifest, sync: bool) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(manifest)?;
    let temp = path.join(".MANIFEST.json.tmp");
    let final_path = path.join(MANIFEST_FILE);
    let mut file = File::create(&temp)?;
    file.write_all(&bytes)?;
    if sync {
        file.sync_all()?;
    }
    fs::rename(temp, final_path)?;
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
