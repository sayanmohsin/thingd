//! Backend-neutral durable keyspace adapter.

#![allow(
    clippy::cast_possible_truncation,
    clippy::doc_markdown,
    clippy::map_unwrap_or,
    clippy::missing_const_for_fn,
    clippy::needless_collect,
    clippy::needless_pass_by_value,
    clippy::redundant_pub_crate,
    clippy::unnecessary_wraps
)]

use std::fmt::{Display, Formatter};
use std::ops::{Bound, RangeBounds};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rocksdb::{
    ColumnFamilyDescriptor, DB, FlushOptions, IteratorMode, Options, WriteBatch, WriteOptions,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StorageBackend {
    RocksDb,
    ThingDb,
}

#[derive(Debug)]
pub(crate) struct Error(String);

impl Display for Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<rocksdb::Error> for Error {
    fn from(error: rocksdb::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<String> for Error {
    fn from(error: String) -> Self {
        Self(error)
    }
}

#[derive(Clone)]
pub(crate) struct Database {
    db: Arc<Backend>,
    path: PathBuf,
}

enum Backend {
    RocksDb(Arc<DB>),
    ThingDb(thingdb::Database),
}

pub(crate) struct DatabaseBuilder {
    path: PathBuf,
    max_journaling_size: u64,
    backend: StorageBackend,
}

#[derive(Clone, Copy)]
pub(crate) struct KeyspaceCreateOptions;

impl Default for KeyspaceCreateOptions {
    fn default() -> Self {
        Self
    }
}

impl KeyspaceCreateOptions {
    #[allow(non_upper_case_globals)]
    pub(crate) const default: Self = Self;
}

#[derive(Clone, Copy)]
pub(crate) enum PersistMode {
    SyncAll,
}

pub(crate) struct Keyspace {
    db: Arc<Backend>,
    name: String,
}

pub(crate) struct Batch {
    db: Arc<Backend>,
    writes: Vec<BatchWrite>,
    error: Option<Error>,
}

struct BatchWrite {
    keyspace: String,
    key: Vec<u8>,
    value: Option<Vec<u8>>,
}

#[derive(Clone)]
pub(crate) struct Slice(Vec<u8>);

pub(crate) struct Guard {
    key: Vec<u8>,
    value: Vec<u8>,
    error: Option<Error>,
}

pub(crate) struct Iter {
    entries: std::vec::IntoIter<Guard>,
}

impl Database {
    pub(crate) fn in_memory_thingdb() -> Result<Self, Error> {
        Ok(Self {
            db: Arc::new(Backend::ThingDb(
                thingdb::Database::in_memory().map_err(|error| Error(error.to_string()))?,
            )),
            path: PathBuf::new(),
        })
    }

    pub(crate) fn is_in_memory(&self) -> bool {
        self.path.as_os_str().is_empty()
    }

    pub(crate) fn builder_with_backend(
        path: impl AsRef<Path>,
        backend: StorageBackend,
    ) -> DatabaseBuilder {
        DatabaseBuilder {
            path: path.as_ref().to_path_buf(),
            max_journaling_size: 32 * 1024 * 1024,
            backend,
        }
    }

    pub(crate) fn keyspace(
        &self,
        name: &str,
        _options: KeyspaceCreateOptions,
    ) -> Result<Keyspace, Error> {
        if let Backend::RocksDb(db) = self.db.as_ref()
            && db.cf_handle(name).is_none()
        {
            return Err(Error(format!("missing RocksDB column family: {name}")));
        }
        Ok(Keyspace {
            db: Arc::clone(&self.db),
            name: name.to_string(),
        })
    }

    pub(crate) fn batch(&self) -> Batch {
        Batch {
            db: Arc::clone(&self.db),
            writes: Vec::new(),
            error: None,
        }
    }

    pub(crate) fn persist(&self, mode: PersistMode) -> Result<(), Error> {
        match self.db.as_ref() {
            Backend::RocksDb(db) => {
                let _ = mode;
                db.flush_wal(true).map_err(Error::from)?;
                db.flush_opt(&FlushOptions::default()).map_err(Error::from)
            },
            Backend::ThingDb(db) => db
                .persist(thingdb::PersistMode::SyncAll)
                .map_err(|error| Error(error.to_string())),
        }
    }

    pub(crate) fn journal_disk_space(&self) -> Result<u64, Error> {
        match self.db.as_ref() {
            Backend::RocksDb(_) => Ok(directory_size(&self.path)),
            Backend::ThingDb(db) => db
                .journal_disk_space()
                .map_err(|error| Error(error.to_string())),
        }
    }

    pub(crate) fn journal_count(&self) -> usize {
        if self.is_in_memory() {
            return 0;
        }
        if let Backend::ThingDb(_) = self.db.as_ref() {
            return 1;
        }
        std::fs::read_dir(&self.path)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_name().to_string_lossy().starts_with("LOG")
                    || entry.file_name().to_string_lossy().ends_with(".log")
            })
            .count()
    }

    pub(crate) fn wal_diagnostics(&self) -> Result<Option<thingdb::WalDiagnostics>, Error> {
        match self.db.as_ref() {
            Backend::RocksDb(_) => Ok(None),
            Backend::ThingDb(db) => db
                .wal_diagnostics()
                .map(Some)
                .map_err(|error| Error(error.to_string())),
        }
    }

    pub(crate) fn ram_diagnostics(&self) -> Result<thingdb::RamDiagnostics, Error> {
        match self.db.as_ref() {
            Backend::RocksDb(_) => Ok(thingdb::RamDiagnostics::default()),
            Backend::ThingDb(db) => db
                .ram_diagnostics()
                .map_err(|error| Error(error.to_string())),
        }
    }

    pub(crate) fn record_ram_deserialization(&self, duration_ns: u64) {
        if let Backend::ThingDb(db) = self.db.as_ref() {
            db.record_ram_deserialization(duration_ns);
        }
    }

    pub(crate) fn record_ram_search(&self, duration_ns: u64) {
        if let Backend::ThingDb(db) = self.db.as_ref() {
            db.record_ram_search(duration_ns);
        }
    }
}

impl DatabaseBuilder {
    pub(crate) fn max_journaling_size(mut self, bytes: u64) -> Self {
        self.max_journaling_size = bytes;
        self
    }

    pub(crate) fn open(self) -> Result<Database, Error> {
        std::fs::create_dir_all(&self.path).map_err(|error| Error(error.to_string()))?;
        let backend = match self.backend {
            StorageBackend::RocksDb => {
                let mut options = Options::default();
                options.create_if_missing(true);
                options.create_missing_column_families(true);
                options.set_write_buffer_size(self.max_journaling_size as usize);
                let names = [
                    "default",
                    "objects",
                    "events",
                    "event_meta",
                    "queue_jobs",
                    "ready_jobs",
                    "lease_jobs",
                    "links_by_id",
                    "links_from",
                    "links_to",
                    "schemas",
                    "migrations",
                    "indexes",
                    "vectors",
                ];
                let descriptors = names
                    .iter()
                    .map(|name| ColumnFamilyDescriptor::new(*name, Options::default()))
                    .collect::<Vec<_>>();
                Backend::RocksDb(Arc::new(DB::open_cf_descriptors(
                    &options,
                    &self.path,
                    descriptors,
                )?))
            },
            StorageBackend::ThingDb => Backend::ThingDb(
                thingdb::Database::builder(&self.path)
                    .max_journaling_size(self.max_journaling_size)
                    .open()
                    .map_err(|error| Error(error.to_string()))?,
            ),
        };
        Ok(Database {
            db: Arc::new(backend),
            path: self.path,
        })
    }
}

impl Keyspace {
    pub(crate) fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Slice>, Error> {
        match self.db.as_ref() {
            Backend::RocksDb(db) => {
                let cf = db.cf_handle(&self.name).ok_or_else(|| {
                    Error(format!("missing RocksDB column family: {}", self.name))
                })?;
                Ok(db.get_cf(cf, key.as_ref())?.map(Slice))
            },
            Backend::ThingDb(db) => Ok(db
                .keyspace(&self.name, thingdb::KeyspaceCreateOptions::default)
                .map_err(|error| Error(error.to_string()))?
                .get(key.as_ref())
                .map_err(|error| Error(error.to_string()))?
                .map(Slice)),
        }
    }

    pub(crate) fn with_value<T>(
        &self,
        key: impl AsRef<[u8]>,
        callback: impl FnOnce(Option<&[u8]>) -> Result<T, Error>,
    ) -> Result<T, Error> {
        match self.db.as_ref() {
            Backend::RocksDb(db) => {
                let cf = db.cf_handle(&self.name).ok_or_else(|| {
                    Error(format!("missing RocksDB column family: {}", self.name))
                })?;
                let value = db.get_cf(cf, key.as_ref())?;
                callback(value.as_deref())
            },
            Backend::ThingDb(db) => db
                .keyspace(&self.name, thingdb::KeyspaceCreateOptions::default)
                .map_err(|error| Error(error.to_string()))?
                .with_value(key, |value| {
                    callback(value).map_err(|error| error.to_string())
                })
                .map_err(|error| Error(error.to_string())),
        }
    }

    pub(crate) fn insert(
        &self,
        key: impl AsRef<[u8]>,
        value: impl AsRef<[u8]>,
    ) -> Result<(), Error> {
        let mut batch = Batch {
            db: Arc::clone(&self.db),
            writes: Vec::new(),
            error: None,
        };
        batch.insert(self, key, value);
        batch.commit()
    }

    pub(crate) fn remove(&self, key: impl AsRef<[u8]>) -> Result<(), Error> {
        let mut batch = Batch {
            db: Arc::clone(&self.db),
            writes: Vec::new(),
            error: None,
        };
        batch.remove(self, key);
        batch.commit()
    }

    pub(crate) fn iter(&self) -> Iter {
        self.collect(None)
    }

    pub(crate) fn prefix(&self, prefix: impl AsRef<[u8]>) -> Iter {
        self.collect(Some(prefix.as_ref().to_vec()))
    }

    pub(crate) fn first_prefix_after(
        &self,
        prefix: impl AsRef<[u8]>,
        after: Option<&[u8]>,
    ) -> Result<Option<Guard>, Error> {
        let prefix = prefix.as_ref();
        match self.db.as_ref() {
            Backend::RocksDb(db) => {
                let cf = db.cf_handle(&self.name).ok_or_else(|| {
                    Error(format!("missing RocksDB column family: {}", self.name))
                })?;
                let start = after.unwrap_or(prefix);
                let iterator =
                    db.iterator_cf(cf, IteratorMode::From(start, rocksdb::Direction::Forward));
                for entry in iterator {
                    match entry {
                        Ok((key, value))
                            if key.starts_with(prefix) && after != Some(key.as_ref()) =>
                        {
                            return Ok(Some(Guard {
                                key: key.to_vec(),
                                value: value.to_vec(),
                                error: None,
                            }));
                        },
                        Ok((key, _)) if !key.starts_with(prefix) => return Ok(None),
                        Ok(_) => {},
                        Err(error) => return Err(Error::from(error)),
                    }
                }
                Ok(None)
            },
            Backend::ThingDb(db) => {
                let keyspace = db
                    .keyspace(&self.name, thingdb::KeyspaceCreateOptions::default)
                    .map_err(|error| Error(error.to_string()))?;
                let entry = keyspace
                    .first_prefix_after(prefix, after)
                    .map_err(|error| Error(error.to_string()))?
                    .map(|entry| Guard {
                        key: entry.key,
                        value: entry.value,
                        error: None,
                    });
                Ok(entry)
            },
        }
    }

    pub(crate) fn range<K, R>(&self, range: R) -> Iter
    where
        K: AsRef<[u8]>,
        R: RangeBounds<K>,
    {
        let start = match range.start_bound() {
            Bound::Included(value) => (value.as_ref().to_vec(), true),
            Bound::Excluded(value) => (value.as_ref().to_vec(), false),
            Bound::Unbounded => (Vec::new(), true),
        };
        let end = match range.end_bound() {
            Bound::Included(value) => Some((value.as_ref().to_vec(), true)),
            Bound::Excluded(value) => Some((value.as_ref().to_vec(), false)),
            Bound::Unbounded => None,
        };
        if let Backend::ThingDb(db) = self.db.as_ref()
            && let Ok(keyspace) = db.keyspace(&self.name, thingdb::KeyspaceCreateOptions::default)
        {
            let entries: Vec<_> = keyspace
                .range_bounds(
                    Some((start.0.as_slice(), start.1)),
                    end.as_ref()
                        .map(|(value, inclusive)| (value.as_slice(), *inclusive)),
                )
                .map(|entry| Guard {
                    key: entry.key,
                    value: entry.value,
                    error: None,
                })
                .collect();
            return Iter {
                entries: entries.into_iter(),
            };
        }
        let entries: Vec<_> = self
            .collect(None)
            .filter(|entry| {
                if entry.error.is_some() {
                    return false;
                }
                let after_start = if start.1 {
                    entry.key.as_slice() >= start.0.as_slice()
                } else {
                    entry.key.as_slice() > start.0.as_slice()
                };
                let before_end = end.as_ref().is_none_or(|(value, inclusive)| {
                    if *inclusive {
                        entry.key.as_slice() <= value.as_slice()
                    } else {
                        entry.key.as_slice() < value.as_slice()
                    }
                });
                after_start && before_end
            })
            .collect();
        Iter {
            entries: entries.into_iter(),
        }
    }

    fn collect(&self, prefix: Option<Vec<u8>>) -> Iter {
        let entries = match self.db.as_ref() {
            Backend::RocksDb(db) => match db.cf_handle(&self.name) {
                Some(cf) => db
                    .iterator_cf(cf, rocksdb::IteratorMode::Start)
                    .filter_map(|entry| match entry {
                        Ok((key, value)) if prefix.as_ref().is_none_or(|p| key.starts_with(p)) => {
                            Some(Guard {
                                key: key.to_vec(),
                                value: value.to_vec(),
                                error: None,
                            })
                        },
                        Ok(_) => None,
                        Err(error) => Some(Guard {
                            key: Vec::new(),
                            value: Vec::new(),
                            error: Some(Error::from(error)),
                        }),
                    })
                    .collect(),
                None => vec![Guard {
                    key: Vec::new(),
                    value: Vec::new(),
                    error: Some(Error(format!(
                        "missing RocksDB column family: {}",
                        self.name
                    ))),
                }],
            },
            Backend::ThingDb(db) => match db
                .keyspace(&self.name, thingdb::KeyspaceCreateOptions::default)
                .map_err(|error| Error(error.to_string()))
            {
                Ok(keyspace) => {
                    let iterator = match prefix {
                        Some(prefix) => keyspace.prefix(prefix),
                        None => keyspace.iter(),
                    };
                    iterator
                        .map(|entry| Guard {
                            key: entry.key,
                            value: entry.value,
                            error: None,
                        })
                        .collect()
                },
                Err(error) => vec![Guard {
                    key: Vec::new(),
                    value: Vec::new(),
                    error: Some(error),
                }],
            },
        };
        Iter {
            entries: entries.into_iter(),
        }
    }

    pub(crate) fn major_compact(&self) -> Result<(), Error> {
        match self.db.as_ref() {
            Backend::RocksDb(db) => {
                let cf = db.cf_handle(&self.name).ok_or_else(|| {
                    Error(format!("missing RocksDB column family: {}", self.name))
                })?;
                db.compact_range_cf(cf, None::<&[u8]>, None::<&[u8]>);
                db.flush().map_err(Error::from)
            },
            Backend::ThingDb(db) => db.compact().map_err(|error| Error(error.to_string())),
        }
    }
}

impl Batch {
    pub(crate) fn insert(
        &mut self,
        keyspace: &Keyspace,
        key: impl AsRef<[u8]>,
        value: impl AsRef<[u8]>,
    ) {
        self.writes.push(BatchWrite {
            keyspace: keyspace.name.clone(),
            key: key.as_ref().to_vec(),
            value: Some(value.as_ref().to_vec()),
        });
    }

    pub(crate) fn remove(&mut self, keyspace: &Keyspace, key: impl AsRef<[u8]>) {
        self.writes.push(BatchWrite {
            keyspace: keyspace.name.clone(),
            key: key.as_ref().to_vec(),
            value: None,
        });
    }

    pub(crate) fn commit(self) -> Result<(), Error> {
        if let Some(error) = self.error {
            return Err(error);
        }
        match self.db.as_ref() {
            Backend::RocksDb(db) => {
                let mut writes = WriteBatch::default();
                for operation in self.writes {
                    let cf = db.cf_handle(&operation.keyspace).ok_or_else(|| {
                        Error(format!(
                            "missing RocksDB column family: {}",
                            operation.keyspace
                        ))
                    })?;
                    if let Some(value) = operation.value {
                        writes.put_cf(cf, &operation.key, value);
                    } else {
                        writes.delete_cf(cf, &operation.key);
                    }
                }
                let mut options = WriteOptions::default();
                options.set_sync(true);
                db.write_opt(writes, &options).map_err(Error::from)
            },
            Backend::ThingDb(db) => {
                let mut writes = db.batch();
                for operation in self.writes {
                    let keyspace = db
                        .keyspace(&operation.keyspace, thingdb::KeyspaceCreateOptions::default)
                        .map_err(|error| Error(error.to_string()))?;
                    writes = match operation.value {
                        Some(value) => writes.put(&keyspace, operation.key, value),
                        None => writes.delete(&keyspace, operation.key),
                    };
                }
                writes.commit().map_err(|error| Error(error.to_string()))
            },
        }
    }
}

impl Slice {
    pub(crate) fn to_vec(&self) -> Vec<u8> {
        self.0.clone()
    }
}

impl Guard {
    pub(crate) fn into_inner(self) -> Result<(Slice, Slice), Error> {
        if let Some(error) = self.error {
            return Err(error);
        }
        Ok((Slice(self.key), Slice(self.value)))
    }
}

impl Iterator for Iter {
    type Item = Guard;

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.next()
    }
}

fn directory_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| {
            let path = entry.path();
            entry
                .metadata()
                .map(|metadata| {
                    if metadata.is_dir() {
                        directory_size(&path)
                    } else if path.extension().is_some_and(|extension| extension == "log")
                        || path.file_name().is_some_and(|name| {
                            name == "LOG" || name.to_string_lossy().starts_with("LOG.old")
                        })
                    {
                        metadata.len()
                    } else {
                        0
                    }
                })
                .unwrap_or(0)
        })
        .sum()
}
