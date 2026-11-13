#![deny(unsafe_code)]
#![allow(missing_docs)] // The public Node API is documented in index.d.ts and README.md.
#![allow(
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::type_complexity,
    clippy::option_if_let_else
)] // N-API's owned values and Task types are required by the JS boundary.

use std::sync::{Arc, Mutex};

use napi::bindgen_prelude::{AsyncTask, Buffer};
use napi::{Env, Task};
use napi_derive::napi;
use thingdb::{Database, KeyspaceCreateOptions, PersistMode};

fn napi_error(error: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}

type BatchItem = (String, Vec<u8>, Option<Vec<u8>>, bool);

type DbHandle = Arc<Mutex<Option<Database>>>;

fn clone_database(handle: &DbHandle) -> napi::Result<Database> {
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

#[napi(js_name = "ThingDB", type_tag = "com.thingd.thingdb.database")]
pub struct ThingDB {
    database: DbHandle,
}

#[napi]
impl ThingDB {
    #[napi(constructor)]
    pub fn new(path: String) -> napi::Result<Self> {
        let database = if path == ":memory:" {
            Database::in_memory()
        } else {
            Database::open(path)
        }
        .map_err(napi_error)?;
        Ok(Self {
            database: Arc::new(Mutex::new(Some(database))),
        })
    }

    #[napi]
    pub fn keyspace(&self, name: String) -> napi::Result<ThingDBKeyspace> {
        let database = clone_database(&self.database)?;
        database
            .keyspace(&name, KeyspaceCreateOptions::default)
            .map_err(napi_error)?;
        Ok(ThingDBKeyspace {
            database: Arc::clone(&self.database),
            name,
        })
    }

    #[napi]
    pub fn close(&self) -> napi::Result<()> {
        let database = self
            .database
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
    pub fn put_many(
        &self,
        operations: Vec<BatchOperation>,
    ) -> napi::Result<AsyncTask<PutManyTask>> {
        if operations.is_empty() {
            return Err(napi::Error::from_reason(
                "putMany requires at least one operation",
            ));
        }
        Ok(AsyncTask::new(PutManyTask {
            database: clone_database(&self.database)?,
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
    pub fn persist(&self) -> napi::Result<AsyncTask<PersistTask>> {
        Ok(AsyncTask::new(PersistTask {
            database: clone_database(&self.database)?,
        }))
    }

    #[napi]
    pub fn compact(&self) -> napi::Result<AsyncTask<CompactTask>> {
        Ok(AsyncTask::new(CompactTask {
            database: clone_database(&self.database)?,
        }))
    }

    #[napi]
    pub fn snapshot(&self) -> napi::Result<AsyncTask<SnapshotTask>> {
        Ok(AsyncTask::new(SnapshotTask {
            database: clone_database(&self.database)?,
        }))
    }
}

#[napi(js_name = "ThingDBKeyspace", type_tag = "com.thingd.thingdb.keyspace")]
pub struct ThingDBKeyspace {
    database: DbHandle,
    name: String,
}

#[napi]
impl ThingDBKeyspace {
    #[napi]
    pub fn get(&self, key: Buffer) -> napi::Result<AsyncTask<GetTask>> {
        Ok(AsyncTask::new(GetTask {
            database: clone_database(&self.database)?,
            name: self.name.clone(),
            key: key.to_vec(),
        }))
    }

    #[napi]
    pub fn put(&self, key: Buffer, value: Buffer) -> napi::Result<AsyncTask<PutTask>> {
        Ok(AsyncTask::new(PutTask {
            database: clone_database(&self.database)?,
            name: self.name.clone(),
            key: key.to_vec(),
            value: value.to_vec(),
        }))
    }

    #[napi]
    pub fn delete(&self, key: Buffer) -> napi::Result<AsyncTask<DeleteTask>> {
        Ok(AsyncTask::new(DeleteTask {
            database: clone_database(&self.database)?,
            name: self.name.clone(),
            key: key.to_vec(),
        }))
    }

    #[napi]
    pub fn entries(&self) -> napi::Result<AsyncTask<EntriesTask>> {
        Ok(AsyncTask::new(EntriesTask {
            database: clone_database(&self.database)?,
            name: self.name.clone(),
            prefix: None,
        }))
    }

    #[napi]
    pub fn prefix(&self, prefix: Buffer) -> napi::Result<AsyncTask<EntriesTask>> {
        Ok(AsyncTask::new(EntriesTask {
            database: clone_database(&self.database)?,
            name: self.name.clone(),
            prefix: Some(prefix.to_vec()),
        }))
    }
}

#[napi(js_name = "ThingDBSnapshot", type_tag = "com.thingd.thingdb.snapshot")]
pub struct ThingDBSnapshot {
    snapshot: thingdb::Snapshot,
}

#[napi]
impl ThingDBSnapshot {
    #[napi]
    pub fn get(&self, keyspace: String, key: Buffer) -> Option<Buffer> {
        self.snapshot
            .keyspace(&keyspace)
            .get(key.as_ref())
            .map(Buffer::from)
    }
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
    type JsValue = ThingDBSnapshot;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.database.snapshot().map_err(napi_error)
    }

    fn resolve(&mut self, _env: Env, snapshot: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(ThingDBSnapshot { snapshot })
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
