#![allow(
    clippy::branches_sharing_code,
    clippy::missing_errors_doc,
    clippy::match_wildcard_for_single_variants,
    clippy::assigning_clones,
    clippy::needless_pass_by_value
)]

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use fjall::{Database, Keyspace, KeyspaceCreateOptions};

use crate::error::ThingdResult;
use crate::model::{
    AggregateFunction, AggregateGroupResult, AggregateOptions, AggregateResult, LinkDirection,
    LinkQueryOptions, ListEventsOptions, ListObjectsOptions, PutObjectOptions, SchemaOptions,
    SearchOptions, SortDirection, TimeBucket, TimeSeriesBucket, TimeSeriesOptions,
    TimeSeriesResult,
};
use crate::store::{AggregateStore, EventLog, LinkStore, ObjectStore, QueueStore, Searcher};
use crate::{
    CollectionSchema, FieldSchema, Link, MemoryEvent, MemoryObject, QueueClaimOptions, QueueJob,
    QueueJobStatus, QueueNackOptions, SearchHit, ThingdError,
};
use crate::{now_iso_string, unix_timestamp_millis};

/// Fjall-backed storage engine implementing all 6 storage traits.
///
/// Data directory layout:
/// - `objects`: `{collection}\0{id}` → serialized `MemoryObject`
/// - `events`: `{stream}\0{seq:8BE}` → serialized `MemoryEvent`
/// - `queue_jobs`: `{queue}\0{id}` → serialized `QueueJob`
/// - `links_by_id`: `{link_id}` → serialized `Link`
/// - `links_from`: `{from_ref}\0{type}\0{link_id}` → `()`
/// - `links_to`: `{to_ref}\0{type}\0{link_id}` → `()`
pub struct FjallEngine {
    #[allow(dead_code)]
    db: Database,
    objects: Keyspace,
    events: Keyspace,
    queue_jobs: Keyspace,
    ready_jobs: Keyspace,
    links_by_id: Keyspace,
    links_from: Keyspace,
    links_to: Keyspace,
    next_link_id: AtomicU64,
    event_seq_counters: HashMap<String, u64>,
    event_idempotency_keys: HashMap<(String, String), u64>,
    #[cfg(feature = "search")]
    search_index: Option<tantivy::Index>,
}

fn value_to_vec(v: Option<fjall::Slice>) -> Option<Vec<u8>> {
    v.map(|c| c.to_vec())
}

impl FjallEngine {
    /// Open or create a Fjall database at the given path.
    /// Creates all required keyspaces (partitions) on first open.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, fjall::Error> {
        let db = Database::builder(path.as_ref()).open()?;

        let objects = db.keyspace("objects", KeyspaceCreateOptions::default)?;
        let events = db.keyspace("events", KeyspaceCreateOptions::default)?;
        let queue_jobs = db.keyspace("queue_jobs", KeyspaceCreateOptions::default)?;
        let ready_jobs = db.keyspace("ready_jobs", KeyspaceCreateOptions::default)?;
        let links_by_id = db.keyspace("links_by_id", KeyspaceCreateOptions::default)?;
        let links_from = db.keyspace("links_from", KeyspaceCreateOptions::default)?;
        let links_to = db.keyspace("links_to", KeyspaceCreateOptions::default)?;

        let mut next_link_id = 0u64;
        for kv in links_by_id.iter() {
            let (key, _) = kv.into_inner()?;
            let key_str = String::from_utf8_lossy(&key);
            if let Some(id_str) = key_str.strip_prefix("link-")
                && let Ok(id) = id_str.parse::<u64>()
                && id > next_link_id
            {
                next_link_id = id;
            }
        }

        #[cfg(feature = "search")]
        let search_index = Self::init_search_index(path.as_ref());

        Ok(Self {
            db,
            objects,
            events,
            queue_jobs,
            ready_jobs,
            links_by_id,
            links_from,
            links_to,
            next_link_id: AtomicU64::new(next_link_id + 1),
            event_seq_counters: HashMap::new(),
            event_idempotency_keys: HashMap::new(),
            #[cfg(feature = "search")]
            search_index,
        })
    }

    #[cfg(feature = "search")]
    fn init_search_index(path: &Path) -> Option<tantivy::Index> {
        let search_dir = path.join("search");
        let _ = std::fs::create_dir_all(&search_dir);

        let mut schema_builder = tantivy::schema::Schema::builder();
        schema_builder.add_text_field(
            "collection",
            tantivy::schema::STRING | tantivy::schema::STORED,
        );
        schema_builder.add_text_field("id", tantivy::schema::STRING | tantivy::schema::STORED);
        schema_builder.add_text_field("body", tantivy::schema::TEXT | tantivy::schema::STORED);
        schema_builder.add_text_field("kind", tantivy::schema::STRING | tantivy::schema::STORED);
        let schema = schema_builder.build();

        let index = tantivy::Index::create_in_dir(&search_dir, schema).ok()?;
        Some(index)
    }

    fn serialize<T: serde::Serialize>(value: &T) -> ThingdResult<Vec<u8>> {
        serde_json::to_vec(value).map_err(|e| ThingdError::Storage(e.to_string()))
    }

    fn deserialize<'a, T: serde::Deserialize<'a>>(bytes: &'a [u8]) -> ThingdResult<T> {
        serde_json::from_slice(bytes).map_err(|e| ThingdError::Storage(e.to_string()))
    }

    fn make_object_key(collection: &str, id: &str) -> Vec<u8> {
        format!("{collection}\0{id}").into_bytes()
    }

    fn split_object_key(key: &[u8]) -> ThingdResult<(String, String)> {
        let key_str = String::from_utf8_lossy(key);
        let Some((collection, id)) = key_str.split_once('\0') else {
            return Err(ThingdError::Storage("invalid object key format".into()));
        };
        Ok((collection.to_string(), id.to_string()))
    }

    fn make_event_key(stream: &str, sequence: u64) -> Vec<u8> {
        let seq_be = sequence.to_be_bytes();
        let mut key = Vec::with_capacity(stream.len() + 1 + 8);
        key.extend_from_slice(stream.as_bytes());
        key.push(0);
        key.extend_from_slice(&seq_be);
        key
    }

    fn split_event_key(key: &[u8]) -> ThingdResult<(String, u64)> {
        let Some(pos) = key.iter().position(|&b| b == 0) else {
            return Err(ThingdError::Storage("invalid event key format".into()));
        };
        let stream = String::from_utf8_lossy(&key[..pos]).to_string();
        let mut seq_bytes = [0u8; 8];
        seq_bytes.copy_from_slice(&key[pos + 1..pos + 9]);
        let sequence = u64::from_be_bytes(seq_bytes);
        Ok((stream, sequence))
    }

    fn make_queue_key(queue: &str, id: &str) -> Vec<u8> {
        format!("{queue}\0{id}").into_bytes()
    }

    /// Ready jobs index key: {`queue}\0{priority_rev:8BE}\0{created_at}\0{id`}
    fn make_ready_key(queue: &str, priority: i32, created_at: &str, id: &str) -> Vec<u8> {
        let priority_rev = (i32::MAX - priority).to_be_bytes();
        let mut key = Vec::new();
        key.extend_from_slice(queue.as_bytes());
        key.push(b'\0');
        key.extend_from_slice(&priority_rev);
        key.push(b'\0');
        key.extend_from_slice(created_at.as_bytes());
        key.push(b'\0');
        key.extend_from_slice(id.as_bytes());
        key
    }

    fn make_link_from_key(from_ref: &str, link_type: &str, link_id: &str) -> Vec<u8> {
        format!("{from_ref}\0{link_type}\0{link_id}").into_bytes()
    }

    fn make_link_to_key(to_ref: &str, link_type: &str, link_id: &str) -> Vec<u8> {
        format!("{to_ref}\0{link_type}\0{link_id}").into_bytes()
    }
}

fn guard_data(kv: fjall::Guard) -> ThingdResult<(Vec<u8>, Vec<u8>)> {
    let kv = kv.into_inner()?;
    let key = kv.0.to_vec();
    let val = kv.1.to_vec();
    Ok((key, val))
}

// ── ObjectStore ──────────────────────────────────────────────────────────────

impl ObjectStore for FjallEngine {
    fn put_object(&mut self, mut object: MemoryObject) -> ThingdResult<MemoryObject> {
        let key = Self::make_object_key(&object.key.collection, &object.key.id);

        if object.created_at.is_empty() {
            object.created_at = now_iso_string();
        }
        object.updated_at = now_iso_string();

        if let Some(existing) = value_to_vec(self.objects.get(&key)?) {
            let existing_obj: MemoryObject = Self::deserialize(&existing)?;
            object.version = existing_obj.version + 1;
            object.created_at.clone_from(&existing_obj.created_at);
        } else {
            object.version = 1;
        }

        let data = Self::serialize(&object)?;
        self.objects.insert(&key, &data)?;

        #[cfg(feature = "search")]
        self.index_object_for_search(&object);

        Ok(object)
    }

    fn put_object_with_options(
        &mut self,
        object: MemoryObject,
        options: PutObjectOptions,
    ) -> ThingdResult<MemoryObject> {
        let key = Self::make_object_key(&object.key.collection, &object.key.id);

        if let Some(expected_version) = options.expected_version {
            match value_to_vec(self.objects.get(&key)?) {
                Some(existing) => {
                    let existing_obj: MemoryObject = Self::deserialize(&existing)?;
                    if existing_obj.version != expected_version {
                        return Err(ThingdError::Conflict(format!(
                            "expected version {} but current version is {}",
                            expected_version, existing_obj.version
                        )));
                    }
                },
                None => {
                    return Err(ThingdError::Conflict(format!(
                        "object '{}/{}' does not exist",
                        object.key.collection, object.key.id
                    )));
                },
            }
        }

        self.put_object(object)
    }

    fn get_object(&self, collection: &str, id: &str) -> ThingdResult<Option<MemoryObject>> {
        let key = Self::make_object_key(collection, id);
        match value_to_vec(self.objects.get(&key)?) {
            Some(data) => Ok(Some(Self::deserialize(&data)?)),
            None => Ok(None),
        }
    }

    fn get_objects_batch(
        &self,
        collection: &str,
        ids: &[String],
    ) -> ThingdResult<Vec<Option<MemoryObject>>> {
        ids.iter()
            .map(|id| self.get_object(collection, id))
            .collect()
    }

    fn list_objects(
        &self,
        collections: Option<&[String]>,
        options: &ListObjectsOptions,
    ) -> ThingdResult<Vec<MemoryObject>> {
        let prefix = if let Some(collections) = collections
            && collections.len() == 1
        {
            Some(Self::make_object_key(&collections[0], ""))
        } else {
            None
        };

        let mut objects: Vec<MemoryObject> = if let Some(ref prefix) = prefix {
            let mut objs = Vec::new();
            for kv in self.objects.prefix(prefix) {
                let (_, value) = guard_data(kv)?;
                objs.push(Self::deserialize(&value)?);
            }
            objs
        } else {
            let mut objs = Vec::new();
            for kv in self.objects.iter() {
                let (_, value) = guard_data(kv)?;
                objs.push(Self::deserialize(&value)?);
            }
            objs
        };

        if let Some(cols) = collections
            && cols.len() != 1
        {
            objects.retain(|o| cols.contains(&o.key.collection));
        }

        if !options.filter.is_empty() {
            objects.retain(|object| {
                let Ok(body) = serde_json::from_str::<serde_json::Value>(&object.body) else {
                    return false;
                };
                options.filter.iter().all(|(key, expected)| {
                    let field_val = body.get(key.as_str());
                    match expected {
                        serde_json::Value::Object(ops)
                            if ops.keys().any(|k| {
                                matches!(
                                    k.as_str(),
                                    "$gt" | "$gte" | "$lt" | "$lte" | "$ne" | "$in" | "$like"
                                )
                            }) =>
                        {
                            let Some(fv) = field_val else {
                                return false;
                            };
                            ops.iter().all(|(op, operand)| match op.as_str() {
                                "$gt" => value_compare(fv, operand) == std::cmp::Ordering::Greater,
                                "$gte" => matches!(
                                    value_compare(fv, operand),
                                    std::cmp::Ordering::Greater | std::cmp::Ordering::Equal
                                ),
                                "$lt" => value_compare(fv, operand) == std::cmp::Ordering::Less,
                                "$lte" => matches!(
                                    value_compare(fv, operand),
                                    std::cmp::Ordering::Less | std::cmp::Ordering::Equal
                                ),
                                "$ne" => value_compare(fv, operand) != std::cmp::Ordering::Equal,
                                "$in" => {
                                    if let serde_json::Value::Array(items) = operand {
                                        items.iter().any(|item| fv == item)
                                    } else {
                                        false
                                    }
                                },
                                "$like" => {
                                    if let (
                                        serde_json::Value::String(s),
                                        serde_json::Value::String(pat),
                                    ) = (fv, operand)
                                    {
                                        like_match(s, pat)
                                    } else {
                                        false
                                    }
                                },
                                _ => true,
                            })
                        },
                        _ => field_val.is_some_and(|v| v == expected),
                    }
                })
            });
        }

        if let Some(ref sort_by) = options.sort_by {
            let asc = sort_by.direction == SortDirection::Asc;
            objects.sort_by(|a, b| {
                let cmp = if sort_by.field.starts_with("$.") {
                    let path = sort_by.field.trim_start_matches('$');
                    let a_val = serde_json::from_str::<serde_json::Value>(&a.body)
                        .ok()
                        .and_then(|v| v.get(path).cloned());
                    let b_val = serde_json::from_str::<serde_json::Value>(&b.body)
                        .ok()
                        .and_then(|v| v.get(path).cloned());
                    match (&a_val, &b_val) {
                        (Some(a), Some(b)) => value_compare(a, b),
                        (Some(_), None) => std::cmp::Ordering::Greater,
                        (None, Some(_)) => std::cmp::Ordering::Less,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                } else {
                    match sort_by.field.as_str() {
                        "id" => a.key.id.cmp(&b.key.id),
                        "collection" => a.key.collection.cmp(&b.key.collection),
                        "created_at" => a.created_at.cmp(&b.created_at),
                        "updated_at" => a.updated_at.cmp(&b.updated_at),
                        "version" => a.version.cmp(&b.version),
                        _ => std::cmp::Ordering::Equal,
                    }
                };
                if asc { cmp } else { cmp.reverse() }
            });
        }

        if let Some(offset) = options.offset {
            let skip = usize::try_from(offset).unwrap_or(usize::MAX);
            objects = objects.into_iter().skip(skip).collect();
        }
        if let Some(limit) = options.limit {
            let take = usize::try_from(limit).unwrap_or(usize::MAX);
            objects.truncate(take);
        }

        Ok(objects)
    }

    fn delete_object(&mut self, collection: &str, id: &str) -> ThingdResult<bool> {
        let key = Self::make_object_key(collection, id);
        let existed = self.objects.get(&key)?.is_some();
        let _ = self.objects.remove(&key);
        Ok(existed)
    }

    fn delete_objects_batch(&mut self, keys: &[(String, String)]) -> ThingdResult<u64> {
        let mut count = 0u64;
        for (collection, id) in keys {
            let key = Self::make_object_key(collection, id);
            if self.objects.get(&key)?.is_some() {
                self.objects.remove(&key)?;
                count += 1;
            }
        }
        Ok(count)
    }

    fn count_objects(&self) -> ThingdResult<u64> {
        let mut count = 0u64;
        for kv in self.objects.iter() {
            let _ = kv;
            count += 1;
        }
        Ok(count)
    }

    fn count_objects_in_collection(&self, collection: &str) -> ThingdResult<u64> {
        let prefix = Self::make_object_key(collection, "");
        let mut count = 0u64;
        for kv in self.objects.prefix(&prefix) {
            let _ = kv;
            count += 1;
        }
        Ok(count)
    }

    fn list_collections(&self) -> ThingdResult<Vec<String>> {
        let mut collections: Vec<String> = Vec::new();
        for kv in self.objects.iter() {
            let (key, _) = guard_data(kv)?;
            let (collection, _) = Self::split_object_key(&key)?;
            if !collections.contains(&collection) {
                collections.push(collection);
            }
        }
        Ok(collections)
    }

    fn create_index(&mut self, _collection: &str, _field: &str) -> ThingdResult<()> {
        Ok(())
    }

    fn list_indexes(&self) -> ThingdResult<Vec<(String, String)>> {
        Ok(vec![])
    }

    fn schema(
        &self,
        collection: Option<&str>,
        options: &SchemaOptions,
    ) -> ThingdResult<Vec<CollectionSchema>> {
        let sample_size = options.sample_size.unwrap_or(50);
        let mut schemas: Vec<CollectionSchema> = Vec::new();

        let collections: Vec<String> = if let Some(c) = collection {
            vec![c.to_string()]
        } else {
            self.list_collections()?
        };

        for col in collections {
            let prefix = Self::make_object_key(&col, "");
            let mut objects: Vec<MemoryObject> = Vec::new();
            let mut count = 0u64;
            for kv in self.objects.prefix(&prefix) {
                count += 1;
                if objects.len() < sample_size {
                    let (_, value) = guard_data(kv)?;
                    objects.push(Self::deserialize(&value)?);
                }
            }

            let object_count = count;
            let mut fields: Vec<FieldSchema> = Vec::new();
            let mut field_types: HashMap<String, (String, bool, Vec<serde_json::Value>)> =
                HashMap::new();

            for obj in &objects {
                if let Ok(body) = serde_json::from_str::<serde_json::Value>(&obj.body)
                    && let serde_json::Value::Object(map) = body
                {
                    for (field_name, field_val) in map {
                        let entry = field_types
                            .entry(field_name.clone())
                            .or_insert_with(|| ("unknown".into(), false, Vec::new()));
                        let t = infer_json_type(&field_val);
                        if entry.0 == "unknown" {
                            entry.0 = t.clone();
                        } else if entry.0 != t {
                            entry.0 = "string".into();
                        }
                        if !field_val.is_null() && entry.2.len() < 3 {
                            entry.2.push(field_val.clone());
                        }
                    }
                }
            }

            for (name, (field_type, _, sample_values)) in field_types {
                fields.push(FieldSchema {
                    name,
                    field_type,
                    nullable: false,
                    sample_values,
                });
            }

            fields.sort_by(|a, b| a.name.cmp(&b.name));

            schemas.push(CollectionSchema {
                name: col,
                object_count,
                fields,
            });
        }

        Ok(schemas)
    }
}

// ── EventLog ─────────────────────────────────────────────────────────────────

impl EventLog for FjallEngine {
    fn is_protected_stream(&self, stream: &str) -> bool {
        stream.starts_with("__thingd:")
    }

    fn append_event(&mut self, mut event: MemoryEvent) -> ThingdResult<MemoryEvent> {
        if !event.idempotency_key.is_empty() {
            let idem_key = (event.stream.clone(), event.idempotency_key.clone());
            if let Some(&existing_seq) = self.event_idempotency_keys.get(&idem_key) {
                let ekey = Self::make_event_key(&event.stream, existing_seq);
                if let Some(data) = value_to_vec(self.events.get(&ekey)?) {
                    let existing: MemoryEvent = Self::deserialize(&data)?;
                    return Ok(existing);
                }
            }
        }

        let seq = self
            .event_seq_counters
            .entry(event.stream.clone())
            .and_modify(|s| *s += 1)
            .or_insert(1);
        event.sequence = *seq;

        if event.created_at.is_empty() {
            event.created_at = now_iso_string();
        }

        let ekey = Self::make_event_key(&event.stream, event.sequence);
        let data = Self::serialize(&event)?;
        self.events.insert(&ekey, &data)?;

        #[cfg(feature = "search")]
        self.index_event_for_search(&event);

        if !event.idempotency_key.is_empty() {
            self.event_idempotency_keys.insert(
                (event.stream.clone(), event.idempotency_key.clone()),
                event.sequence,
            );
        }

        Ok(event)
    }

    fn append_events_batch(&mut self, events: Vec<MemoryEvent>) -> ThingdResult<Vec<MemoryEvent>> {
        let mut results = Vec::with_capacity(events.len());
        for event in events {
            results.push(self.append_event(event)?);
        }
        Ok(results)
    }

    fn list_events(
        &self,
        stream: Option<&str>,
        options: ListEventsOptions,
    ) -> ThingdResult<Vec<MemoryEvent>> {
        let mut results: Vec<MemoryEvent> = Vec::new();

        if let Some(stream_name) = stream {
            for kv in self.events.iter() {
                let (key, value) = guard_data(kv)?;
                let (s, seq) = Self::split_event_key(&key)?;
                if s != stream_name {
                    continue;
                }
                if let Some(from_seq) = options.from_sequence
                    && seq <= from_seq
                {
                    continue;
                }
                let event: MemoryEvent = Self::deserialize(&value)?;
                if let Some(ref since) = options.since
                    && event.created_at.as_str() < since.as_str()
                {
                    continue;
                }
                results.push(event);
                if let Some(limit) = options.limit
                    && results.len() as u64 >= limit
                {
                    break;
                }
            }
        } else {
            for kv in self.events.iter() {
                let (_, value) = guard_data(kv)?;
                let event: MemoryEvent = Self::deserialize(&value)?;
                if let Some(ref since) = options.since
                    && event.created_at.as_str() < since.as_str()
                {
                    continue;
                }
                results.push(event);
                if let Some(limit) = options.limit
                    && results.len() as u64 >= limit
                {
                    break;
                }
            }
        }

        Ok(results)
    }

    fn delete_last_event(&mut self, stream: &str) -> ThingdResult<Option<MemoryEvent>> {
        if self.is_protected_stream(stream) {
            return Err(ThingdError::Protected(format!(
                "stream '{stream}' is protected and cannot be modified"
            )));
        }

        let mut last_key: Option<Vec<u8>> = None;
        let mut last_event: Option<MemoryEvent> = None;

        for kv in self.events.iter() {
            let (key, value) = guard_data(kv)?;
            let (s, _) = Self::split_event_key(&key)?;
            if s == stream {
                last_key = Some(key);
                last_event = Some(Self::deserialize(&value)?);
            }
        }

        if let Some(key) = last_key {
            self.events.remove(&key)?;
            Ok(last_event)
        } else {
            Ok(None)
        }
    }

    fn delete_stream(&mut self, stream: &str) -> ThingdResult<u64> {
        if self.is_protected_stream(stream) {
            return Err(ThingdError::Protected(format!(
                "stream '{stream}' is protected and cannot be modified"
            )));
        }

        let keys: Vec<Vec<u8>> = self
            .events
            .iter()
            .filter_map(|kv| {
                let (key, _) = guard_data(kv).ok()?;
                let (s, _) = Self::split_event_key(&key).ok()?;
                if s == stream { Some(key) } else { None }
            })
            .collect();

        let count = keys.len() as u64;
        for key in keys {
            self.events.remove(&key)?;
        }

        self.event_seq_counters.remove(stream);
        self.event_idempotency_keys.retain(|(s, _), _| s != stream);

        Ok(count)
    }

    fn count_events(&self) -> ThingdResult<u64> {
        let mut count = 0u64;
        for kv in self.events.iter() {
            let _ = kv;
            count += 1;
        }
        Ok(count)
    }

    fn list_streams(&self) -> ThingdResult<Vec<String>> {
        let mut streams: Vec<String> = Vec::new();
        for kv in self.events.iter() {
            let (key, _) = guard_data(kv)?;
            let (stream, _) = Self::split_event_key(&key)?;
            if !streams.contains(&stream) {
                streams.push(stream);
            }
        }
        Ok(streams)
    }
}

// ── QueueStore ───────────────────────────────────────────────────────────────

impl QueueStore for FjallEngine {
    fn push_job(&mut self, mut job: QueueJob) -> ThingdResult<QueueJob> {
        if job.created_at.is_empty() {
            job.created_at = now_iso_string();
        }
        let key = Self::make_queue_key(&job.queue, &job.id);
        let data = Self::serialize(&job)?;
        self.queue_jobs.insert(&key, &data)?;
        // Index in ready_jobs for O(1) claiming
        if job.status == QueueJobStatus::Ready {
            let rkey = Self::make_ready_key(&job.queue, job.priority, &job.created_at, &job.id);
            self.ready_jobs.insert(&rkey, [])?;
        }
        Ok(job)
    }

    fn push_jobs_batch(&mut self, jobs: Vec<QueueJob>) -> ThingdResult<Vec<QueueJob>> {
        let mut results = Vec::with_capacity(jobs.len());
        for job in jobs {
            results.push(self.push_job(job)?);
        }
        Ok(results)
    }

    fn claim_job_with_options(
        &mut self,
        queue: &str,
        options: QueueClaimOptions,
    ) -> ThingdResult<Option<QueueJob>> {
        let now = unix_timestamp_millis();

        // Release expired leases and re-index into ready_jobs
        let qprefix = Self::make_queue_key(queue, "");
        for kv in self.queue_jobs.prefix(&qprefix) {
            let (key, value) = guard_data(kv)?;
            let mut job: QueueJob = Self::deserialize(&value)?;
            if job.status == QueueJobStatus::Leased
                && job.lease_expires_at_ms.is_some_and(|exp| exp <= now)
            {
                job.status = QueueJobStatus::Ready;
                job.leased_at_ms = None;
                job.lease_expires_at_ms = None;
                let data = Self::serialize(&job)?;
                self.queue_jobs.insert(&key, &data)?;
                let rkey = Self::make_ready_key(&job.queue, job.priority, &job.created_at, &job.id);
                self.ready_jobs.insert(&rkey, [])?;
            }
        }

        // Scan ready_jobs index — first entry is highest priority, oldest
        let prefix = format!("{queue}\0");
        let mut best_ready_key: Option<Vec<u8>> = None;
        let mut best_job_id: Option<String> = None;

        for kv in self.ready_jobs.prefix(prefix.as_bytes()) {
            let (key, _) = guard_data(kv)?;
            let key_str = String::from_utf8_lossy(&key);
            // Extract job ID from key: {queue}\0{priority}\0{created_at}\0{id}
            let parts: Vec<&str> = key_str.splitn(4, '\0').collect();
            if parts.len() >= 4 {
                let job_id = parts[3].to_string();
                best_ready_key = Some(key);
                best_job_id = Some(job_id);
                break; // First entry is the best match
            }
        }

        if let (Some(rkey), Some(job_id)) = (best_ready_key, best_job_id) {
            // Read full job from queue_jobs
            let qkey = Self::make_queue_key(queue, &job_id);
            let mut job: QueueJob = match value_to_vec(self.queue_jobs.get(&qkey)?) {
                Some(data) => Self::deserialize(&data)?,
                None => return Ok(None),
            };

            // Release expired lease if this job was previously leased
            if job.status == QueueJobStatus::Leased
                && job.lease_expires_at_ms.is_some_and(|exp| exp <= now)
            {
                job.status = QueueJobStatus::Ready;
                job.leased_at_ms = None;
                job.lease_expires_at_ms = None;
            }

            // Check if job is actually claimable
            if job.status != QueueJobStatus::Ready || job.available_at_ms > now {
                // Remove stale index entry and retry
                self.ready_jobs.remove(&rkey)?;
                return self.claim_job_with_options(queue, options);
            }

            // Remove from ready index
            self.ready_jobs.remove(&rkey)?;

            // Claim the job
            job.status = QueueJobStatus::Leased;
            job.attempts = job.attempts.saturating_add(1);
            job.leased_at_ms = Some(now);
            job.lease_expires_at_ms = Some(now + options.lease_ms as i64);
            let data = Self::serialize(&job)?;
            self.queue_jobs.insert(&qkey, &data)?;
            Ok(Some(job))
        } else {
            Ok(None)
        }
    }

    fn ack_job(&mut self, queue: &str, id: &str) -> ThingdResult<Option<QueueJob>> {
        let key = Self::make_queue_key(queue, id);
        match value_to_vec(self.queue_jobs.get(&key)?) {
            Some(data) => {
                let mut job: QueueJob = Self::deserialize(&data)?;
                if job.status != QueueJobStatus::Leased {
                    return Ok(None);
                }
                job.status = QueueJobStatus::Completed;
                job.completed_at_ms = Some(unix_timestamp_millis());
                let new_data = Self::serialize(&job)?;
                self.queue_jobs.insert(&key, &new_data)?;
                Ok(Some(job))
            },
            None => Ok(None),
        }
    }

    fn nack_job_with_options(
        &mut self,
        queue: &str,
        id: &str,
        options: QueueNackOptions,
    ) -> ThingdResult<Option<QueueJob>> {
        let key = Self::make_queue_key(queue, id);
        match value_to_vec(self.queue_jobs.get(&key)?) {
            Some(data) => {
                let mut job: QueueJob = Self::deserialize(&data)?;
                if job.status != QueueJobStatus::Leased {
                    return Ok(None);
                }
                job.last_error = options.error;
                job.leased_at_ms = None;
                job.lease_expires_at_ms = None;

                let is_dead = job.attempts >= job.max_attempts;
                if is_dead {
                    job.status = QueueJobStatus::Dead;
                    job.dead_at_ms = Some(unix_timestamp_millis());
                } else {
                    job.status = QueueJobStatus::Ready;
                    job.available_at_ms = unix_timestamp_millis() + options.delay_ms as i64;
                }

                let new_data = Self::serialize(&job)?;
                self.queue_jobs.insert(&key, &new_data)?;

                // Re-index if retrying
                if !is_dead {
                    let rkey =
                        Self::make_ready_key(&job.queue, job.priority, &job.created_at, &job.id);
                    self.ready_jobs.insert(&rkey, [])?;
                }

                Ok(Some(job))
            },
            None => Ok(None),
        }
    }

    fn list_jobs(&self, queue: &str) -> ThingdResult<Vec<QueueJob>> {
        let prefix = Self::make_queue_key(queue, "");
        let mut jobs = Vec::new();
        for kv in self.queue_jobs.prefix(&prefix) {
            let (_, value) = guard_data(kv)?;
            jobs.push(Self::deserialize(&value)?);
        }
        Ok(jobs)
    }

    fn list_dead_jobs(&self, queue: &str) -> ThingdResult<Vec<QueueJob>> {
        let prefix = Self::make_queue_key(queue, "");
        let mut jobs = Vec::new();
        for kv in self.queue_jobs.prefix(&prefix) {
            let (_, value) = guard_data(kv)?;
            let job: QueueJob = Self::deserialize(&value)?;
            if job.status == QueueJobStatus::Dead {
                jobs.push(job);
            }
        }
        Ok(jobs)
    }

    fn list_queues(&self) -> ThingdResult<Vec<String>> {
        let mut queues: Vec<String> = Vec::new();
        for kv in self.queue_jobs.iter() {
            let (key, _) = guard_data(kv)?;
            let key_str = String::from_utf8_lossy(&key);
            if let Some((queue, _)) = key_str.split_once('\0')
                && !queues.contains(&queue.to_string())
            {
                queues.push(queue.to_string());
            }
        }
        Ok(queues)
    }

    fn count_active_jobs(&self) -> ThingdResult<u64> {
        let mut count = 0u64;
        for kv in self.queue_jobs.iter() {
            let (_, value) = guard_data(kv)?;
            let job: QueueJob = Self::deserialize(&value)?;
            if job.status == QueueJobStatus::Ready || job.status == QueueJobStatus::Leased {
                count += 1;
            }
        }
        Ok(count)
    }

    fn count_dead_jobs(&self) -> ThingdResult<u64> {
        let mut count = 0u64;
        for kv in self.queue_jobs.iter() {
            let (_, value) = guard_data(kv)?;
            let job: QueueJob = Self::deserialize(&value)?;
            if job.status == QueueJobStatus::Dead {
                count += 1;
            }
        }
        Ok(count)
    }
}

// ── Searcher ─────────────────────────────────────────────────────────────────

impl Searcher for FjallEngine {
    fn search(&self, query: &str, options: SearchOptions) -> ThingdResult<Vec<SearchHit>> {
        // Try Tantivy search first
        #[cfg(feature = "search")]
        if let Some(ref index) = self.search_index {
            return self.search_tantivy(index, query, options);
        }

        // Fallback: naive substring search (same as MemoryEngine)
        self.search_naive(query, options)
    }
}

impl FjallEngine {
    #[cfg(feature = "search")]
    fn index_object_for_search(&self, object: &MemoryObject) {
        let Some(ref index) = self.search_index else {
            return;
        };
        let schema = index.schema();
        let body_field = schema.get_field("body").unwrap();
        let collection_field = schema.get_field("collection").unwrap();
        let id_field = schema.get_field("id").unwrap();
        let kind_field = schema.get_field("kind").unwrap();

        let mut writer = match index.writer(50_000_000) {
            Ok(w) => w,
            Err(_) => return,
        };

        let mut doc = tantivy::TantivyDocument::new();
        doc.add_text(collection_field, &object.key.collection);
        doc.add_text(id_field, &object.key.id);
        doc.add_text(body_field, &object.body);
        doc.add_text(kind_field, "object");

        let _ = writer.add_document(doc);
        let _ = writer.commit();
    }

    #[cfg(feature = "search")]
    fn index_event_for_search(&self, event: &MemoryEvent) {
        let Some(ref index) = self.search_index else {
            return;
        };
        let schema = index.schema();
        let body_field = schema.get_field("body").unwrap();
        let collection_field = schema.get_field("collection").unwrap();
        let id_field = schema.get_field("id").unwrap();
        let kind_field = schema.get_field("kind").unwrap();

        let mut writer = match index.writer(50_000_000) {
            Ok(w) => w,
            Err(_) => return,
        };

        let mut doc = tantivy::TantivyDocument::new();
        doc.add_text(collection_field, &event.stream);
        doc.add_text(id_field, &event.sequence.to_string());
        doc.add_text(body_field, &event.body);
        doc.add_text(kind_field, "event");

        let _ = writer.add_document(doc);
        let _ = writer.commit();
    }

    #[cfg(feature = "search")]
    fn search_tantivy(
        &self,
        index: &tantivy::Index,
        query: &str,
        options: SearchOptions,
    ) -> ThingdResult<Vec<SearchHit>> {
        use tantivy::collector::DocSetCollector;
        use tantivy::query::QueryParser;
        use tantivy::schema::Value;

        let reader = index
            .reader()
            .map_err(|e| ThingdError::Storage(e.to_string()))?;
        let searcher = reader.searcher();
        let schema = index.schema();

        let body_field = schema.get_field("body").unwrap();
        let collection_field = schema.get_field("collection").unwrap();
        let id_field = schema.get_field("id").unwrap();
        let kind_field = schema.get_field("kind").unwrap();

        let mut parser = QueryParser::for_index(index, vec![body_field]);
        parser.set_conjunction_by_default();

        let tantivy_query = parser
            .parse_query(query)
            .map_err(|e| ThingdError::InvalidInput(e.to_string()))?;

        let doc_ids = searcher
            .search(&tantivy_query, &DocSetCollector)
            .map_err(|e| ThingdError::Storage(e.to_string()))?;

        let limit = options.limit.unwrap_or(10);
        let mut hits = Vec::new();

        for doc_address in doc_ids.iter().take(limit) {
            let doc = searcher
                .doc::<tantivy::TantivyDocument>(*doc_address)
                .map_err(|e| ThingdError::Storage(e.to_string()))?;

            let collection = doc
                .get_first(collection_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let id = doc
                .get_first(id_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let body = doc
                .get_first(body_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let kind = doc
                .get_first(kind_field)
                .and_then(|v| v.as_str())
                .unwrap_or("object")
                .to_string();

            if let Some(ref collections) = options.collections
                && !collections.contains(&collection)
            {
                continue;
            }

            if let Some(ref filter) = options.filter
                && !matches_filter_memory(&body, filter)
            {
                continue;
            }

            let col = collection.clone();
            let doc_id = id.clone();
            let doc_body = body.clone();

            if kind == "object"
                && let Some(obj_data) = self
                    .objects
                    .get(Self::make_object_key(&col, &doc_id))
                    .ok()
                    .and_then(value_to_vec)
                && let Ok(obj) = Self::deserialize::<MemoryObject>(&obj_data)
            {
                hits.push(SearchHit {
                    kind: "object".to_string(),
                    collection: col,
                    id: doc_id,
                    text: doc_body.clone(),
                    score: 1.0,
                    body: doc_body,
                    version: Some(obj.version),
                    created_at: obj.created_at,
                    updated_at: Some(obj.updated_at),
                    event_type: None,
                });
            } else if kind == "event"
                && let Ok(seq) = id.parse::<u64>()
                && let Some(ev_data) = self
                    .events
                    .get(Self::make_event_key(&collection, seq))
                    .ok()
                    .and_then(value_to_vec)
                && let Ok(event) = Self::deserialize::<MemoryEvent>(&ev_data)
            {
                hits.push(SearchHit {
                    kind: "event".to_string(),
                    collection: collection.clone(),
                    id: seq.to_string(),
                    text: body.clone(),
                    score: 1.0,
                    body: body.clone(),
                    version: None,
                    created_at: event.created_at,
                    updated_at: None,
                    event_type: Some(event.event_type),
                });
            }
        }

        Ok(hits)
    }

    fn search_naive(&self, query: &str, options: SearchOptions) -> ThingdResult<Vec<SearchHit>> {
        let query_words: Vec<String> = query
            .split_whitespace()
            .map(|w| {
                w.to_lowercase()
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect()
            })
            .filter(|w: &String| !w.is_empty())
            .collect();

        if query_words.is_empty() {
            return Ok(Vec::new());
        }

        let mut hits = Vec::new();

        for kv in self.objects.iter() {
            let (_, value) = guard_data(kv)?;
            let object: MemoryObject = Self::deserialize(&value)?;

            if let Some(ref collections) = options.collections
                && !collections.contains(&object.key.collection)
            {
                continue;
            }

            if let Some(ref filter) = options.filter
                && !matches_filter_memory(&object.body, filter)
            {
                continue;
            }

            let text_to_search = format!(
                "{} {} {}",
                object.key.collection, object.key.id, object.body
            )
            .to_lowercase();
            let matches_all = query_words.iter().all(|word| text_to_search.contains(word));

            if matches_all {
                hits.push(SearchHit {
                    kind: "object".to_string(),
                    collection: object.key.collection.clone(),
                    id: object.key.id.clone(),
                    text: object.body.clone(),
                    score: 1.0,
                    body: object.body.clone(),
                    version: Some(object.version),
                    created_at: object.created_at.clone(),
                    updated_at: Some(object.updated_at.clone()),
                    event_type: None,
                });
            }
        }

        for kv in self.events.iter() {
            let (_, value) = guard_data(kv)?;
            let event: MemoryEvent = Self::deserialize(&value)?;

            if let Some(ref collections) = options.collections
                && !collections.contains(&event.stream)
            {
                continue;
            }

            if let Some(ref filter) = options.filter
                && !matches_filter_memory(&event.body, filter)
            {
                continue;
            }

            let text_to_search =
                format!("{} {} {}", event.stream, event.event_type, event.body).to_lowercase();
            let matches_all = query_words.iter().all(|word| text_to_search.contains(word));

            if matches_all {
                hits.push(SearchHit {
                    kind: "event".to_string(),
                    collection: event.stream.clone(),
                    id: event.sequence.to_string(),
                    text: event.body.clone(),
                    score: 1.0,
                    body: event.body.clone(),
                    version: None,
                    created_at: event.created_at.clone(),
                    updated_at: None,
                    event_type: Some(event.event_type.clone()),
                });
            }
        }

        if let Some(limit) = options.limit {
            hits.truncate(limit);
        }

        Ok(hits)
    }
}

// ── LinkStore ────────────────────────────────────────────────────────────────

impl LinkStore for FjallEngine {
    fn create_link(&mut self, mut link: Link) -> ThingdResult<Link> {
        let id = self.next_link_id.fetch_add(1, Ordering::Relaxed);
        link.id = format!("link-{id}");
        if link.created_at.is_empty() {
            link.created_at = now_iso_string();
        }

        let data = Self::serialize(&link)?;

        self.links_by_id.insert(link.id.as_bytes(), &data)?;
        self.links_from.insert(
            Self::make_link_from_key(&link.from_ref, &link.link_type, &link.id),
            [],
        )?;
        self.links_to.insert(
            Self::make_link_to_key(&link.to_ref, &link.link_type, &link.id),
            [],
        )?;

        Ok(link)
    }

    fn delete_link(&mut self, id: &str) -> ThingdResult<bool> {
        if let Some(data) = value_to_vec(self.links_by_id.get(id.as_bytes())?) {
            let link: Link = Self::deserialize(&data)?;
            self.links_by_id.remove(id.as_bytes())?;
            self.links_from.remove(Self::make_link_from_key(
                &link.from_ref,
                &link.link_type,
                id,
            ))?;
            self.links_to
                .remove(Self::make_link_to_key(&link.to_ref, &link.link_type, id))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn get_link(&self, id: &str) -> ThingdResult<Option<Link>> {
        match value_to_vec(self.links_by_id.get(id.as_bytes())?) {
            Some(data) => Ok(Some(Self::deserialize(&data)?)),
            None => Ok(None),
        }
    }

    fn get_neighbors(
        &self,
        reference: &str,
        direction: LinkDirection,
        options: LinkQueryOptions,
    ) -> ThingdResult<Vec<Link>> {
        let mut results: Vec<Link> = Vec::new();
        let ref_prefix = format!("{reference}\0");

        if direction == LinkDirection::Outgoing || direction == LinkDirection::Both {
            for kv in self.links_from.prefix(ref_prefix.as_bytes()) {
                let (key, _) = guard_data(kv)?;
                let key_str = String::from_utf8_lossy(&key);
                let parts: Vec<&str> = key_str.splitn(3, '\0').collect();
                if parts.len() >= 3 {
                    let link_type = parts[1];
                    let link_id = parts[2];

                    if let Some(ref lt) = options.link_type
                        && link_type != lt.as_str()
                    {
                        continue;
                    }

                    if let Some(data) = value_to_vec(self.links_by_id.get(link_id.as_bytes())?) {
                        let link: Link = Self::deserialize(&data)?;
                        results.push(link);
                    }
                }
            }
        }

        if direction == LinkDirection::Incoming || direction == LinkDirection::Both {
            for kv in self.links_to.prefix(ref_prefix.as_bytes()) {
                let (key, _) = guard_data(kv)?;
                let key_str = String::from_utf8_lossy(&key);
                let parts: Vec<&str> = key_str.splitn(3, '\0').collect();
                if parts.len() >= 3 {
                    let link_type = parts[1];
                    let link_id = parts[2];

                    if let Some(ref lt) = options.link_type
                        && link_type != lt.as_str()
                    {
                        continue;
                    }

                    if let Some(data) = value_to_vec(self.links_by_id.get(link_id.as_bytes())?) {
                        let link: Link = Self::deserialize(&data)?;
                        results.push(link);
                    }
                }
            }
        }

        if let Some(limit) = options.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    fn count_links(&self) -> ThingdResult<u64> {
        let mut count = 0u64;
        for kv in self.links_by_id.iter() {
            let _ = kv;
            count += 1;
        }
        Ok(count)
    }
}

// ── AggregateStore ───────────────────────────────────────────────────────────

impl AggregateStore for FjallEngine {
    fn aggregate(
        &self,
        collection: &str,
        options: &AggregateOptions,
    ) -> ThingdResult<AggregateResult> {
        let prefix = Self::make_object_key(collection, "");
        let mut objects: Vec<MemoryObject> = Vec::new();

        for kv in self.objects.prefix(&prefix) {
            let (_, value) = guard_data(kv)?;
            let obj: MemoryObject = Self::deserialize(&value)?;

            if options.filter.is_empty() {
                objects.push(obj);
            } else {
                let Ok(body) = serde_json::from_str::<serde_json::Value>(&obj.body) else {
                    continue;
                };
                let matches = options
                    .filter
                    .iter()
                    .all(|(key, expected)| body.get(key.as_str()).is_some_and(|v| v == expected));
                if matches {
                    objects.push(obj);
                }
            }
        }

        if let Some(group_field) = &options.group_by {
            let mut groups: HashMap<String, Vec<MemoryObject>> = HashMap::new();
            for obj in &objects {
                let key = extract_field_str(&obj.body, group_field);
                groups.entry(key).or_default().push(obj.clone());
            }

            let mut group_results: Vec<AggregateGroupResult> = groups
                .iter()
                .map(|(key, objs)| AggregateGroupResult {
                    key: key.clone(),
                    value: compute_aggregate(objs, options.function, options.field.as_deref()),
                })
                .collect();
            group_results.sort_by(|a, b| {
                b.value
                    .partial_cmp(&a.value)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let total = group_results.iter().map(|g| g.value).sum();

            Ok(AggregateResult {
                total,
                groups: group_results,
            })
        } else {
            let total = compute_aggregate(&objects, options.function, options.field.as_deref());
            Ok(AggregateResult {
                total,
                groups: vec![],
            })
        }
    }

    fn timeseries(
        &self,
        collection: &str,
        options: &TimeSeriesOptions,
    ) -> ThingdResult<TimeSeriesResult> {
        let prefix = Self::make_object_key(collection, "");
        let mut bucket_map: HashMap<String, Vec<f64>> = HashMap::new();

        for kv in self.objects.prefix(&prefix) {
            let (_, value) = guard_data(kv)?;
            let obj: MemoryObject = Self::deserialize(&value)?;

            if !options.filter.is_empty() {
                let Ok(body) = serde_json::from_str::<serde_json::Value>(&obj.body) else {
                    continue;
                };
                let matches = options
                    .filter
                    .iter()
                    .all(|(key, expected)| body.get(key.as_str()).is_some_and(|v| v == expected));
                if !matches {
                    continue;
                }
            }

            let bucket_label = bucket_label_for_date(&obj.created_at, options.bucket);

            if let Some(ref from) = options.from
                && bucket_label.as_str() < from.as_str()
            {
                continue;
            }
            if let Some(ref to) = options.to
                && bucket_label.as_str() > to.as_str()
            {
                continue;
            }

            if options.function == AggregateFunction::Count {
                bucket_map.entry(bucket_label).or_default().push(1.0);
            } else if let Some(ref field) = options.field
                && let Ok(body) = serde_json::from_str::<serde_json::Value>(&obj.body)
                && let Some(val) = body.get(field.as_str()).and_then(serde_json::Value::as_f64)
            {
                bucket_map.entry(bucket_label).or_default().push(val);
            }
        }

        let mut bucket_list: Vec<(String, f64)> = bucket_map
            .into_iter()
            .map(|(label, values)| {
                let value = match options.function {
                    AggregateFunction::Count => values.len() as f64,
                    AggregateFunction::Sum => values.iter().sum(),
                    AggregateFunction::Avg => {
                        if values.is_empty() {
                            0.0
                        } else {
                            values.iter().sum::<f64>() / values.len() as f64
                        }
                    },
                    AggregateFunction::Min => values.iter().copied().fold(f64::MAX, f64::min),
                    AggregateFunction::Max => {
                        values.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                    },
                };
                (label, value)
            })
            .collect();

        bucket_list.sort_by(|a, b| a.0.cmp(&b.0));

        let buckets: Vec<TimeSeriesBucket> = bucket_list
            .into_iter()
            .map(|(label, value)| TimeSeriesBucket { label, value })
            .collect();

        Ok(TimeSeriesResult { buckets })
    }
}

// ── Helper functions ─────────────────────────────────────────────────────────

fn value_compare(a: &serde_json::Value, b: &serde_json::Value) -> std::cmp::Ordering {
    match (a, b) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            let a_f = a.as_f64().unwrap_or(0.0);
            let b_f = b.as_f64().unwrap_or(0.0);
            a_f.partial_cmp(&b_f).unwrap_or(std::cmp::Ordering::Equal)
        },
        (serde_json::Value::String(a), serde_json::Value::String(b)) => a.cmp(b),
        (serde_json::Value::Bool(a), serde_json::Value::Bool(b)) => a.cmp(b),
        _ => format!("{a}").cmp(&format!("{b}")),
    }
}

fn like_match(s: &str, pattern: &str) -> bool {
    let parts = pattern.split('%');
    let mut pos = 0;
    for part in parts {
        if part.is_empty() {
            continue;
        }
        if let Some(idx) = s[pos..].find(part) {
            pos += idx + part.len();
        } else {
            return false;
        }
    }
    true
}

fn matches_filter_memory(body_str: &str, filter: &serde_json::Value) -> bool {
    let Ok(body) = serde_json::from_str::<serde_json::Value>(body_str) else {
        return false;
    };
    if let serde_json::Value::Object(map) = filter {
        map.iter()
            .all(|(key, expected)| body.get(key.as_str()).is_some_and(|v| v == expected))
    } else {
        false
    }
}

fn extract_field_str(body_str: &str, field: &str) -> String {
    if let Ok(body) = serde_json::from_str::<serde_json::Value>(body_str)
        && let Some(val) = body.get(field)
    {
        if let Some(s) = val.as_str() {
            return s.to_string();
        }
        return format!("{val}");
    }
    String::new()
}

fn compute_aggregate(
    objects: &[MemoryObject],
    function: AggregateFunction,
    field: Option<&str>,
) -> f64 {
    if function == AggregateFunction::Count {
        objects.len() as f64
    } else {
        let values: Vec<f64> = objects
            .iter()
            .filter_map(|obj| {
                field.and_then(|f| {
                    let Ok(body) = serde_json::from_str::<serde_json::Value>(&obj.body) else {
                        return None;
                    };
                    body.get(f).and_then(serde_json::Value::as_f64)
                })
            })
            .collect();

        match function {
            AggregateFunction::Sum => values.iter().sum(),
            AggregateFunction::Avg => {
                if values.is_empty() {
                    0.0
                } else {
                    values.iter().sum::<f64>() / values.len() as f64
                }
            },
            AggregateFunction::Min => values.iter().copied().fold(f64::MAX, f64::min),
            AggregateFunction::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            _ => values.len() as f64,
        }
    }
}

fn infer_json_type(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(_) => "boolean".to_string(),
        serde_json::Value::Number(_) => "number".to_string(),
        serde_json::Value::String(s) => {
            if s.parse::<chrono::DateTime<chrono::Utc>>().is_ok() {
                "date".to_string()
            } else {
                "string".to_string()
            }
        },
        serde_json::Value::Array(_) => "array".to_string(),
        serde_json::Value::Object(_) => "object".to_string(),
    }
}

fn bucket_label_for_date(iso_date: &str, bucket: TimeBucket) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(iso_date) {
        match bucket {
            TimeBucket::Hour => dt.format("%Y-%m-%dT%H:00:00Z").to_string(),
            TimeBucket::Day => dt.format("%Y-%m-%d").to_string(),
            TimeBucket::Week => dt.format("%Y-W%V").to_string(),
            TimeBucket::Month => dt.format("%Y-%m").to_string(),
        }
    } else {
        iso_date.to_string()
    }
}
