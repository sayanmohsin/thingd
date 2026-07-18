#![allow(clippy::too_many_arguments)]
use std::sync::{Arc, Mutex, MutexGuard};

use napi::bindgen_prelude::{Error, Result};
use napi_derive::napi;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thingd::{
    AggregateFunction, AggregateOptions, AggregateStore, EventLog, Link, LinkDirection,
    LinkQueryOptions, LinkStore, ListEventsOptions, ListObjectsOptions, MemoryEvent, MemoryObject,
    ObjectStore, PutObjectOptions, QueueClaimOptions, QueueJob, QueueJobStatus, QueueNackOptions,
    QueueStore, SchemaOptions, SearchOptions, Searcher, SqliteThingStore, TimeSeriesOptions,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchObjectInput {
    collection: String,
    id: String,
    body: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchEventInput {
    stream: String,
    event_type: String,
    body: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchJobInput {
    queue: String,
    id: String,
    body: String,
    max_attempts: u32,
    delay_ms: i64,
}

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
    pub fn put_object_json(
        &self,
        collection: String,
        id: String,
        body: String,
        expected_version: Option<i64>,
    ) -> Result<String> {
        let mut store = self.lock_store()?;
        let object = if let Some(version) = expected_version {
            let opts = PutObjectOptions {
                expected_version: Some(version as u64),
                ..Default::default()
            };
            store
                .put_object_with_options(MemoryObject::new(collection, id, body), opts)
                .map_err(napi_error)?
        } else {
            store
                .put_object(MemoryObject::new(collection, id, body))
                .map_err(napi_error)?
        };

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

    #[napi(js_name = "getObjectsBatchJson")]
    pub fn get_objects_batch_json(&self, collection: String, ids: Vec<String>) -> Result<String> {
        let store = self.lock_store()?;
        let objects = store
            .get_objects_batch(&collection, &ids)
            .map_err(napi_error)?;
        let records: Vec<Option<NativeObjectRecord>> = objects
            .into_iter()
            .map(|opt| opt.map(object_record))
            .collect();
        to_json(&records)
    }

    #[napi(js_name = "createIndexJson")]
    pub fn create_index_json(&self, collection: String, field: String) -> Result<()> {
        let mut store = self.lock_store()?;
        store.create_index(&collection, &field).map_err(napi_error)
    }

    #[napi(js_name = "listIndexesJson")]
    pub fn list_indexes_json(&self) -> Result<String> {
        let store = self.lock_store()?;
        let indexes = store.list_indexes().map_err(napi_error)?;
        to_json(&indexes)
    }

    #[napi(js_name = "listObjectsJson")]
    pub fn list_objects_json(
        &self,
        collections_json: Option<String>,
        filter_json: Option<String>,
        limit: Option<i64>,
        offset: Option<i64>,
        sort_field: Option<String>,
        sort_direction: Option<String>,
    ) -> Result<AsyncTask<ListObjectsTask>> {
        Ok(AsyncTask::new(ListObjectsTask {
            store: self.store.clone(),
            collections_json,
            filter_json,
            limit,
            offset,
            sort_field,
            sort_direction,
        }))
    }

    #[napi(js_name = "deleteObject")]
    pub fn delete_object(&self, collection: String, id: String) -> Result<bool> {
        let mut store = self.lock_store()?;
        store.delete_object(&collection, &id).map_err(napi_error)
    }

    #[napi(js_name = "appendEventJson")]
    pub fn append_event_json(&self, stream: String, body: String) -> Result<String> {
        let mut store = self.lock_store()?;
        let event_type = event_type_from_body(&body);
        let idempotency_key = extract_idempotency_key(&body);
        let mut event = MemoryEvent::new(stream, event_type, body);
        if let Some(key) = idempotency_key {
            event.idempotency_key = key;
        }
        let event = store.append_event(event).map_err(napi_error)?;

        to_json(&event_record(event))
    }

    #[napi(js_name = "listEventsJson")]
    pub fn list_events_json(
        &self,
        stream: Option<String>,
        from_sequence: Option<i64>,
        limit: Option<i64>,
        since: Option<String>,
    ) -> Result<String> {
        let store = self.lock_store()?;
        let events = store
            .list_events(
                stream.as_deref(),
                ListEventsOptions {
                    from_sequence: from_sequence.map(|v| v as u64),
                    limit: limit.map(|v| v as u64),
                    since,
                },
            )
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
            },
            Some(QueueJobStatus::Ready) => NativeQueueJobResult::failed("not_leased"),
            Some(QueueJobStatus::Leased) => {
                let job = store
                    .ack_job(&queue, &id)
                    .map_err(napi_error)?
                    .ok_or_else(|| Error::from_reason("leased job disappeared during ack"))?;
                NativeQueueJobResult::ok(job_record(job))
            },
        };

        to_json(&result)
    }

    #[napi(js_name = "nackJobJson")]
    pub fn nack_job_json(
        &self,
        queue: String,
        id: String,
        delay_ms: i64,
        error: Option<String>,
    ) -> Result<String> {
        let mut store = self.lock_store()?;
        let result = match current_job_status(&store, &queue, &id)? {
            None => NativeQueueJobResult::failed("not_found"),
            Some(QueueJobStatus::Completed | QueueJobStatus::Dead) => {
                NativeQueueJobResult::failed("terminal")
            },
            Some(QueueJobStatus::Ready) => NativeQueueJobResult::failed("not_leased"),
            Some(QueueJobStatus::Leased) => {
                let job = store
                    .nack_job_with_options(
                        &queue,
                        &id,
                        QueueNackOptions::with_error(
                            non_negative_u64(delay_ms)?,
                            error.unwrap_or_default(),
                        ),
                    )
                    .map_err(napi_error)?
                    .ok_or_else(|| Error::from_reason("leased job disappeared during nack"))?;
                NativeQueueJobResult::ok(job_record(job))
            },
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

    #[napi(js_name = "walCheckpoint")]
    pub fn wal_checkpoint(&self) -> Result<String> {
        let store = self.lock_store()?;
        let (frames, pages) = store.wal_checkpoint().map_err(napi_error)?;
        to_json(&serde_json::json!({ "framesBefore": frames, "framesAfter": pages }))
    }

    #[napi(js_name = "optimizeSearchIndex")]
    pub fn optimize_search_index(&self) -> Result<()> {
        let store = self.lock_store()?;
        store.optimize_search_index().map_err(napi_error)
    }

    #[napi(js_name = "backupTo")]
    pub fn backup_to(&self, path: String) -> Result<()> {
        let store = self.lock_store()?;
        store.backup_to(&path).map_err(napi_error)
    }

    #[napi(js_name = "putObjectsBatchJson")]
    pub fn put_objects_batch_json(&self, objects_json: String) -> Result<String> {
        let objects: Vec<BatchObjectInput> =
            serde_json::from_str(&objects_json).map_err(napi_error)?;
        let memory_objects: Vec<MemoryObject> = objects
            .into_iter()
            .map(|o| MemoryObject::new(o.collection, o.id, o.body))
            .collect();

        let mut store = self.lock_store()?;
        let results = store
            .put_objects_batch(memory_objects)
            .map_err(napi_error)?;
        let records: Vec<NativeObjectRecord> = results.into_iter().map(object_record).collect();
        to_json(&records)
    }

    #[napi(js_name = "appendEventsBatchJson")]
    pub fn append_events_batch_json(&self, events_json: String) -> Result<String> {
        let events: Vec<BatchEventInput> =
            serde_json::from_str(&events_json).map_err(napi_error)?;
        let memory_events: Vec<MemoryEvent> = events
            .into_iter()
            .map(|e| MemoryEvent::new(e.stream, e.event_type, e.body))
            .collect();

        let mut store = self.lock_store()?;
        let results = store
            .append_events_batch(memory_events)
            .map_err(napi_error)?;
        let records: Vec<NativeEventRecord> = results.into_iter().map(event_record).collect();
        to_json(&records)
    }

    #[napi(js_name = "pushJobsBatchJson")]
    pub fn push_jobs_batch_json(&self, jobs_json: String) -> Result<String> {
        let jobs: Vec<BatchJobInput> = serde_json::from_str(&jobs_json).map_err(napi_error)?;
        let queue_jobs: Vec<QueueJob> = jobs
            .into_iter()
            .map(|j| {
                let mut job = QueueJob::new(j.queue, j.id, j.body, j.max_attempts);
                if j.delay_ms > 0 {
                    job = job.delay_by_ms(j.delay_ms as u64);
                }
                job
            })
            .collect();

        let mut store = self.lock_store()?;
        let results = store.push_jobs_batch(queue_jobs).map_err(napi_error)?;
        let records: Vec<NativeQueueJobRecord> = results.into_iter().map(job_record).collect();
        to_json(&records)
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

    #[napi(js_name = "deleteObjectsBatchJson")]
    pub fn delete_objects_batch_json(&self, keys_json: String) -> Result<u32> {
        let keys: Vec<(String, String)> = serde_json::from_str(&keys_json).map_err(napi_error)?;
        let mut store = self.lock_store()?;
        let count = store.delete_objects_batch(&keys).map_err(napi_error)?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }

    #[napi(js_name = "createLinkJson")]
    pub fn create_link_json(
        &self,
        from_ref: String,
        link_type: String,
        to_ref: String,
        weight: Option<f64>,
        metadata_json: Option<String>,
    ) -> Result<String> {
        let mut store = self.lock_store()?;
        let mut link = Link::new(from_ref, link_type, to_ref);
        if let Some(w) = weight {
            link = link.with_weight(w);
        }
        if let Some(m) = metadata_json {
            link = link.with_metadata(m);
        }

        let created = store.create_link(link).map_err(napi_error)?;
        to_json(&link_record(created))
    }

    #[napi(js_name = "deleteLink")]
    pub fn delete_link(&self, id: String) -> Result<bool> {
        let mut store = self.lock_store()?;
        store.delete_link(&id).map_err(napi_error)
    }

    #[napi(js_name = "getLinkJson")]
    pub fn get_link_json(&self, id: String) -> Result<Option<String>> {
        let store = self.lock_store()?;
        let link = store
            .get_link(&id)
            .map_err(napi_error)?
            .map(link_record)
            .map(|record| to_json(&record))
            .transpose()?;

        Ok(link)
    }

    #[napi(js_name = "getNeighborsJson")]
    pub fn get_neighbors_json(
        &self,
        reference: String,
        direction: String,
        link_type: Option<String>,
        limit: Option<i64>,
    ) -> Result<String> {
        let dir = match direction.as_str() {
            "Outgoing" => LinkDirection::Outgoing,
            "Incoming" => LinkDirection::Incoming,
            "Both" => LinkDirection::Both,
            _ => return Err(Error::from_reason("invalid direction")),
        };
        let options = LinkQueryOptions {
            link_type,
            limit: limit.map(|l| l as usize),
        };

        let store = self.lock_store()?;
        let links = store
            .get_neighbors(&reference, dir, options)
            .map_err(napi_error)?
            .into_iter()
            .map(link_record)
            .collect::<Vec<_>>();

        to_json(&links)
    }

    #[napi(js_name = "aggregateJson")]
    pub fn aggregate_json(
        &self,
        collection: String,
        function: String,
        field: Option<String>,
        group_by: Option<String>,
        filter_json: Option<String>,
    ) -> Result<String> {
        let agg_function = match function.as_str() {
            "count" => AggregateFunction::Count,
            "sum" => AggregateFunction::Sum,
            "avg" => AggregateFunction::Avg,
            "min" => AggregateFunction::Min,
            "max" => AggregateFunction::Max,
            _ => return Err(Error::from_reason(format!("unknown function '{function}'"))),
        };

        let filter = parse_filter_json(filter_json)?;

        let options = AggregateOptions {
            filter,
            group_by,
            function: agg_function,
            field,
        };

        let store = self.lock_store()?;
        let result = store.aggregate(&collection, &options).map_err(napi_error)?;

        to_json(&AggregateResultRecord {
            total: result.total,
            groups: result
                .groups
                .into_iter()
                .map(|g| AggregateGroupRecord {
                    key: g.key,
                    value: g.value,
                })
                .collect(),
        })
    }

    #[napi(js_name = "timeseriesJson")]
    pub fn timeseries_json(
        &self,
        collection: String,
        function: String,
        field: Option<String>,
        bucket: String,
        from: Option<String>,
        to: Option<String>,
        filter_json: Option<String>,
    ) -> Result<String> {
        let agg_function = match function.as_str() {
            "count" => AggregateFunction::Count,
            "sum" => AggregateFunction::Sum,
            "avg" => AggregateFunction::Avg,
            "min" => AggregateFunction::Min,
            "max" => AggregateFunction::Max,
            _ => return Err(Error::from_reason(format!("unknown function '{function}'"))),
        };

        let time_bucket = match bucket.as_str() {
            "hour" => thingd::TimeBucket::Hour,
            "day" => thingd::TimeBucket::Day,
            "week" => thingd::TimeBucket::Week,
            "month" => thingd::TimeBucket::Month,
            _ => return Err(Error::from_reason(format!("unknown bucket '{bucket}'"))),
        };

        let filter = parse_filter_json(filter_json)?;

        let options = TimeSeriesOptions {
            filter,
            bucket: time_bucket,
            function: agg_function,
            field,
            from,
            to,
        };

        let store = self.lock_store()?;
        let result = store
            .timeseries(&collection, &options)
            .map_err(napi_error)?;

        to_json(&TimeSeriesResultRecord {
            buckets: result
                .buckets
                .into_iter()
                .map(|b| TimeSeriesBucketRecord {
                    label: b.label,
                    value: b.value,
                })
                .collect(),
        })
    }

    #[napi(js_name = "schemaJson")]
    pub fn schema_json(
        &self,
        collection: Option<String>,
        sample_size: Option<i64>,
    ) -> Result<String> {
        let store = self.lock_store()?;
        let options = SchemaOptions {
            sample_size: sample_size.map(|s| s as usize),
        };
        let schemas = store
            .schema(collection.as_deref(), &options)
            .map_err(napi_error)?;
        to_json(&schemas)
    }
}

use napi::bindgen_prelude::AsyncTask;
use napi::{Env, Task};

pub struct ListObjectsTask {
    store: Arc<Mutex<SqliteThingStore>>,
    collections_json: Option<String>,
    filter_json: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
    sort_field: Option<String>,
    sort_direction: Option<String>,
}

#[napi]
impl Task for ListObjectsTask {
    type Output = String;
    type JsValue = String;

    fn compute(&mut self) -> Result<Self::Output> {
        let collections = parse_optional_string_array(self.collections_json.clone())?;

        let filter_pairs: Vec<(String, Value)> = self
            .filter_json
            .as_ref()
            .map(|json| {
                let obj: serde_json::Map<String, Value> =
                    serde_json::from_str(json).map_err(napi_error)?;
                Ok::<_, Error>(obj.into_iter().collect())
            })
            .transpose()?
            .unwrap_or_default();

        let sort_by = self.sort_field.as_ref().map(|field| {
            let direction = match self.sort_direction.as_deref() {
                Some("desc") => thingd::SortDirection::Desc,
                _ => thingd::SortDirection::Asc,
            };
            thingd::SortBy {
                field: field.clone(),
                direction,
            }
        });

        let options = ListObjectsOptions {
            filter: filter_pairs,
            sort_by,
            limit: self.limit.map(|v| v as u64),
            offset: self.offset.map(|v| v as u64),
        };

        let store = self
            .store
            .lock()
            .map_err(|_| Error::from_reason("poisoned"))?;
        let objects = store
            .list_objects(collections.as_deref(), &options)
            .map_err(napi_error)?
            .into_iter()
            .map(object_record)
            .collect::<Vec<_>>();

        to_json(&objects)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

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

    #[napi(js_name = "countLinksJson")]
    pub fn count_links_json(&self) -> Result<AsyncTask<CountLinksTask>> {
        Ok(AsyncTask::new(CountLinksTask {
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
    last_error: String,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeLinkRecord {
    id: String,
    from_ref: String,
    link_type: String,
    to_ref: String,
    weight: Option<f64>,
    metadata_json: String,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AggregateResultRecord {
    total: f64,
    groups: Vec<AggregateGroupRecord>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AggregateGroupRecord {
    key: String,
    value: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TimeSeriesResultRecord {
    buckets: Vec<TimeSeriesBucketRecord>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TimeSeriesBucketRecord {
    label: String,
    value: f64,
}

pub struct CountLinksTask {
    store: Arc<Mutex<SqliteThingStore>>,
}
#[napi]
impl Task for CountLinksTask {
    type Output = u32;
    type JsValue = u32;
    fn compute(&mut self) -> Result<Self::Output> {
        let store = self
            .store
            .lock()
            .map_err(|_| Error::from_reason("poisoned"))?;
        let count = store.count_links().map_err(napi_error)?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
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
        last_error: job.last_error,
    }
}

fn link_record(link: Link) -> NativeLinkRecord {
    NativeLinkRecord {
        id: link.id,
        from_ref: link.from_ref,
        link_type: link.link_type,
        to_ref: link.to_ref,
        weight: link.weight,
        metadata_json: link.metadata_json,
        created_at: link.created_at,
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

fn extract_idempotency_key(body: &str) -> Option<String> {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("idempotencyKey")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .filter(|key| !key.is_empty())
}

fn parse_optional_string_array(value: Option<String>) -> Result<Option<Vec<String>>> {
    value
        .map(|json| serde_json::from_str::<Vec<String>>(&json).map_err(napi_error))
        .transpose()
}

fn parse_filter_json(filter_json: Option<String>) -> Result<Vec<(String, Value)>> {
    filter_json
        .map(|json| {
            let obj: serde_json::Map<String, Value> =
                serde_json::from_str(&json).map_err(napi_error)?;
            Ok::<_, Error>(obj.into_iter().collect())
        })
        .transpose()
        .map(|opt| opt.unwrap_or_default())
}

fn to_json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(napi_error)
}

fn non_negative_u64(value: i64) -> Result<u64> {
    u64::try_from(value)
        .map_err(|_| Error::from_reason(format!("expected non-negative integer, got {value}")))
}

fn napi_error(error: impl std::fmt::Display) -> Error {
    Error::from_reason(error.to_string())
}
