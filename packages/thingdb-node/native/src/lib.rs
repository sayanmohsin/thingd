#![deny(unsafe_code)]
#![allow(missing_docs)] // The public Node API is documented in index.d.ts and README.md.
#![allow(
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::type_complexity,
    clippy::option_if_let_else
)] // N-API's owned values and Task types are required by the JS boundary.

use std::{
    collections::HashMap,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicU32, Ordering},
    },
};

use napi::bindgen_prelude::{AsyncTask, Buffer};
use napi::{Env, Task};
use napi_derive::napi;
use thingdb::{Database, KeyspaceCreateOptions, PersistMode};

fn napi_error(error: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}

type BatchItem = (String, Vec<u8>, Option<Vec<u8>>, bool);

type DbHandle = Arc<Mutex<Option<Database>>>;

static DATABASES: LazyLock<Mutex<HashMap<u32, DbHandle>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static SNAPSHOTS: LazyLock<Mutex<HashMap<u32, Arc<thingdb::Snapshot>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_DATABASE_HANDLE: AtomicU32 = AtomicU32::new(1);
static NEXT_SNAPSHOT_HANDLE: AtomicU32 = AtomicU32::new(1);

fn next_handle(counter: &AtomicU32, kind: &str) -> napi::Result<u32> {
    counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map_err(|_| napi::Error::from_reason(format!("ThingDB {kind} handle space exhausted")))
}

fn clone_database(database_handle: u32) -> napi::Result<Database> {
    let handle = DATABASES
        .lock()
        .map_err(|_| napi::Error::from_reason("ThingDB handle registry lock poisoned"))?
        .get(&database_handle)
        .cloned()
        .ok_or_else(|| napi::Error::from_reason("ThingDB is closed or the handle is invalid"))?;
    handle
        .lock()
        .map_err(|_| napi::Error::from_reason("ThingDB handle lock poisoned"))?
        .as_ref()
        .cloned()
        .ok_or_else(|| napi::Error::from_reason("ThingDB is closed"))
}

#[napi(object)]
pub struct BatchOperation {
    pub keyspace: String,
    pub key: Buffer,
    pub value: Option<Buffer>,
    pub delete: Option<bool>,
}

#[napi]
pub fn open_database(path: String) -> napi::Result<u32> {
    let database = if path == ":memory:" {
        Database::in_memory()
    } else {
        Database::open(path)
    }
    .map_err(napi_error)?;
    let handle = next_handle(&NEXT_DATABASE_HANDLE, "database")?;
    DATABASES
        .lock()
        .map_err(|_| napi::Error::from_reason("ThingDB handle registry lock poisoned"))?
        .insert(handle, Arc::new(Mutex::new(Some(database))));
    Ok(handle)
}

#[napi]
pub fn close_database(database_handle: u32) -> napi::Result<()> {
    let handle = DATABASES
        .lock()
        .map_err(|_| napi::Error::from_reason("ThingDB handle registry lock poisoned"))?
        .remove(&database_handle)
        .ok_or_else(|| {
            napi::Error::from_reason("ThingDB is already closed or the handle is invalid")
        })?;
    let database = handle
        .lock()
        .map_err(|_| napi::Error::from_reason("ThingDB handle lock poisoned"))?
        .take();
    if database.is_none() {
        return Err(napi::Error::from_reason("ThingDB is already closed"));
    }
    drop(database);
    Ok(())
}

#[napi]
pub fn create_keyspace(database_handle: u32, name: String) -> napi::Result<()> {
    clone_database(database_handle)?
        .keyspace(&name, KeyspaceCreateOptions::default)
        .map(|_| ())
        .map_err(napi_error)
}

#[napi]
pub fn put_many(
    database_handle: u32,
    operations: Vec<BatchOperation>,
) -> napi::Result<AsyncTask<PutManyTask>> {
    if operations.is_empty() {
        return Err(napi::Error::from_reason(
            "putMany requires at least one operation",
        ));
    }
    Ok(AsyncTask::new(PutManyTask {
        database: clone_database(database_handle)?,
        operations: operations
            .into_iter()
            .map(|operation| {
                (
                    operation.keyspace,
                    operation.key.to_vec(),
                    operation.value.map(|value| value.to_vec()),
                    operation.delete.unwrap_or(false),
                )
            })
            .collect(),
    }))
}

#[napi]
pub fn persist(database_handle: u32) -> napi::Result<AsyncTask<PersistTask>> {
    Ok(AsyncTask::new(PersistTask {
        database: clone_database(database_handle)?,
    }))
}

#[napi]
pub fn compact(database_handle: u32) -> napi::Result<AsyncTask<CompactTask>> {
    Ok(AsyncTask::new(CompactTask {
        database: clone_database(database_handle)?,
    }))
}

#[napi]
pub fn get(database_handle: u32, name: String, key: Buffer) -> napi::Result<AsyncTask<GetTask>> {
    Ok(AsyncTask::new(GetTask {
        database: clone_database(database_handle)?,
        name,
        key: key.to_vec(),
    }))
}

#[napi]
pub fn put(
    database_handle: u32,
    name: String,
    key: Buffer,
    value: Buffer,
) -> napi::Result<AsyncTask<PutTask>> {
    Ok(AsyncTask::new(PutTask {
        database: clone_database(database_handle)?,
        name,
        key: key.to_vec(),
        value: value.to_vec(),
    }))
}

#[napi(js_name = "delete")]
pub fn delete_key(
    database_handle: u32,
    name: String,
    key: Buffer,
) -> napi::Result<AsyncTask<DeleteTask>> {
    Ok(AsyncTask::new(DeleteTask {
        database: clone_database(database_handle)?,
        name,
        key: key.to_vec(),
    }))
}

#[napi]
pub fn entries(database_handle: u32, name: String) -> napi::Result<AsyncTask<EntriesTask>> {
    Ok(AsyncTask::new(EntriesTask {
        database: clone_database(database_handle)?,
        name,
        prefix: None,
    }))
}

#[napi]
pub fn prefix(
    database_handle: u32,
    name: String,
    prefix: Buffer,
) -> napi::Result<AsyncTask<EntriesTask>> {
    Ok(AsyncTask::new(EntriesTask {
        database: clone_database(database_handle)?,
        name,
        prefix: Some(prefix.to_vec()),
    }))
}

#[napi]
pub fn snapshot(database_handle: u32) -> napi::Result<AsyncTask<SnapshotTask>> {
    Ok(AsyncTask::new(SnapshotTask {
        database: clone_database(database_handle)?,
    }))
}

fn register_snapshot(snapshot: thingdb::Snapshot) -> napi::Result<u32> {
    let handle = next_handle(&NEXT_SNAPSHOT_HANDLE, "snapshot")?;
    SNAPSHOTS
        .lock()
        .map_err(|_| napi::Error::from_reason("ThingDB snapshot registry lock poisoned"))?
        .insert(handle, Arc::new(snapshot));
    Ok(handle)
}

#[napi]
pub fn snapshot_get(
    snapshot_handle: u32,
    keyspace: String,
    key: Buffer,
) -> napi::Result<Option<Buffer>> {
    let snapshot = SNAPSHOTS
        .lock()
        .map_err(|_| napi::Error::from_reason("ThingDB snapshot registry lock poisoned"))?
        .get(&snapshot_handle)
        .cloned()
        .ok_or_else(|| {
            napi::Error::from_reason("ThingDB snapshot is closed or the handle is invalid")
        })?;
    Ok(snapshot
        .keyspace(&keyspace)
        .get(key.as_ref())
        .map(Buffer::from))
}

#[napi]
pub fn close_snapshot(snapshot_handle: u32) -> napi::Result<()> {
    SNAPSHOTS
        .lock()
        .map_err(|_| napi::Error::from_reason("ThingDB snapshot registry lock poisoned"))?
        .remove(&snapshot_handle)
        .map(|_| ())
        .ok_or_else(|| {
            napi::Error::from_reason("ThingDB snapshot is already closed or the handle is invalid")
        })
}

pub struct GetTask {
    database: Database,
    name: String,
    key: Vec<u8>,
}

#[napi]
impl Task for GetTask {
    type Output = Option<Vec<u8>>;
    type JsValue = Option<Buffer>;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.database
            .keyspace(&self.name, KeyspaceCreateOptions::default)
            .and_then(|keyspace| keyspace.get(&self.key))
            .map_err(napi_error)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output.map(Buffer::from))
    }
}

pub struct PutTask {
    database: Database,
    name: String,
    key: Vec<u8>,
    value: Vec<u8>,
}

#[napi]
impl Task for PutTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.database
            .keyspace(&self.name, KeyspaceCreateOptions::default)
            .and_then(|keyspace| keyspace.insert(&self.key, &self.value))
            .map_err(napi_error)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct DeleteTask {
    database: Database,
    name: String,
    key: Vec<u8>,
}

#[napi]
impl Task for DeleteTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.database
            .keyspace(&self.name, KeyspaceCreateOptions::default)
            .and_then(|keyspace| keyspace.remove(&self.key))
            .map_err(napi_error)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct PutManyTask {
    database: Database,
    operations: Vec<BatchItem>,
}

#[napi]
impl Task for PutManyTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let mut batch = self.database.batch_with_capacity(self.operations.len());
        for (name, key, value, delete) in &self.operations {
            let keyspace = self
                .database
                .keyspace(name, KeyspaceCreateOptions::default)
                .map_err(napi_error)?;
            if *delete {
                batch = batch.delete(&keyspace, key);
            } else {
                let value = value.as_ref().ok_or_else(|| {
                    napi::Error::from_reason("batch put operations require a value")
                })?;
                batch = batch.put(&keyspace, key, value);
            }
        }
        batch.commit().map_err(napi_error)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct PersistTask {
    database: Database,
}

#[napi]
impl Task for PersistTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.database
            .persist(PersistMode::SyncAll)
            .map_err(napi_error)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct CompactTask {
    database: Database,
}

pub struct SnapshotTask {
    database: Database,
}

#[napi]
impl Task for SnapshotTask {
    type Output = thingdb::Snapshot;
    type JsValue = u32;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.database.snapshot().map_err(napi_error)
    }

    fn resolve(&mut self, _env: Env, snapshot: Self::Output) -> napi::Result<Self::JsValue> {
        register_snapshot(snapshot)
    }
}

#[napi]
impl Task for CompactTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.database.compact().map_err(napi_error)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct EntriesTask {
    database: Database,
    name: String,
    prefix: Option<Vec<u8>>,
}

#[napi]
impl Task for EntriesTask {
    type Output = Vec<(Vec<u8>, Vec<u8>)>;
    type JsValue = Vec<Entry>;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let keyspace = self
            .database
            .keyspace(&self.name, KeyspaceCreateOptions::default)
            .map_err(napi_error)?;
        let entries = match &self.prefix {
            Some(prefix) => keyspace.prefix(prefix),
            None => keyspace.iter(),
        }
        .into_result()
        .map_err(napi_error)?;
        Ok(entries
            .into_iter()
            .map(|entry| (entry.key, entry.value))
            .collect())
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output
            .into_iter()
            .map(|(key, value)| Entry {
                key: Buffer::from(key),
                value: Buffer::from(value),
            })
            .collect())
    }
}

#[napi(object)]
pub struct Entry {
    pub key: Buffer,
    pub value: Buffer,
}
