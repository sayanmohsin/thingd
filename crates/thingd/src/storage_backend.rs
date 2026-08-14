//! Small internal adapter around RocksDB used by the persistent engine.
//!
//! This module is intentionally private. It keeps the durable engine's
//! keyspace and batch semantics separate from the public `ThingStore` API.

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

use rocksdb::{ColumnFamilyDescriptor, DB, FlushOptions, Options, WriteBatch, WriteOptions};

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

#[derive(Clone)]
pub(crate) struct Database {
    db: Arc<DB>,
    path: PathBuf,
}

pub(crate) struct DatabaseBuilder {
    path: PathBuf,
    max_journaling_size: u64,
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
    db: Arc<DB>,
    name: String,
}

pub(crate) struct Batch {
    db: Arc<DB>,
    writes: WriteBatch,
    error: Option<Error>,
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
    pub(crate) fn builder(path: impl AsRef<Path>) -> DatabaseBuilder {
        DatabaseBuilder {
            path: path.as_ref().to_path_buf(),
            max_journaling_size: 32 * 1024 * 1024,
        }
    }

    pub(crate) fn keyspace(
        &self,
        name: &str,
        _options: KeyspaceCreateOptions,
    ) -> Result<Keyspace, Error> {
        if self.db.cf_handle(name).is_none() {
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
            writes: WriteBatch::default(),
            error: None,
        }
    }

    pub(crate) fn persist(&self, mode: PersistMode) -> Result<(), Error> {
        let _ = mode;
        self.db.flush_wal(true).map_err(Error::from)?;
        self.db
            .flush_opt(&FlushOptions::default())
            .map_err(Error::from)
    }

    pub(crate) fn journal_disk_space(&self) -> Result<u64, Error> {
        Ok(directory_size(&self.path))
    }

    pub(crate) fn journal_count(&self) -> usize {
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
}

impl DatabaseBuilder {
    pub(crate) fn max_journaling_size(mut self, bytes: u64) -> Self {
        self.max_journaling_size = bytes;
        self
    }

    pub(crate) fn open(self) -> Result<Database, Error> {
        std::fs::create_dir_all(&self.path).map_err(|error| Error(error.to_string()))?;
        let mut db_options = Options::default();
        db_options.create_if_missing(true);
        db_options.create_missing_column_families(true);
        db_options.set_write_buffer_size(self.max_journaling_size as usize);
        let names = [
            "default",
            "objects",
            "events",
            "event_meta",
            "queue_jobs",
            "ready_jobs",
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
        let db = DB::open_cf_descriptors(&db_options, &self.path, descriptors)?;
        Ok(Database {
            db: Arc::new(db),
            path: self.path,
        })
    }
}

impl Keyspace {
    fn cf(&self) -> Result<&rocksdb::ColumnFamily, Error> {
        self.db
            .cf_handle(&self.name)
            .ok_or_else(|| Error(format!("missing RocksDB column family: {}", self.name)))
    }

    pub(crate) fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Slice>, Error> {
        Ok(self.db.get_cf(self.cf()?, key.as_ref())?.map(Slice))
    }

    pub(crate) fn insert(
        &self,
        key: impl AsRef<[u8]>,
        value: impl AsRef<[u8]>,
    ) -> Result<(), Error> {
        let mut options = WriteOptions::default();
        options.set_sync(true);
        self.db
            .put_cf_opt(self.cf()?, key.as_ref(), value.as_ref(), &options)?;
        Ok(())
    }

    pub(crate) fn remove(&self, key: impl AsRef<[u8]>) -> Result<(), Error> {
        let mut options = WriteOptions::default();
        options.set_sync(true);
        self.db.delete_cf_opt(self.cf()?, key.as_ref(), &options)?;
        Ok(())
    }

    pub(crate) fn iter(&self) -> Iter {
        self.collect(None)
    }

    pub(crate) fn prefix(&self, prefix: impl AsRef<[u8]>) -> Iter {
        self.collect(Some(prefix.as_ref().to_vec()))
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
        let entries = match self.cf() {
            Ok(cf) => self
                .db
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
            Err(error) => vec![Guard {
                key: Vec::new(),
                value: Vec::new(),
                error: Some(error),
            }],
        };
        Iter {
            entries: entries.into_iter(),
        }
    }

    pub(crate) fn major_compact(&self) -> Result<(), Error> {
        self.db
            .compact_range_cf(self.cf()?, None::<&[u8]>, None::<&[u8]>);
        self.db.flush().map_err(Error::from)
    }
}

impl Batch {
    pub(crate) fn insert(
        &mut self,
        keyspace: &Keyspace,
        key: impl AsRef<[u8]>,
        value: impl AsRef<[u8]>,
    ) {
        match keyspace.cf() {
            Ok(cf) => self.writes.put_cf(cf, key.as_ref(), value.as_ref()),
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn remove(&mut self, keyspace: &Keyspace, key: impl AsRef<[u8]>) {
        match keyspace.cf() {
            Ok(cf) => self.writes.delete_cf(cf, key.as_ref()),
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn commit(self) -> Result<(), Error> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let mut options = WriteOptions::default();
        options.set_sync(true);
        self.db
            .write_opt(self.writes, &options)
            .map_err(Error::from)
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
