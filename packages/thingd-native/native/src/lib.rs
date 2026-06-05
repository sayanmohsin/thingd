use std::sync::{Arc, Mutex, MutexGuard};

use napi::bindgen_prelude::{Error, Result};
use napi_derive::napi;
use serde::Serialize;
use serde_json::Value;
use thingd_core::{
    EventLog, MemoryEvent, MemoryObject, ObjectStore, QueueClaimOptions, QueueJob, QueueJobStatus,
    QueueNackOptions, QueueStore, SearchOptions, Searcher, SqliteThingStore,
};

#[napi]
#[derive(Clone)]
pub struct NativeThingStore {
    store: Arc<Mutex<SqliteThingStore>>,
}

#[napi]
impl NativeThingStore {
    #[napi(factory)]
    pub fn open(path: String) -> Result<Self> {
        let store = if path == ":memory:" {
            SqliteThingStore::open_in_memory()
        } else {
            SqliteThingStore::open(path)
        }
        .map_err(napi_error)?;

        Ok(Self {
            store: Arc::new(Mutex::new(store)),
        })
    }

    #[napi(js_name = "putObjectJson")]
    pub fn put_object_json(&self, collection: String, id: String, body: String) -> Result<String> {
        let mut store = self.lock_store()?;
        let object = store
            .put_object(MemoryObject::new(collection, id, body))
            .map_err(napi_error)?;

        to_json(&object_record(object))
    }

    #[napi(js_name = "getObjectJson")]
    pub fn get_object_json(&self, collection: String, id: String) -> Result<Option<String>> {
        let store = self.lock_store()?;
        let object = store
            .get_object(&collection, &id)
            .map_err(napi_error)?
            .map(object_record)
            .map(|record| to_json(&record))
            .transpose()?;

        Ok(object)
    }

    #[napi(js_name = "listObjectsJson")]
    pub fn list_objects_json(&self, collections_json: Option<String>) -> Result<String> {
        let collections = parse_optional_string_array(collections_json)?;
        let store = self.lock_store()?;
        let objects = store
            .list_objects(collections.as_deref())
            .map_err(napi_error)?
            .into_iter()
            .map(object_record)
            .collect::<Vec<_>>();

        to_json(&objects)
    }

    #[napi(js_name = "deleteObject")]
    pub fn delete_object(&self, collection: String, id: String) -> Result<bool> {
        let mut store = self.lock_store()?;
        store.delete_object(&collection, &id).map_err(napi_error)
    }

    #[napi(js_name = "appendEventJson")]
    pub fn append_event_json(&self, stream: String, body: String) -> Result<String> {
        let event_type = event_type_from_body(&body);
        let mut store = self.lock_store()?;
        let event = store
            .append_event(MemoryEvent::new(stream, event_type, body))
            .map_err(napi_error)?;

        to_json(&event_record(event))
    }

    #[napi(js_name = "listEventsJson")]
    pub fn list_events_json(&self, stream: Option<String>) -> Result<String> {
        let store = self.lock_store()?;
        let events = store
            .list_events(stream.as_deref())
            .map_err(napi_error)?
            .into_iter()
            .map(event_record)
            .collect::<Vec<_>>();

        to_json(&events)
    }

    #[napi(js_name = "pushJobJson")]
    pub fn push_job_json(
        &self,
        queue: String,
        id: String,
        body: String,
        max_attempts: u32,
        delay_ms: i64,
    ) -> Result<String> {
        let mut job = QueueJob::new(queue, id, body, max_attempts);
        if delay_ms > 0 {
            job = job.delay_by_ms(delay_ms as u64);
        }

        let mut store = self.lock_store()?;
        let stored = store.push_job(job).map_err(napi_error)?;

        to_json(&job_record(stored))
    }

    #[napi(js_name = "claimJobJson")]
    pub fn claim_job_json(&self, queue: String, lease_ms: i64) -> Result<Option<String>> {
        let mut store = self.lock_store()?;
        let job = store
            .claim_job_with_options(&queue, QueueClaimOptions::new(non_negative_u64(lease_ms)?))
            .map_err(napi_error)?
            .map(job_record)
            .map(|record| to_json(&record))
            .transpose()?;

        Ok(job)
    }

    #[napi(js_name = "ackJobJson")]
    pub fn ack_job_json(&self, queue: String, id: String) -> Result<String> {
        let mut store = self.lock_store()?;
        let result = match current_job_status(&store, &queue, &id)? {
            None => NativeQueueJobResult::failed("not_found"),
            Some(QueueJobStatus::Completed | QueueJobStatus::Dead) => {
                NativeQueueJobResult::failed("terminal")
            }
            Some(QueueJobStatus::Ready) => NativeQueueJobResult::failed("not_leased"),
            Some(QueueJobStatus::Leased) => {
                let job = store
                    .ack_job(&queue, &id)
                    .map_err(napi_error)?
                    .ok_or_else(|| Error::from_reason("leased job disappeared during ack"))?;
                NativeQueueJobResult::ok(job_record(job))
            }
        };

        to_json(&result)
    }

    #[napi(js_name = "nackJobJson")]
    pub fn nack_job_json(&self, queue: String, id: String, delay_ms: i64) -> Result<String> {
        let mut store = self.lock_store()?;
        let result = match current_job_status(&store, &queue, &id)? {
            None => NativeQueueJobResult::failed("not_found"),
            Some(QueueJobStatus::Completed | QueueJobStatus::Dead) => {
                NativeQueueJobResult::failed("terminal")
            }
            Some(QueueJobStatus::Ready) => NativeQueueJobResult::failed("not_leased"),
            Some(QueueJobStatus::Leased) => {
                let job = store
                    .nack_job_with_options(
                        &queue,
                        &id,
                        QueueNackOptions::new(non_negative_u64(delay_ms)?),
                    )
                    .map_err(napi_error)?
                    .ok_or_else(|| Error::from_reason("leased job disappeared during nack"))?;
                NativeQueueJobResult::ok(job_record(job))
            }
        };

        to_json(&result)
    }

    #[napi(js_name = "listJobsJson")]
    pub fn list_jobs_json(&self, queue: String) -> Result<String> {
        let store = self.lock_store()?;
        let jobs = store
            .list_jobs(&queue)
            .map_err(napi_error)?
            .into_iter()
            .map(job_record)
            .collect::<Vec<_>>();

        to_json(&jobs)
    }

    #[napi(js_name = "listDeadJobsJson")]
    pub fn list_dead_jobs_json(&self, queue: String) -> Result<String> {
        let store = self.lock_store()?;
        let jobs = store
            .list_dead_jobs(&queue)
            .map_err(napi_error)?
            .into_iter()
            .map(job_record)
            .collect::<Vec<_>>();

        to_json(&jobs)
    }

    #[napi(js_name = "listQueuesJson")]
    pub fn list_queues_json(&self) -> Result<String> {
        let store = self.lock_store()?;
        let queues = store.list_queues().map_err(napi_error)?;
        to_json(&queues)
    }

    #[napi(js_name = "searchJson")]
    pub fn search_json(
        &self,
        query: String,
        collections_json: Option<String>,
        limit: Option<u32>,
        filter_json: Option<String>,
    ) -> Result<String> {
        let collections = parse_optional_string_array(collections_json)?;
        let limit = limit.map(|l| l as usize);
        let filter = filter_json
            .map(|json| serde_json::from_str::<Value>(&json).map_err(napi_error))
            .transpose()?;

        let options = SearchOptions {
            collections,
            limit,
            filter,
        };

        let store = self.lock_store()?;
        let hits = store.search(&query, options).map_err(napi_error)?;

        let records = hits
            .into_iter()
            .map(|hit| NativeSearchHit {
                kind: hit.kind,
                collection: hit.collection,
                id: hit.id,
                text: hit.text,
                score: hit.score,
                body: hit.body,
                version: hit.version,
                created_at: hit.created_at,
                updated_at: hit.updated_at,
                event_type: hit.event_type,
            })
            .collect::<Vec<_>>();

        to_json(&records)
    }
}

use napi::bindgen_prelude::AsyncTask;
use napi::{Env, Task};

pub struct CountObjectsTask {
    store: Arc<Mutex<SqliteThingStore>>,
}
#[napi]
impl Task for CountObjectsTask {
    type Output = u32;
    type JsValue = u32;
    fn compute(&mut self) -> Result<Self::Output> {
        let store = self
            .store
            .lock()
            .map_err(|_| Error::from_reason("poisoned"))?;
        let count = store.count_objects().map_err(napi_error)?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct CountEventsTask {
    store: Arc<Mutex<SqliteThingStore>>,
}
#[napi]
impl Task for CountEventsTask {
    type Output = u32;
    type JsValue = u32;
    fn compute(&mut self) -> Result<Self::Output> {
        let store = self
            .store
            .lock()
            .map_err(|_| Error::from_reason("poisoned"))?;
        let count = store.count_events().map_err(napi_error)?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct CountActiveJobsTask {
    store: Arc<Mutex<SqliteThingStore>>,
}
#[napi]
impl Task for CountActiveJobsTask {
    type Output = u32;
    type JsValue = u32;
    fn compute(&mut self) -> Result<Self::Output> {
        let store = self
            .store
            .lock()
            .map_err(|_| Error::from_reason("poisoned"))?;
        let count = store.count_active_jobs().map_err(napi_error)?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct CountDeadJobsTask {
    store: Arc<Mutex<SqliteThingStore>>,
}
#[napi]
impl Task for CountDeadJobsTask {
    type Output = u32;
    type JsValue = u32;
    fn compute(&mut self) -> Result<Self::Output> {
        let store = self
            .store
            .lock()
            .map_err(|_| Error::from_reason("poisoned"))?;
        let count = store.count_dead_jobs().map_err(napi_error)?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct ListCollectionsTask {
    store: Arc<Mutex<SqliteThingStore>>,
}
#[napi]
impl Task for ListCollectionsTask {
    type Output = String;
    type JsValue = String;
    fn compute(&mut self) -> Result<Self::Output> {
        let store = self
            .store
            .lock()
            .map_err(|_| Error::from_reason("poisoned"))?;
        let collections = store.list_collections().map_err(napi_error)?;
        to_json(&collections)
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct ListStreamsTask {
    store: Arc<Mutex<SqliteThingStore>>,
}
#[napi]
impl Task for ListStreamsTask {
    type Output = String;
    type JsValue = String;
    fn compute(&mut self) -> Result<Self::Output> {
        let store = self
            .store
            .lock()
            .map_err(|_| Error::from_reason("poisoned"))?;
        let streams = store.list_streams().map_err(napi_error)?;
        to_json(&streams)
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

#[napi]
impl NativeThingStore {
    #[napi(js_name = "countObjectsJson")]
    pub fn count_objects_json(&self) -> Result<AsyncTask<CountObjectsTask>> {
        Ok(AsyncTask::new(CountObjectsTask {
            store: self.store.clone(),
        }))
    }

    #[napi(js_name = "countEventsJson")]
    pub fn count_events_json(&self) -> Result<AsyncTask<CountEventsTask>> {
        Ok(AsyncTask::new(CountEventsTask {
            store: self.store.clone(),
        }))
    }

    #[napi(js_name = "countActiveJobsJson")]
    pub fn count_active_jobs_json(&self) -> Result<AsyncTask<CountActiveJobsTask>> {
        Ok(AsyncTask::new(CountActiveJobsTask {
            store: self.store.clone(),
        }))
    }

    #[napi(js_name = "countDeadJobsJson")]
    pub fn count_dead_jobs_json(&self) -> Result<AsyncTask<CountDeadJobsTask>> {
        Ok(AsyncTask::new(CountDeadJobsTask {
            store: self.store.clone(),
        }))
    }

    #[napi(js_name = "listCollectionsJson")]
    pub fn list_collections_json(&self) -> Result<AsyncTask<ListCollectionsTask>> {
        Ok(AsyncTask::new(ListCollectionsTask {
            store: self.store.clone(),
        }))
    }

    #[napi(js_name = "listStreamsJson")]
    pub fn list_streams_json(&self) -> Result<AsyncTask<ListStreamsTask>> {
        Ok(AsyncTask::new(ListStreamsTask {
            store: self.store.clone(),
        }))
    }
}

impl NativeThingStore {
    fn lock_store(&self) -> Result<MutexGuard<'_, SqliteThingStore>> {
        self.store
            .lock()
            .map_err(|_| Error::from_reason("native memory store lock was poisoned"))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeObjectRecord {
    collection: String,
    id: String,
    body: String,
    version: u64,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeEventRecord {
    stream: String,
    event_type: String,
    body: String,
    sequence: u64,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeQueueJobRecord {
    queue: String,
    id: String,
    body: String,
    status: &'static str,
    attempts: u32,
    max_attempts: u32,
    available_at_ms: i64,
    leased_at_ms: Option<i64>,
    lease_expires_at_ms: Option<i64>,
    completed_at_ms: Option<i64>,
    dead_at_ms: Option<i64>,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeQueueJobResult {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    job: Option<NativeQueueJobRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeSearchHit {
    kind: String,
    collection: String,
    id: String,
    text: String,
    score: f64,
    body: String,
    version: Option<u64>,
    created_at: String,
    updated_at: Option<String>,
    event_type: Option<String>,
}

impl NativeQueueJobResult {
    fn ok(job: NativeQueueJobRecord) -> Self {
        Self {
            ok: true,
            job: Some(job),
            reason: None,
        }
    }

    fn failed(reason: &'static str) -> Self {
        Self {
            ok: false,
            job: None,
            reason: Some(reason),
        }
    }
}

fn object_record(object: MemoryObject) -> NativeObjectRecord {
    NativeObjectRecord {
        collection: object.key.collection,
        id: object.key.id,
        body: object.body,
        version: object.version,
        created_at: object.created_at,
        updated_at: object.updated_at,
    }
}

fn event_record(event: MemoryEvent) -> NativeEventRecord {
    NativeEventRecord {
        stream: event.stream,
        event_type: event.event_type,
        body: event.body,
        sequence: event.sequence,
        created_at: event.created_at,
    }
}

fn job_record(job: QueueJob) -> NativeQueueJobRecord {
    NativeQueueJobRecord {
        queue: job.queue,
        id: job.id,
        body: job.body,
        status: status_to_str(job.status),
        attempts: job.attempts,
        max_attempts: job.max_attempts,
        available_at_ms: job.available_at_ms,
        leased_at_ms: job.leased_at_ms,
        lease_expires_at_ms: job.lease_expires_at_ms,
        completed_at_ms: job.completed_at_ms,
        dead_at_ms: job.dead_at_ms,
        created_at: job.created_at,
    }
}

fn current_job_status(
    store: &SqliteThingStore,
    queue: &str,
    id: &str,
) -> Result<Option<QueueJobStatus>> {
    Ok(store
        .list_jobs(queue)
        .map_err(napi_error)?
        .into_iter()
        .find(|job| job.id == id)
        .map(|job| job.status))
}

fn status_to_str(status: QueueJobStatus) -> &'static str {
    match status {
        QueueJobStatus::Ready => "ready",
        QueueJobStatus::Leased => "leased",
        QueueJobStatus::Completed => "completed",
        QueueJobStatus::Dead => "dead",
    }
}

fn event_type_from_body(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "event".to_string())
}

fn parse_optional_string_array(value: Option<String>) -> Result<Option<Vec<String>>> {
    value
        .map(|json| serde_json::from_str::<Vec<String>>(&json).map_err(napi_error))
        .transpose()
}

fn to_json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(napi_error)
}

fn non_negative_u64(value: i64) -> Result<u64> {
    u64::try_from(value).map_err(|_| Error::from_reason(format!("expected non-negative integer, got {value}")))
}

fn napi_error(error: impl std::fmt::Display) -> Error {
    Error::from_reason(error.to_string())
}
