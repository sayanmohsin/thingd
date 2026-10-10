//! Raw key/value comparisons for embedded engines and Redis server modes.
//!
//! These measurements intentionally do not exercise Thingd's semantic object,
//! event, queue, or search APIs. Redis endpoints must be local and dedicated to
//! this benchmark; only keys under a unique benchmark namespace are touched.

use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use fjall::{Database as FjallDatabase, Keyspace as FjallKeyspace, KeyspaceCreateOptions};
use redis::{Client as RedisClient, Connection as RedisConnection};
use rusqlite::{Connection as SqliteConnection, OptionalExtension, params};
use serde::Serialize;
use tempfile::TempDir;
use thingdb::{
    Database as ThingDatabase, Keyspace as ThingKeyspace,
    KeyspaceCreateOptions as ThingKeyspaceCreateOptions,
};

use super::{CURRENT_REPETITION, RESULTS, record_pending_latency_sample, report};

const VALUE_BYTES: usize = 128;
const BATCH_SIZE: usize = 64;
type BenchError = Box<dyn Error>;
type KvEntry = (Vec<u8>, Vec<u8>);

#[derive(Debug, Serialize)]
#[allow(clippy::redundant_pub_crate)]
pub(crate) struct RawComparisonSnapshot {
    driver: String,
    mode: String,
    repetition: usize,
    database_version: String,
    durability: String,
    client_topology: String,
    restart_recovery: String,
    logical_entries: usize,
    logical_digest: String,
    bytes_on_disk: Option<u64>,
    filesystem_artifacts: Option<usize>,
    resource_status: String,
    redis_server_version: Option<String>,
    redis_used_memory_bytes: Option<u64>,
    redis_persistence_bytes: Option<u64>,
}

#[derive(Clone, Copy)]
enum Mode {
    Ram,
    Durable,
}

impl Mode {
    const fn label(self) -> &'static str {
        match self {
            Self::Ram => "ram",
            Self::Durable => "durable",
        }
    }
}

enum Backend {
    ThingDb {
        database: ThingDatabase,
        keyspace: ThingKeyspace,
    },
    Sqlite {
        connection: Mutex<SqliteConnection>,
    },
    Fjall {
        database: FjallDatabase,
        keyspace: FjallKeyspace,
    },
    #[cfg(feature = "rocksdb-backend")]
    RocksDb {
        database: rocksdb::DB,
    },
    Redis {
        client: RedisClient,
        connection: Mutex<RedisConnection>,
        namespace: String,
        mode: Mode,
        server_version: String,
    },
    Closed,
}

struct RawStore {
    driver: String,
    version: String,
    durability: String,
    topology: String,
    namespace: String,
    directory: Option<TempDir>,
    backend: Backend,
}

struct ResourceProfile {
    bytes_on_disk: Option<u64>,
    filesystem_artifacts: Option<usize>,
    status: String,
    redis_server_version: Option<String>,
    redis_used_memory_bytes: Option<u64>,
    redis_persistence_bytes: Option<u64>,
}

impl RawStore {
    #[allow(clippy::too_many_lines)]
    fn open(driver: &str, namespace: &str) -> Result<Self, BenchError> {
        match driver {
            "thingdb-ram-kv" => {
                let filesystem_probe = tempfile::tempdir()?;
                let database = ThingDatabase::in_memory()?;
                let keyspace =
                    database.keyspace("benchmark", ThingKeyspaceCreateOptions::default)?;
                Ok(Self {
                    driver: driver.to_string(),
                    version: format!("ThingDB {}", locked_version("thingdb")),
                    durability: "process memory; intentionally non-durable".to_string(),
                    topology: "in-process direct keyspace API".to_string(),
                    namespace: namespace.to_string(),
                    directory: Some(filesystem_probe),
                    backend: Backend::ThingDb { database, keyspace },
                })
            },
            "sqlite-memory-kv" => {
                let filesystem_probe = tempfile::tempdir()?;
                let connection = SqliteConnection::open_in_memory()?;
                initialize_sqlite(&connection, Mode::Ram)?;
                let version: String =
                    connection.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
                Ok(Self {
                    driver: driver.to_string(),
                    version: format!("SQLite {version}; rusqlite 0.40.2"),
                    durability: "SQLite :memory:; no persistent journal".to_string(),
                    topology: "in-process single connection".to_string(),
                    namespace: namespace.to_string(),
                    directory: Some(filesystem_probe),
                    backend: Backend::Sqlite {
                        connection: Mutex::new(connection),
                    },
                })
            },
            "thingdb-durable-kv" => {
                let directory = tempfile::tempdir()?;
                let database = ThingDatabase::open(directory.path())?;
                let keyspace =
                    database.keyspace("benchmark", ThingKeyspaceCreateOptions::default)?;
                Ok(Self {
                    driver: driver.to_string(),
                    version: format!("ThingDB {}", locked_version("thingdb")),
                    durability: "WAL sync-before-ack; one logical commit per operation".to_string(),
                    topology: "in-process direct keyspace API".to_string(),
                    namespace: namespace.to_string(),
                    directory: Some(directory),
                    backend: Backend::ThingDb { database, keyspace },
                })
            },
            "sqlite-wal-full-kv" => {
                let directory = tempfile::tempdir()?;
                let connection = SqliteConnection::open(directory.path().join("benchmark.sqlite"))?;
                initialize_sqlite(&connection, Mode::Durable)?;
                let version: String =
                    connection.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
                Ok(Self {
                    driver: driver.to_string(),
                    version: format!("SQLite {version}; rusqlite 0.40.2"),
                    durability: "WAL; synchronous=FULL; autocommit per single write".to_string(),
                    topology: "in-process single connection".to_string(),
                    namespace: namespace.to_string(),
                    directory: Some(directory),
                    backend: Backend::Sqlite {
                        connection: Mutex::new(connection),
                    },
                })
            },
            "fjall-syncall-kv" => {
                let directory = tempfile::tempdir()?;
                let database = FjallDatabase::builder(directory.path()).open()?;
                let keyspace = database.keyspace("benchmark", KeyspaceCreateOptions::default)?;
                Ok(Self {
                    driver: driver.to_string(),
                    version: format!("Fjall {}", locked_version("fjall")),
                    durability: "PersistMode::SyncAll after each write or atomic batch".to_string(),
                    topology: "in-process direct keyspace API".to_string(),
                    namespace: namespace.to_string(),
                    directory: Some(directory),
                    backend: Backend::Fjall { database, keyspace },
                })
            },
            "rocksdb-sync-kv" => {
                #[cfg(feature = "rocksdb-backend")]
                {
                    let directory = tempfile::tempdir()?;
                    let mut options = rocksdb::Options::default();
                    options.create_if_missing(true);
                    let database = rocksdb::DB::open(&options, directory.path())?;
                    Ok(Self {
                        driver: driver.to_string(),
                        version: format!(
                            "RocksDB crate {}; librocksdb-sys {}",
                            locked_version("rocksdb"),
                            locked_version("librocksdb-sys")
                        ),
                        durability: "WriteOptions::set_sync(true) for each commit".to_string(),
                        topology: "in-process direct RocksDB API".to_string(),
                        namespace: namespace.to_string(),
                        directory: Some(directory),
                        backend: Backend::RocksDb { database },
                    })
                }
                #[cfg(not(feature = "rocksdb-backend"))]
                {
                    let _ = namespace;
                    Err("comparable storage benchmark requires the rocksdb-backend feature".into())
                }
            },
            "redis-memory-kv" | "redis-aof-always-kv" => {
                let (url, mode) = if driver == "redis-memory-kv" {
                    (std::env::var("THINGD_BENCH_REDIS_URL")?, Mode::Ram)
                } else {
                    (std::env::var("THINGD_BENCH_REDIS_AOF_URL")?, Mode::Durable)
                };
                ensure_loopback_redis(&url)?;
                let client = RedisClient::open(url)?;
                let mut connection = client.get_connection()?;
                let settings = redis_settings(&mut connection)?;
                let expected = match mode {
                    Mode::Ram => settings.appendonly == "no" && settings.save.is_empty(),
                    Mode::Durable => {
                        settings.appendonly == "yes" && settings.appendfsync == "always"
                    },
                };
                if !expected {
                    return Err(format!("{driver}: Redis persistence settings do not match the required profile: appendonly={}, appendfsync={}, save={:?}", settings.appendonly, settings.appendfsync, settings.save).into());
                }
                Ok(Self {
                    driver: driver.to_string(),
                    version: format!("Redis {}; redis crate {}", settings.version, locked_version("redis")),
                    durability: if matches!(mode, Mode::Ram) {
                        "appendonly=no; save disabled".to_string()
                    } else {
                        "appendonly=yes; appendfsync=always".to_string()
                    },
                    topology: "loopback TCP; one synchronous client connection; server metrics queried separately".to_string(),
                    namespace: namespace.to_string(),
                    directory: None,
                    backend: Backend::Redis {
                        client,
                        connection: Mutex::new(connection),
                        namespace: namespace.to_string(),
                        mode,
                        server_version: settings.version,
                    },
                })
            },
            _ => Err(format!("unknown raw comparison driver {driver}").into()),
        }
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), BenchError> {
        let key = self.physical_key(key);
        match &self.backend {
            Backend::ThingDb { keyspace, .. } => keyspace.insert(&key, value)?,
            Backend::Sqlite { connection, .. } => {
                connection
                    .lock()
                    .map_err(|_| "SQLite benchmark connection poisoned")?
                    .execute(
                        "INSERT OR REPLACE INTO kv (key, value) VALUES (?1, ?2)",
                        params![key, value],
                    )?;
            },
            Backend::Fjall { database, keyspace } => {
                keyspace.insert(&key, value)?;
                database.persist(fjall::PersistMode::SyncAll)?;
            },
            #[cfg(feature = "rocksdb-backend")]
            Backend::RocksDb { database } => {
                let mut options = rocksdb::WriteOptions::default();
                options.set_sync(true);
                database.put_opt(&key, value, &options)?;
            },
            Backend::Redis {
                connection, mode, ..
            } => {
                let mut connection = connection
                    .lock()
                    .map_err(|_| "Redis benchmark connection poisoned")?;
                redis::cmd("SET")
                    .arg(key)
                    .arg(value)
                    .query::<String>(&mut *connection)?;
                let _ = mode;
            },
            Backend::Closed => return Err("benchmark backend is closed".into()),
        }
        Ok(())
    }

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, BenchError> {
        let key = self.physical_key(key);
        match &self.backend {
            Backend::ThingDb { keyspace, .. } => Ok(keyspace.get(&key)?),
            Backend::Sqlite { connection, .. } => {
                let connection = connection
                    .lock()
                    .map_err(|_| "SQLite benchmark connection poisoned")?;
                Ok(connection
                    .query_row("SELECT value FROM kv WHERE key = ?1", params![key], |row| {
                        row.get(0)
                    })
                    .optional()?)
            },
            Backend::Fjall { keyspace, .. } => {
                Ok(keyspace.get(&key)?.map(|value| value.as_ref().to_vec()))
            },
            #[cfg(feature = "rocksdb-backend")]
            Backend::RocksDb { database } => Ok(database.get(&key)?),
            Backend::Redis { connection, .. } => {
                let mut connection = connection
                    .lock()
                    .map_err(|_| "Redis benchmark connection poisoned")?;
                Ok(redis::cmd("GET")
                    .arg(key)
                    .query::<Option<Vec<u8>>>(&mut *connection)?)
            },
            Backend::Closed => Err("benchmark backend is closed".into()),
        }
    }

    fn delete(&self, key: &[u8]) -> Result<(), BenchError> {
        let key = self.physical_key(key);
        match &self.backend {
            Backend::ThingDb { keyspace, .. } => keyspace.remove(&key)?,
            Backend::Sqlite { connection, .. } => {
                connection
                    .lock()
                    .map_err(|_| "SQLite benchmark connection poisoned")?
                    .execute("DELETE FROM kv WHERE key = ?1", params![key])?;
            },
            Backend::Fjall { database, keyspace } => {
                keyspace.remove(&key)?;
                database.persist(fjall::PersistMode::SyncAll)?;
            },
            #[cfg(feature = "rocksdb-backend")]
            Backend::RocksDb { database } => {
                let mut options = rocksdb::WriteOptions::default();
                options.set_sync(true);
                database.delete_opt(&key, &options)?;
            },
            Backend::Redis { connection, .. } => {
                let mut connection = connection
                    .lock()
                    .map_err(|_| "Redis benchmark connection poisoned")?;
                redis::cmd("DEL")
                    .arg(key)
                    .query::<usize>(&mut *connection)?;
            },
            Backend::Closed => return Err("benchmark backend is closed".into()),
        }
        Ok(())
    }

    fn batch(&self, entries: &[KvEntry]) -> Result<(), BenchError> {
        let entries = entries
            .iter()
            .map(|(key, value)| (self.physical_key(key), value.clone()))
            .collect::<Vec<_>>();
        match &self.backend {
            Backend::ThingDb {
                database, keyspace, ..
            } => {
                let mut batch = database.batch_with_capacity(entries.len());
                for (key, value) in entries {
                    batch = batch.put_owned(keyspace, key.clone(), value.clone());
                }
                batch.commit()?;
            },
            Backend::Sqlite { connection, .. } => {
                let mut connection = connection
                    .lock()
                    .map_err(|_| "SQLite benchmark connection poisoned")?;
                let transaction = connection.transaction()?;
                {
                    let mut statement = transaction
                        .prepare_cached("INSERT OR REPLACE INTO kv (key, value) VALUES (?1, ?2)")?;
                    for (key, value) in entries {
                        statement.execute(params![key, value])?;
                    }
                }
                transaction.commit()?;
                drop(connection);
            },
            Backend::Fjall { database, keyspace } => {
                let mut batch = database.batch();
                for (key, value) in &entries {
                    batch.insert(keyspace, key, value);
                }
                batch.commit()?;
                database.persist(fjall::PersistMode::SyncAll)?;
            },
            #[cfg(feature = "rocksdb-backend")]
            Backend::RocksDb { database } => {
                let mut batch = rocksdb::WriteBatch::default();
                for (key, value) in &entries {
                    batch.put(key, value);
                }
                let mut options = rocksdb::WriteOptions::default();
                options.set_sync(true);
                database.write_opt(batch, &options)?;
            },
            Backend::Redis { connection, .. } => {
                let mut connection = connection
                    .lock()
                    .map_err(|_| "Redis benchmark connection poisoned")?;
                let mut command = redis::cmd("MSET");
                for (key, value) in &entries {
                    command.arg(key).arg(value);
                }
                command.query::<String>(&mut *connection)?;
            },
            Backend::Closed => return Err("benchmark backend is closed".into()),
        }
        Ok(())
    }

    fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<KvEntry>, BenchError> {
        let physical_prefix = self.physical_key(prefix);
        match &self.backend {
            Backend::ThingDb { keyspace, .. } => Ok(keyspace
                .prefix(&physical_prefix)
                .map(|entry| (entry.key[self.namespace.len()..].to_vec(), entry.value))
                .collect()),
            Backend::Sqlite { connection, .. } => {
                let connection = connection
                    .lock()
                    .map_err(|_| "SQLite benchmark connection poisoned")?;
                let mut statement =
                    connection.prepare("SELECT key, value FROM kv WHERE key >= ?1 ORDER BY key")?;
                let rows = statement.query_map(params![physical_prefix], |row| {
                    Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
                })?;
                let mut values = Vec::new();
                for row in rows {
                    let (key, value) = row?;
                    if !key.starts_with(&physical_prefix) {
                        break;
                    }
                    values.push((key[self.namespace.len()..].to_vec(), value));
                }
                drop(statement);
                drop(connection);
                Ok(values)
            },
            Backend::Fjall { keyspace, .. } => {
                let mut values = Vec::new();
                for entry in keyspace.prefix(&physical_prefix) {
                    let (key, value) = entry.into_inner()?;
                    values.push((
                        key.as_ref()[self.namespace.len()..].to_vec(),
                        value.as_ref().to_vec(),
                    ));
                }
                Ok(values)
            },
            #[cfg(feature = "rocksdb-backend")]
            Backend::RocksDb { database } => {
                let mut values = Vec::new();
                for entry in database.iterator(rocksdb::IteratorMode::From(
                    &physical_prefix,
                    rocksdb::Direction::Forward,
                )) {
                    let (key, value) = entry?;
                    if !key.starts_with(&physical_prefix) {
                        break;
                    }
                    values.push((key[self.namespace.len()..].to_vec(), value.to_vec()));
                }
                Ok(values)
            },
            Backend::Redis {
                connection,
                namespace,
                ..
            } => {
                let _ = namespace;
                let root = physical_prefix;
                let pattern = format!("{}*", String::from_utf8_lossy(&root));
                let mut connection = connection
                    .lock()
                    .map_err(|_| "Redis benchmark connection poisoned")?;
                let mut cursor = 0_u64;
                let mut keys = Vec::<Vec<u8>>::new();
                loop {
                    let (next, page): (u64, Vec<Vec<u8>>) = redis::cmd("SCAN")
                        .arg(cursor)
                        .arg("MATCH")
                        .arg(&pattern)
                        .arg("COUNT")
                        .arg(1000)
                        .query(&mut *connection)?;
                    keys.extend(page);
                    if next == 0 {
                        break;
                    }
                    cursor = next;
                }
                keys.sort();
                let mut values = Vec::with_capacity(keys.len());
                for full_key in keys {
                    let value: Option<Vec<u8>> =
                        redis::cmd("GET").arg(&full_key).query(&mut *connection)?;
                    if let Some(value) = value {
                        values.push((full_key[self.namespace.len()..].to_vec(), value));
                    }
                }
                drop(connection);
                Ok(values)
            },
            Backend::Closed => Err("benchmark backend is closed".into()),
        }
    }

    fn all_entries(&self, namespace: &[u8]) -> Result<Vec<KvEntry>, BenchError> {
        let _ = namespace;
        let mut entries = self.scan_prefix(b"")?;
        entries.retain(|(key, _)| {
            key.starts_with(b"data/")
                || key.starts_with(b"batch/")
                || key.starts_with(b"concurrent/")
        });
        Ok(entries)
    }

    fn physical_key(&self, key: &[u8]) -> Vec<u8> {
        let mut physical = self.namespace.as_bytes().to_vec();
        physical.extend_from_slice(key);
        physical
    }

    fn scan_range(&self, start: &[u8], end: &[u8]) -> Result<Vec<KvEntry>, BenchError> {
        let physical_start = self.physical_key(start);
        let physical_end = self.physical_key(end);
        match &self.backend {
            Backend::ThingDb { keyspace, .. } => Ok(keyspace
                .range_bounds(Some((&physical_start, true)), Some((&physical_end, false)))
                .map(|entry| (entry.key[self.namespace.len()..].to_vec(), entry.value))
                .collect()),
            Backend::Sqlite { connection, .. } => {
                let connection = connection
                    .lock()
                    .map_err(|_| "SQLite benchmark connection poisoned")?;
                let mut statement = connection.prepare(
                    "SELECT key, value FROM kv WHERE key >= ?1 AND key < ?2 ORDER BY key",
                )?;
                let rows = statement.query_map(params![physical_start, physical_end], |row| {
                    Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
                })?;
                let values = rows
                    .map(|row| {
                        let (key, value) = row?;
                        Ok((key[self.namespace.len()..].to_vec(), value))
                    })
                    .collect::<Result<Vec<_>, rusqlite::Error>>()
                    .map_err(Into::into);
                drop(statement);
                drop(connection);
                values
            },
            Backend::Fjall { keyspace, .. } => {
                let mut values = Vec::new();
                for entry in keyspace.range(physical_start..physical_end) {
                    let (key, value) = entry.into_inner()?;
                    values.push((
                        key.as_ref()[self.namespace.len()..].to_vec(),
                        value.as_ref().to_vec(),
                    ));
                }
                Ok(values)
            },
            #[cfg(feature = "rocksdb-backend")]
            Backend::RocksDb { database } => {
                let mut values = Vec::new();
                for entry in database.iterator(rocksdb::IteratorMode::From(
                    &physical_start,
                    rocksdb::Direction::Forward,
                )) {
                    let (key, value) = entry?;
                    if key.as_ref() >= physical_end.as_slice() {
                        break;
                    }
                    values.push((key[self.namespace.len()..].to_vec(), value.to_vec()));
                }
                Ok(values)
            },
            Backend::Redis { .. } => {
                let all = self.scan_prefix(b"")?;
                Ok(all
                    .into_iter()
                    .filter(|(key, _)| key.as_slice() >= start && key.as_slice() < end)
                    .collect())
            },
            Backend::Closed => Err("benchmark backend is closed".into()),
        }
    }

    fn reopen_and_verify(&mut self, expected_digest: &str) -> Result<String, BenchError> {
        if self.driver == "redis-aof-always-kv" {
            return Ok("not-tested: Redis server restart is outside the client benchmark; AOF policy verified".to_string());
        }
        let Some(directory) = &self.directory else {
            return Ok("not-applicable: process-memory database".to_string());
        };
        let directory_path = directory.path().to_path_buf();
        let old_backend = std::mem::replace(&mut self.backend, Backend::Closed);
        drop(old_backend);
        self.backend = match self.driver.as_str() {
            "thingdb-durable-kv" => {
                let database = ThingDatabase::open(&directory_path)?;
                let keyspace =
                    database.keyspace("benchmark", ThingKeyspaceCreateOptions::default)?;
                Backend::ThingDb { database, keyspace }
            },
            "sqlite-wal-full-kv" => {
                let connection = SqliteConnection::open(directory_path.join("benchmark.sqlite"))?;
                connection.execute_batch("PRAGMA synchronous=FULL;")?;
                Backend::Sqlite {
                    connection: Mutex::new(connection),
                }
            },
            "fjall-syncall-kv" => {
                let database = FjallDatabase::builder(&directory_path).open()?;
                let keyspace = database.keyspace("benchmark", KeyspaceCreateOptions::default)?;
                Backend::Fjall { database, keyspace }
            },
            "rocksdb-sync-kv" => {
                #[cfg(feature = "rocksdb-backend")]
                {
                    let mut options = rocksdb::Options::default();
                    options.create_if_missing(false);
                    Backend::RocksDb {
                        database: rocksdb::DB::open(&options, &directory_path)?,
                    }
                }
                #[cfg(not(feature = "rocksdb-backend"))]
                {
                    return Err("reopening RocksDB requires rocksdb-backend".into());
                }
            },
            other => return Err(format!("restart check is not supported for {other}").into()),
        };
        let started = Instant::now();
        let entries = self.all_entries(b"")?;
        let elapsed = started.elapsed();
        let digest = logical_digest(&entries);
        if digest != expected_digest {
            return Err(format!(
                "{} restart digest mismatch: got {digest}, expected {expected_digest}",
                self.driver
            )
            .into());
        }
        report(
            &self.driver,
            "raw_restart_digest_check",
            entries.len(),
            elapsed,
        );
        Ok("passed: reopened local database and matched logical digest".to_string())
    }

    fn memory_profile(&self) -> Result<ResourceProfile, BenchError> {
        let (disk_bytes, artifacts) = if matches!(self.backend, Backend::Redis { .. }) {
            (None, None)
        } else if let Some(directory) = &self.directory {
            let (bytes, files) = super::directory_stats(directory.path());
            (Some(bytes), Some(files))
        } else {
            (Some(0), Some(0))
        };
        let mut server_version = None;
        let mut server_memory = None;
        let mut persistence_bytes = None;
        if let Backend::Redis {
            client,
            namespace,
            mode,
            server_version: version,
            ..
        } = &self.backend
        {
            server_version = Some(version.clone());
            let mut connection = client.get_connection()?;
            let info = redis::cmd("INFO")
                .arg("memory")
                .query::<String>(&mut connection)?;
            server_memory = Some(
                info_value(&info, "used_memory").ok_or("Redis INFO memory omitted used_memory")?,
            );
            if matches!(mode, Mode::Durable) {
                let info = redis::cmd("INFO")
                    .arg("persistence")
                    .query::<String>(&mut connection)?;
                persistence_bytes = Some(
                    info_value(&info, "aof_current_size")
                        .ok_or("Redis INFO persistence omitted aof_current_size")?,
                );
            }
            drop(connection);
            let mut cleanup_connection = client.get_connection()?;
            cleanup_redis_namespace(&mut cleanup_connection, namespace)?;
            drop(cleanup_connection);
        }
        let resources = if server_version.is_some() {
            "Redis server RSS is not exposed by INFO; used_memory and AOF bytes are server-reported. Process RSS/CPU belongs to the benchmark client.".to_string()
        } else {
            "Process RSS/CPU is recorded in benchmark metadata; database-specific memory unsupported.".to_string()
        };
        Ok(ResourceProfile {
            bytes_on_disk: disk_bytes,
            filesystem_artifacts: artifacts,
            status: resources,
            redis_server_version: server_version,
            redis_used_memory_bytes: server_memory,
            redis_persistence_bytes: persistence_bytes,
        })
    }
}

#[allow(clippy::redundant_pub_crate)]
pub(crate) fn run(operations: usize, seed: u64) -> Result<(), BenchError> {
    let repetition = CURRENT_REPETITION.load(std::sync::atomic::Ordering::Relaxed);
    let namespace = format!("tdbench:{}:{seed}:{repetition}:", std::process::id());
    let drivers = [
        ("thingdb-ram-kv", Mode::Ram),
        ("sqlite-memory-kv", Mode::Ram),
        ("redis-memory-kv", Mode::Ram),
        ("thingdb-durable-kv", Mode::Durable),
        ("rocksdb-sync-kv", Mode::Durable),
        ("sqlite-wal-full-kv", Mode::Durable),
        ("fjall-syncall-kv", Mode::Durable),
        ("redis-aof-always-kv", Mode::Durable),
    ];
    let mut digests = Vec::new();
    for (driver, mode) in drivers {
        let mut store = Arc::new(RawStore::open(driver, &namespace)?);
        warmup(&store, operations.min(100), seed)?;
        run_workloads(&store, operations, seed)?;
        let entries = store.all_entries(namespace.as_bytes())?;
        let digest = logical_digest(&entries);
        let count = entries.len();
        digests.push((driver.to_string(), count, digest.clone()));
        let restart_recovery = if matches!(mode, Mode::Durable) {
            Arc::get_mut(&mut store)
                .ok_or("raw comparison backend still has active worker handles")?
                .reopen_and_verify(&digest)?
        } else {
            "not-applicable: RAM-only mode".to_string()
        };
        let resources = store.memory_profile()?;
        RESULTS
            .get()
            .expect("benchmark output is initialized")
            .lock()
            .map_err(|_| "benchmark results lock poisoned")?
            .raw_comparisons
            .push(RawComparisonSnapshot {
                driver: driver.to_string(),
                mode: mode.label().to_string(),
                repetition,
                database_version: store.version.clone(),
                durability: store.durability.clone(),
                client_topology: store.topology.clone(),
                restart_recovery,
                logical_entries: count,
                logical_digest: digest.clone(),
                bytes_on_disk: resources.bytes_on_disk,
                filesystem_artifacts: resources.filesystem_artifacts,
                resource_status: resources.status,
                redis_server_version: resources.redis_server_version,
                redis_used_memory_bytes: resources.redis_used_memory_bytes,
                redis_persistence_bytes: resources.redis_persistence_bytes,
            });
        println!("{driver}: raw key/value workloads finished; entries={count}; digest={digest}");
        drop(store);
    }
    let Some((_, expected_count, expected_digest)) = digests.first() else {
        return Err("raw comparison suite did not run any engines".into());
    };
    for (driver, count, digest) in &digests {
        if count != expected_count || digest != expected_digest {
            return Err(format!("raw comparison logical digest mismatch for {driver}: got {count}/{digest}, expected {expected_count}/{expected_digest}").into());
        }
    }
    if digests.len() != drivers_len() {
        return Err("raw comparison is incomplete; every configured backend must pass".into());
    }
    Ok(())
}

const fn drivers_len() -> usize {
    8
}

#[allow(clippy::too_many_lines)]
fn run_workloads(store: &Arc<RawStore>, operations: usize, seed: u64) -> Result<(), BenchError> {
    let count = operations.max(1);
    let prefix = b"data/";
    let started = Instant::now();
    for index in 0..count {
        let key = data_key(index);
        let value = value(seed, index, 0);
        let sample = sample_started(index, count);
        store.put(&key, &value)?;
        if let Some(started) = sample {
            record_pending_latency_sample(started.elapsed());
        }
    }
    report(
        &store.driver,
        "raw_sequential_sync_put",
        count,
        started.elapsed(),
    );

    let started = Instant::now();
    for index in 0..count {
        let selected = super::deterministic_index(seed ^ 0x91ab, index, count);
        let key = data_key(selected);
        let updated = value(seed, selected, 1);
        let sample = sample_started(index, count);
        store.put(&key, &updated)?;
        if let Some(started) = sample {
            record_pending_latency_sample(started.elapsed());
        }
    }
    report(
        &store.driver,
        "raw_random_sync_update",
        count,
        started.elapsed(),
    );

    let started = Instant::now();
    for index in 0..count {
        let key = data_key(index);
        let sample = sample_started(index, count);
        if store.get(&key)?.is_none() {
            return Err(format!("{} missing hit key {index}", store.driver).into());
        }
        if let Some(started) = sample {
            record_pending_latency_sample(started.elapsed());
        }
    }
    report(&store.driver, "raw_point_hit", count, started.elapsed());

    let started = Instant::now();
    for index in 0..count {
        let key = format!("missing/{index:010}");
        let sample = sample_started(index, count);
        if store.get(key.as_bytes())?.is_some() {
            return Err(format!("{} unexpectedly found miss key", store.driver).into());
        }
        if let Some(started) = sample {
            record_pending_latency_sample(started.elapsed());
        }
    }
    report(&store.driver, "raw_point_miss", count, started.elapsed());

    let mut batch_count = 0;
    let started = Instant::now();
    for base in (0..count).step_by(BATCH_SIZE) {
        let end = (base + BATCH_SIZE).min(count);
        let batch = (base..end)
            .map(|index| (batch_key(index), value(seed, index, 2)))
            .collect::<Vec<_>>();
        let sample = Instant::now();
        store.batch(&batch)?;
        record_pending_latency_sample(sample.elapsed());
        batch_count += batch.len();
    }
    report(
        &store.driver,
        "raw_atomic_batch_put",
        batch_count,
        started.elapsed(),
    );

    let started = Instant::now();
    let prefix_entries = store.scan_prefix(prefix)?;
    if prefix_entries.len() != count {
        return Err(format!(
            "{} prefix scan returned {} entries, expected {count}",
            store.driver,
            prefix_entries.len()
        )
        .into());
    }
    record_pending_latency_sample(started.elapsed());
    report(
        &store.driver,
        "raw_prefix_scan",
        prefix_entries.len(),
        started.elapsed(),
    );

    let started = Instant::now();
    let range_entries = store.scan_range(b"data/00000", b"data/00010")?;
    if range_entries.is_empty() {
        return Err(format!("{} bounded-range probe returned no entries", store.driver).into());
    }
    record_pending_latency_sample(started.elapsed());
    report(
        &store.driver,
        "raw_bounded_range_probe",
        range_entries.len(),
        started.elapsed(),
    );

    let started = Instant::now();
    for index in 0..count / 2 {
        let sample = sample_started(index, count / 2);
        store.delete(&data_key(index))?;
        if let Some(started) = sample {
            record_pending_latency_sample(started.elapsed());
        }
    }
    report(
        &store.driver,
        "raw_sync_delete",
        count / 2,
        started.elapsed(),
    );

    let concurrent = count.max(1);
    let writers = concurrent.min(4);
    let shared = Arc::clone(store);
    let barrier = Arc::new(std::sync::Barrier::new(writers));
    let started = Instant::now();
    let handles = (0..writers)
        .map(|writer| {
            let store = Arc::clone(&shared);
            let barrier = Arc::clone(&barrier);
            let base = concurrent / writers;
            let remainder = concurrent % writers;
            let writer_count = base + usize::from(writer < remainder);
            let first = writer * base + writer.min(remainder);
            std::thread::spawn(move || -> Result<(), String> {
                barrier.wait();
                for offset in 0..writer_count {
                    let index = first + offset;
                    let sample = sample_started(index, concurrent);
                    let started = sample.map(|_| Instant::now());
                    store
                        .put(
                            format!("concurrent/{index:010}").as_bytes(),
                            &value(seed, index, 3),
                        )
                        .map_err(|error| error.to_string())?;
                    if let Some(started) = started {
                        record_pending_latency_sample(started.elapsed());
                    }
                }
                Ok(())
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle
            .join()
            .map_err(|_| "raw comparison writer panicked")?
            .map_err(std::io::Error::other)?;
    }
    report(
        &shared.driver,
        "raw_concurrent_sync_put_4w",
        concurrent,
        started.elapsed(),
    );
    Ok(())
}

fn warmup(store: &RawStore, count: usize, seed: u64) -> Result<(), Box<dyn Error>> {
    for index in 0..count {
        let key = format!("warmup/{index:010}");
        let value = value(seed, index, 9);
        store.put(key.as_bytes(), &value)?;
        if store.get(key.as_bytes())?.as_deref() != Some(value.as_slice()) {
            return Err(format!("{} warmup readback mismatch", store.driver).into());
        }
        store.delete(key.as_bytes())?;
    }
    Ok(())
}

fn initialize_sqlite(connection: &SqliteConnection, mode: Mode) -> Result<(), Box<dyn Error>> {
    match mode {
        Mode::Ram => {
            connection.execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF;")?;
        },
        Mode::Durable => {
            connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
        },
    }
    connection.execute_batch(
        "CREATE TABLE kv (key BLOB PRIMARY KEY NOT NULL, value BLOB NOT NULL) WITHOUT ROWID;",
    )?;
    Ok(())
}

fn data_key(index: usize) -> Vec<u8> {
    format!("data/{index:010}").into_bytes()
}
fn batch_key(index: usize) -> Vec<u8> {
    format!("batch/{index:010}").into_bytes()
}
fn value(seed: u64, index: usize, revision: u8) -> Vec<u8> {
    let mut result = vec![0_u8; VALUE_BYTES];
    result[..8].copy_from_slice(&seed.to_le_bytes());
    result[8..16].copy_from_slice(&(index as u64).to_le_bytes());
    result[16] = revision;
    for (offset, byte) in result.iter_mut().enumerate().skip(17) {
        *byte = offset.to_le_bytes()[0]
            .wrapping_mul(31)
            .wrapping_add(revision);
    }
    result
}

fn logical_digest(entries: &[(Vec<u8>, Vec<u8>)]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for (key, value) in entries {
        for byte in key.iter().chain(value) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

fn sample_started(index: usize, count: usize) -> Option<Instant> {
    if count <= super::LATENCY_SAMPLE_BUDGET
        || index.is_multiple_of(count.div_ceil(super::LATENCY_SAMPLE_BUDGET))
    {
        Some(Instant::now())
    } else {
        None
    }
}

struct RedisSettings {
    appendonly: String,
    appendfsync: String,
    save: String,
    version: String,
}

fn redis_settings(connection: &mut RedisConnection) -> Result<RedisSettings, Box<dyn Error>> {
    let config = |connection: &mut RedisConnection, name: &str| -> Result<String, Box<dyn Error>> {
        let values: Vec<String> = redis::cmd("CONFIG")
            .arg("GET")
            .arg(name)
            .query(connection)?;
        Ok(values.get(1).cloned().unwrap_or_default())
    };
    let server: String = redis::cmd("INFO").arg("server").query(connection)?;
    Ok(RedisSettings {
        appendonly: config(connection, "appendonly")?,
        appendfsync: config(connection, "appendfsync")?,
        save: config(connection, "save")?,
        version: server
            .lines()
            .find_map(|line| line.strip_prefix("redis_version:").map(str::to_string))
            .unwrap_or_else(|| "unknown".to_string()),
    })
}

fn info_value(info: &str, key: &str) -> Option<u64> {
    info.lines().find_map(|line| {
        line.strip_prefix(&format!("{key}:"))
            .and_then(|value| value.trim().parse().ok())
    })
}

fn locked_version(package: &str) -> String {
    let Ok(lock) = std::fs::read_to_string("Cargo.lock") else {
        return "unknown-lock-version".to_string();
    };
    for block in lock.split("[[package]]") {
        let mut name = None;
        let mut version = None;
        for line in block.lines() {
            if let Some(value) = line.strip_prefix("name = ") {
                name = Some(value.trim_matches('"'));
            }
            if let Some(value) = line.strip_prefix("version = ") {
                version = Some(value.trim_matches('"'));
            }
            if name == Some(package)
                && let Some(version) = version
            {
                return version.to_string();
            }
        }
    }
    "unknown-lock-version".to_string()
}

fn ensure_loopback_redis(url: &str) -> Result<(), Box<dyn Error>> {
    let authority = url
        .strip_prefix("redis://")
        .and_then(|rest| rest.split('/').next())
        .and_then(|authority| authority.rsplit('@').next())
        .ok_or("Redis benchmark URL must use redis:// and include a host")?;
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        bracketed
            .split_once(']')
            .map(|(host, _)| host)
            .ok_or("invalid bracketed Redis benchmark host")?
    } else {
        authority.split(':').next().unwrap_or_default()
    };
    if !matches!(host, "127.0.0.1" | "localhost" | "::1") {
        return Err("Redis benchmark endpoints must use loopback; do not point a benchmark at shared or production Redis".into());
    }
    Ok(())
}

fn cleanup_redis_namespace(
    connection: &mut RedisConnection,
    namespace: &str,
) -> Result<(), BenchError> {
    let pattern = format!("{namespace}*");
    let mut cursor = 0_u64;
    loop {
        let (next, keys): (u64, Vec<Vec<u8>>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(1000)
            .query(connection)?;
        if !keys.is_empty() {
            let _: usize = redis::cmd("DEL").arg(keys).query(connection)?;
        }
        if next == 0 {
            return Ok(());
        }
        cursor = next;
    }
}

#[cfg(test)]
mod tests {
    use super::ensure_loopback_redis;

    #[test]
    fn redis_benchmark_refuses_non_loopback_or_ambiguous_hosts() {
        for url in [
            "redis://example.com:6379/0",
            "redis://localhost.attacker.test:6379/0",
            "redis://127.0.0.1.attacker.test:6379/0",
            "rediss://localhost:6379/0",
        ] {
            assert!(ensure_loopback_redis(url).is_err(), "accepted {url}");
        }
    }

    #[test]
    fn redis_benchmark_accepts_only_exact_loopback_hosts() {
        for url in [
            "redis://127.0.0.1:6379/0",
            "redis://localhost:6379/0",
            "redis://[::1]:6379/0",
            "redis://bench-user:secret@127.0.0.1:6379/0",
        ] {
            assert!(ensure_loopback_redis(url).is_ok(), "rejected {url}");
        }
    }
}
