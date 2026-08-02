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
    QueueJobStatus, QueueNackOptions, SearchHit, ThingdError, VectorSearchHit, VectorSearchOptions,
};
use crate::{now_iso_string, unix_timestamp_millis};

/// Persistent storage engine implementing all 6 storage traits.
///
/// Data directory layout:
/// - `objects`: `{collection}\0{id}` → serialized `MemoryObject`
/// - `events`: `{stream}\0{seq:8BE}` → serialized `MemoryEvent`
/// - `queue_jobs`: `{queue}\0{id}` → serialized `QueueJob`
/// - `links_by_id`: `{link_id}` → serialized `Link`
/// - `links_from`: `{from_ref}\0{type}\0{link_id}` → `()`
/// - `links_to`: `{to_ref}\0{type}\0{link_id}` → `()`
pub struct PersistentEngine {
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
    #[cfg(feature = "vectors")]
    vectors: Keyspace,
}

fn value_to_vec(v: Option<fjall::Slice>) -> Option<Vec<u8>> {
    v.map(|c| c.to_vec())
}

impl PersistentEngine {
    /// Open or create a Persistent database at the given path.
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

        #[cfg(feature = "vectors")]
        let vectors = db.keyspace("vectors", KeyspaceCreateOptions::default)?;

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

        // Reconstruct event sequence counters and idempotency keys from durable data.
        // This ensures sequences continue monotonic and idempotency dedup survives reopen.
        let mut event_seq_counters: HashMap<String, u64> = HashMap::new();
        let mut event_idempotency_keys: HashMap<(String, String), u64> = HashMap::new();
        for kv in events.iter() {
            if let Ok((key, value)) = guard_data(kv)
                && let Ok((stream, seq)) = Self::split_event_key(&key)
            {
                event_seq_counters
                    .entry(stream.clone())
                    .and_modify(|max| *max = (*max).max(seq))
                    .or_insert(seq);
                if let Ok(event) = Self::deserialize::<MemoryEvent>(&value)
                    && !event.idempotency_key.is_empty()
                {
                    event_idempotency_keys.insert((stream, event.idempotency_key), seq);
                }
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
            event_seq_counters,
            event_idempotency_keys,
            #[cfg(feature = "search")]
            search_index,
            #[cfg(feature = "vectors")]
            vectors,
        })
    }

    #[cfg(feature = "search")]
    fn init_search_index(path: &Path) -> Option<tantivy::Index> {
        let search_dir = path.join("search");
        let _ = std::fs::create_dir_all(&search_dir);

        // Try to open an existing Tantivy index first; create a fresh one if absent
        if let Ok(index) = tantivy::Index::open_in_dir(&search_dir) {
            return Some(index);
        }

        let mut schema_builder = tantivy::schema::Schema::builder();
        schema_builder.add_text_field("doc_key", tantivy::schema::STRING | tantivy::schema::STORED);
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

    #[cfg(feature = "vectors")]
    fn make_vector_key(collection: &str, id: &str) -> Vec<u8> {
        format!("{collection}\0{id}").into_bytes()
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

impl ObjectStore for PersistentEngine {
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

        // Atomic batch: object data + vector state
        let mut batch = self.db.batch();
        batch.insert(&self.objects, &key, &data);
        #[cfg(feature = "vectors")]
        {
            let vkey = Self::make_vector_key(&object.key.collection, &object.key.id);
            if let Some(ref vector) = object.vector {
                let vdata = Self::serialize(vector)?;
                batch.insert(&self.vectors, &vkey, vdata);
            } else {
                batch.remove(&self.vectors, vkey);
            }
        }
        batch
            .commit()
            .map_err(|e| ThingdError::Storage(e.to_string()))?;

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

        // Atomic batch: object + vector removal
        let mut batch = self.db.batch();
        batch.remove(&self.objects, key);
        #[cfg(feature = "vectors")]
        {
            let vkey = Self::make_vector_key(collection, id);
            batch.remove(&self.vectors, vkey);
        }
        batch
            .commit()
            .map_err(|e| ThingdError::Storage(e.to_string()))?;

        #[cfg(feature = "search")]
        self.delete_object_from_search_index(collection, id);
        Ok(existed)
    }

    fn delete_objects_batch(&mut self, keys: &[(String, String)]) -> ThingdResult<u64> {
        let mut count = 0u64;
        let mut batch = self.db.batch();

        for (collection, id) in keys {
            let key = Self::make_object_key(collection, id);
            if self.objects.get(&key)?.is_some() {
                batch.remove(&self.objects, key);
                count += 1;
            }
            #[cfg(feature = "vectors")]
            {
                let vkey = Self::make_vector_key(collection, id);
                batch.remove(&self.vectors, vkey);
            }
        }

        if count > 0 {
            batch
                .commit()
                .map_err(|e| ThingdError::Storage(e.to_string()))?;
        }

        #[cfg(feature = "search")]
        for (collection, id) in keys {
            self.delete_object_from_search_index(collection, id);
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

impl EventLog for PersistentEngine {
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
            #[cfg(feature = "search")]
            if let Some(ref ev) = last_event {
                self.delete_event_from_search_index(&ev.stream, ev.sequence);
            }
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

        let entries: Vec<(Vec<u8>, u64)> = self
            .events
            .iter()
            .filter_map(|kv| {
                guard_data(kv)
                    .ok()
                    .and_then(|(key, _)| Self::split_event_key(&key).ok())
                    .filter(|(s, _)| s == stream)
                    .map(|(_, seq)| {
                        let ekey = Self::make_event_key(stream, seq);
                        (ekey, seq)
                    })
            })
            .collect();

        let count = entries.len() as u64;
        for (key, seq) in &entries {
            self.events.remove(key)?;
            #[cfg(feature = "search")]
            self.delete_event_from_search_index(stream, *seq);
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

impl QueueStore for PersistentEngine {
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

        // Scan ready_jobs index in priority order, skipping delayed and stale entries
        let prefix = format!("{queue}\0");
        for kv in self.ready_jobs.prefix(prefix.as_bytes()) {
            let (key, _) = guard_data(kv)?;
            let key_str = String::from_utf8_lossy(&key);
            let parts: Vec<&str> = key_str.splitn(4, '\0').collect();
            if parts.len() < 4 {
                continue;
            }
            let job_id = parts[3].to_string();
            let rkey = key;

            // Read full job from queue_jobs
            let qkey = Self::make_queue_key(queue, &job_id);
            let Some(job_data) = value_to_vec(self.queue_jobs.get(&qkey)?) else {
                // Job record missing — remove stale index entry
                let _ = self.ready_jobs.remove(&rkey);
                continue;
            };
            let mut job: QueueJob = Self::deserialize(&job_data)?;
            // Release expired lease if this job was previously leased
            if job.status == QueueJobStatus::Leased
                && job.lease_expires_at_ms.is_some_and(|exp| exp <= now)
            {
                job.status = QueueJobStatus::Ready;
                job.leased_at_ms = None;
                job.lease_expires_at_ms = None;
            }

            // Skip (and remove) stale entries for completed or dead jobs
            if job.status != QueueJobStatus::Ready {
                let _ = self.ready_jobs.remove(&rkey);
                continue;
            }

            // Job is delayed — skip it but keep the index entry so it can be claimed later
            if job.available_at_ms > now {
                continue;
            }

            // Claim this job
            self.ready_jobs.remove(&rkey)?;
            job.status = QueueJobStatus::Leased;
            job.attempts = job.attempts.saturating_add(1);
            job.leased_at_ms = Some(now);
            job.lease_expires_at_ms = Some(now + options.lease_ms as i64);
            let data = Self::serialize(&job)?;
            self.queue_jobs.insert(&qkey, &data)?;
            return Ok(Some(job));
        }

        Ok(None)
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

impl Searcher for PersistentEngine {
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

impl PersistentEngine {
    #[cfg(feature = "search")]
    fn index_object_for_search(&self, object: &MemoryObject) {
        let Some(ref index) = self.search_index else {
            return;
        };
        let schema = index.schema();
        let doc_key_field = schema.get_field("doc_key").unwrap();
        let body_field = schema.get_field("body").unwrap();
        let collection_field = schema.get_field("collection").unwrap();
        let id_field = schema.get_field("id").unwrap();
        let kind_field = schema.get_field("kind").unwrap();

        let mut writer = match index.writer(50_000_000) {
            Ok(w) => w,
            Err(_) => return,
        };

        let doc_key = format!("{}/{}", object.key.collection, object.key.id);
        // Remove existing document with the same doc_key to prevent duplicates
        let term = tantivy::Term::from_field_text(doc_key_field, &doc_key);
        let _ = writer.delete_term(term);

        let mut doc = tantivy::TantivyDocument::new();
        doc.add_text(doc_key_field, &doc_key);
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
        let doc_key_field = schema.get_field("doc_key").unwrap();
        let body_field = schema.get_field("body").unwrap();
        let collection_field = schema.get_field("collection").unwrap();
        let id_field = schema.get_field("id").unwrap();
        let kind_field = schema.get_field("kind").unwrap();

        let mut writer = match index.writer(50_000_000) {
            Ok(w) => w,
            Err(_) => return,
        };

        let mut doc = tantivy::TantivyDocument::new();
        let doc_key = format!("event:{}/{}", event.stream, event.sequence);
        doc.add_text(doc_key_field, &doc_key);
        doc.add_text(collection_field, &event.stream);
        doc.add_text(id_field, event.sequence.to_string());
        doc.add_text(body_field, &event.body);
        doc.add_text(kind_field, "event");

        let _ = writer.add_document(doc);
        let _ = writer.commit();
    }

    #[cfg(feature = "search")]
    fn delete_event_from_search_index(&self, stream: &str, sequence: u64) {
        let Some(ref index) = self.search_index else {
            return;
        };
        let schema = index.schema();
        let doc_key_field = schema.get_field("doc_key").unwrap();

        let mut writer: tantivy::IndexWriter<tantivy::TantivyDocument> =
            match index.writer(50_000_000) {
                Ok(w) => w,
                Err(_) => return,
            };

        let doc_key = format!("event:{stream}/{sequence}");
        let term = tantivy::Term::from_field_text(doc_key_field, &doc_key);
        let _ = writer.delete_term(term);
        let _ = writer.commit();
    }

    #[cfg(feature = "search")]
    fn delete_object_from_search_index(&self, collection: &str, id: &str) {
        let Some(ref index) = self.search_index else {
            return;
        };
        let schema = index.schema();
        let doc_key_field = schema.get_field("doc_key").unwrap();

        let mut writer: tantivy::IndexWriter<tantivy::TantivyDocument> =
            match index.writer(50_000_000) {
                Ok(w) => w,
                Err(_) => return,
            };

        let doc_key = format!("{collection}/{id}");
        let term = tantivy::Term::from_field_text(doc_key_field, &doc_key);
        let _ = writer.delete_term(term);
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

impl LinkStore for PersistentEngine {
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

impl AggregateStore for PersistentEngine {
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

// ── VectorStore ──────────────────────────────────────────────────────────────

impl crate::store::VectorStore for PersistentEngine {
    fn vector_search(
        &self,
        collection: &str,
        query_vector: &[f32],
        options: VectorSearchOptions,
    ) -> ThingdResult<Vec<VectorSearchHit>> {
        if query_vector.is_empty() {
            return Err(ThingdError::InvalidInput(
                "query vector must not be empty".to_string(),
            ));
        }

        #[cfg(not(feature = "vectors"))]
        {
            let _ = (collection, query_vector, options);
            Ok(vec![])
        }

        #[cfg(feature = "vectors")]
        {
            let prefix = Self::make_vector_key(collection, "");
            let mut hits: Vec<VectorSearchHit> = Vec::new();

            for kv in self.vectors.prefix(&prefix) {
                let (key, value) = guard_data(kv)?;
                let key_str = String::from_utf8_lossy(&key);
                let Some((_, id)) = key_str.split_once('\0') else {
                    continue;
                };

                let vector: Vec<f32> = Self::deserialize(&value)?;

                if vector.len() != query_vector.len() {
                    return Err(ThingdError::InvalidInput(format!(
                        "query vector dimension {} does not match stored vector dimension {}",
                        query_vector.len(),
                        vector.len()
                    )));
                }

                let Some(object) = self.get_object(collection, id)? else {
                    continue;
                };

                if let Some(ref filter) = options.filter
                    && !matches_filter_memory(&object.body, filter)
                {
                    continue;
                }

                let score = crate::cosine_similarity(query_vector, &vector);
                hits.push(VectorSearchHit {
                    id: id.to_string(),
                    score,
                    value: object,
                });
            }

            hits.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            if let Some(top_k) = options.top_k {
                hits.truncate(top_k);
            }

            Ok(hits)
        }
    }

    fn add_vector(&mut self, collection: &str, id: &str, vector: &[f32]) -> ThingdResult<()> {
        #[cfg(not(feature = "vectors"))]
        {
            let _ = (collection, id, vector);
        }

        #[cfg(feature = "vectors")]
        {
            let vkey = Self::make_vector_key(collection, id);
            let vdata = Self::serialize(&vector.to_vec())?;
            self.vectors.insert(&vkey, &vdata)?;
        }

        Ok(())
    }

    fn remove_vector(&mut self, collection: &str, id: &str) -> ThingdResult<()> {
        #[cfg(not(feature = "vectors"))]
        {
            let _ = (collection, id);
        }

        #[cfg(feature = "vectors")]
        {
            let vkey = Self::make_vector_key(collection, id);
            let _ = self.vectors.remove(&vkey);
        }

        Ok(())
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

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::cast_precision_loss)]
mod tests {
    use super::*;
    #[cfg(feature = "vectors")]
    use crate::VectorSearchOptions;
    #[cfg(feature = "vectors")]
    use crate::store::VectorStore;
    use crate::store::{AggregateStore, EventLog, LinkStore, ObjectStore, QueueStore, Searcher};
    use crate::{
        Link, ListObjectsOptions, MemoryEvent, MemoryObject, QueueClaimOptions, QueueJob,
        QueueJobStatus, QueueNackOptions, SearchOptions, TimeBucket,
    };

    /// Create a test engine with a temp directory that stays alive for the caller.
    fn setup() -> (PersistentEngine, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let engine = PersistentEngine::open(dir.path()).unwrap();
        (engine, dir)
    }

    // ── ObjectStore ───────────────────────────────────────────────────────

    #[test]
    fn persistent_stores_and_reads_objects() {
        let (mut engine, _dir) = setup();
        let object = engine
            .put_object(MemoryObject::new(
                "decisions",
                "rust-core",
                r#"{"text":"Use Rust"}"#,
            ))
            .unwrap();
        let stored = engine
            .get_object("decisions", "rust-core")
            .unwrap()
            .unwrap();
        assert_eq!(object.version, 1);
        assert_eq!(stored.key.collection, "decisions");
        assert_eq!(stored.key.id, "rust-core");
    }

    #[test]
    fn persistent_object_created_at_preserved_on_update() {
        let (mut engine, _dir) = setup();
        let first = engine
            .put_object(MemoryObject::new("col", "id", r#"{"v":1}"#))
            .unwrap();
        assert!(!first.created_at.is_empty());
        let second = engine
            .put_object(MemoryObject::new("col", "id", r#"{"v":2}"#))
            .unwrap();
        assert_eq!(second.created_at, first.created_at);
        assert!(second.updated_at >= first.created_at);
    }

    #[test]
    fn persistent_object_version_increments_on_update() {
        let (mut engine, _dir) = setup();
        let v1 = engine
            .put_object(MemoryObject::new("col", "x", "{}"))
            .unwrap();
        assert_eq!(v1.version, 1);
        let v2 = engine
            .put_object(MemoryObject::new("col", "x", r#"{"v":2}"#))
            .unwrap();
        assert_eq!(v2.version, 2);
    }

    #[test]
    fn persistent_lists_objects_with_filter() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("w", "a", r#"{"color":"red","size":1}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("w", "b", r#"{"color":"blue","size":2}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("w", "c", r#"{"color":"red","size":3}"#))
            .unwrap();
        let opts = ListObjectsOptions {
            filter: vec![("color".into(), serde_json::json!("red"))],
            ..Default::default()
        };
        let results = engine
            .list_objects(Some(&["w".to_string()]), &opts)
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|o| o.body.contains("\"red\"")));
    }

    #[test]
    fn persistent_list_objects_pagination() {
        let (mut engine, _dir) = setup();
        for i in 0..5u32 {
            engine
                .put_object(MemoryObject::new("col", format!("id-{i}"), "{}"))
                .unwrap();
        }
        let limit_opts = ListObjectsOptions {
            limit: Some(3),
            ..Default::default()
        };
        assert_eq!(
            engine
                .list_objects(Some(&["col".to_string()]), &limit_opts)
                .unwrap()
                .len(),
            3
        );
        let offset_opts = ListObjectsOptions {
            offset: Some(3),
            ..Default::default()
        };
        assert_eq!(
            engine
                .list_objects(Some(&["col".to_string()]), &offset_opts)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn persistent_list_objects_sort_by_created_at_desc() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("w", "a", r#"{"x":1}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("w", "b", r#"{"x":2}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("w", "c", r#"{"x":3}"#))
            .unwrap();
        let opts = ListObjectsOptions {
            sort_by: Some(crate::SortBy::desc("created_at")),
            ..Default::default()
        };
        let results = engine
            .list_objects(Some(&["w".to_string()]), &opts)
            .unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn persistent_list_objects_sort_by_id_asc() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("w", "c", r#"{"x":3}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("w", "a", r#"{"x":1}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("w", "b", r#"{"x":2}"#))
            .unwrap();
        let opts = ListObjectsOptions {
            sort_by: Some(crate::SortBy::asc("id")),
            ..Default::default()
        };
        let results = engine
            .list_objects(Some(&["w".to_string()]), &opts)
            .unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].key.id, "a");
        assert_eq!(results[1].key.id, "b");
        assert_eq!(results[2].key.id, "c");
    }

    #[test]
    fn persistent_cas_succeeds_on_matching_version() {
        let (mut engine, _dir) = setup();
        let stored = engine
            .put_object(MemoryObject::new("col", "id", r#"{"v":1}"#))
            .unwrap();
        assert_eq!(stored.version, 1);
        let opts = crate::PutObjectOptions {
            expected_version: Some(1),
            ..Default::default()
        };
        let updated = engine
            .put_object_with_options(MemoryObject::new("col", "id", r#"{"v":2}"#), opts)
            .unwrap();
        assert_eq!(updated.version, 2);
    }

    #[test]
    fn persistent_cas_fails_on_version_mismatch() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("col", "id", r#"{"v":1}"#))
            .unwrap();
        let opts = crate::PutObjectOptions {
            expected_version: Some(42),
            ..Default::default()
        };
        let err = engine
            .put_object_with_options(MemoryObject::new("col", "id", r#"{"v":2}"#), opts)
            .unwrap_err();
        assert!(matches!(err, crate::ThingdError::Conflict(_)));
    }

    #[test]
    fn persistent_cas_fails_on_nonexistent_object() {
        let (mut engine, _dir) = setup();
        let opts = crate::PutObjectOptions {
            expected_version: Some(1),
            ..Default::default()
        };
        let err = engine
            .put_object_with_options(MemoryObject::new("col", "id", r#"{"v":1}"#), opts)
            .unwrap_err();
        assert!(matches!(err, crate::ThingdError::Conflict(_)));
    }

    #[test]
    fn persistent_delete_objects_batch() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("w", "a", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("w", "b", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("w", "c", "{}"))
            .unwrap();
        let keys = vec![
            ("w".to_string(), "a".to_string()),
            ("w".to_string(), "b".to_string()),
        ];
        let deleted = engine.delete_objects_batch(&keys).unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(engine.count_objects().unwrap(), 1);
        assert!(engine.get_object("w", "a").unwrap().is_none());
        assert!(engine.get_object("w", "c").unwrap().is_some());
    }

    // ── EventLog ──────────────────────────────────────────────────────────

    #[test]
    fn persistent_appends_events_with_sequence_numbers() {
        let (mut engine, _dir) = setup();
        let event = engine
            .append_event(MemoryEvent::new(
                "project:thingd",
                "decision.made",
                "MCP-native object storage",
            ))
            .unwrap();
        assert_eq!(event.sequence, 1);
        assert_eq!(
            engine
                .list_events(Some("project:thingd"), ListEventsOptions::default())
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn persistent_event_idempotency() {
        let (mut engine, _dir) = setup();
        let mut event = MemoryEvent::new("stream", "test", r#"{"key":"val"}"#);
        event.idempotency_key = "idem-1".to_string();
        let first = engine.append_event(event.clone()).unwrap();
        assert_eq!(first.sequence, 1);
        let second = engine.append_event(event).unwrap();
        assert_eq!(second.sequence, first.sequence);
        assert_eq!(second.body, first.body);
    }

    #[test]
    fn persistent_deletes_last_event_from_stream() {
        let (mut engine, _dir) = setup();
        engine
            .append_event(MemoryEvent::new("match:1", "turn.recorded", "{}"))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("match:1", "turn.recorded", "{}"))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("match:2", "turn.recorded", "{}"))
            .unwrap();
        let deleted = engine.delete_last_event("match:1").unwrap().unwrap();
        assert_eq!(deleted.sequence, 2);
        let remaining = engine
            .list_events(Some("match:1"), ListEventsOptions::default())
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].sequence, 1);
        let match2 = engine
            .list_events(Some("match:2"), ListEventsOptions::default())
            .unwrap();
        assert_eq!(match2.len(), 1);
    }

    #[test]
    fn persistent_deletes_stream_and_returns_count() {
        let (mut engine, _dir) = setup();
        engine
            .append_event(MemoryEvent::new("match:1", "turn.recorded", "{}"))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("match:1", "turn.recorded", "{}"))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("match:2", "turn.recorded", "{}"))
            .unwrap();
        assert_eq!(engine.delete_stream("match:1").unwrap(), 2);
        assert_eq!(
            engine
                .list_events(Some("match:1"), ListEventsOptions::default())
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            engine
                .list_events(Some("match:2"), ListEventsOptions::default())
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn persistent_lists_streams() {
        let (mut engine, _dir) = setup();
        assert!(engine.list_streams().unwrap().is_empty());
        engine
            .append_event(MemoryEvent::new("s1", "t", "e1"))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("s2", "t", "e2"))
            .unwrap();
        let mut streams = engine.list_streams().unwrap();
        streams.sort();
        assert_eq!(streams, vec!["s1", "s2"]);
    }

    // ── QueueStore ────────────────────────────────────────────────────────

    #[test]
    fn persistent_claims_and_acks_queue_jobs() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("embed", "job-1", "doc-1", 3))
            .unwrap();
        let claimed = engine.claim_job("embed").unwrap().unwrap();
        let acked = engine.ack_job("embed", "job-1").unwrap().unwrap();
        assert_eq!(claimed.status, QueueJobStatus::Leased);
        assert_eq!(claimed.attempts, 1);
        assert_eq!(acked.status, QueueJobStatus::Completed);
    }

    #[test]
    fn persistent_nacks_to_dead_letter_after_max_attempts() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("embed", "job-1", "doc-1", 1))
            .unwrap();
        engine.claim_job("embed").unwrap().unwrap();
        let nacked = engine.nack_job("embed", "job-1").unwrap().unwrap();
        assert_eq!(nacked.status, QueueJobStatus::Dead);
        assert_eq!(engine.list_dead_jobs("embed").unwrap().len(), 1);
    }

    #[test]
    fn persistent_does_not_claim_delayed_jobs() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("embed", "job-1", "doc-1", 3).delay_by_ms(60_000))
            .unwrap();
        assert!(engine.claim_job("embed").unwrap().is_none());
    }

    #[test]
    fn persistent_nacks_with_retry_delay() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("embed", "job-1", "doc-1", 3))
            .unwrap();
        engine.claim_job("embed").unwrap().unwrap();
        let retried = engine
            .nack_job_with_options("embed", "job-1", QueueNackOptions::new(60_000))
            .unwrap()
            .unwrap();
        assert_eq!(retried.status, QueueJobStatus::Ready);
        assert!(engine.claim_job("embed").unwrap().is_none());
    }

    #[test]
    fn persistent_queue_counts() {
        let (mut engine, _dir) = setup();
        assert_eq!(engine.count_active_jobs().unwrap(), 0);
        assert_eq!(engine.count_dead_jobs().unwrap(), 0);
        engine
            .push_job(QueueJob::new("work", "j1", "p1", 3))
            .unwrap();
        engine
            .push_job(QueueJob::new("work", "j2", "p2", 3))
            .unwrap();
        engine
            .push_job(QueueJob::new("other", "j3", "p3", 1))
            .unwrap();
        assert_eq!(engine.count_active_jobs().unwrap(), 3);
        engine.claim_job("other").unwrap();
        engine.nack_job("other", "j3").unwrap();
        assert_eq!(engine.count_dead_jobs().unwrap(), 1);
        assert_eq!(engine.count_active_jobs().unwrap(), 2);
    }

    #[test]
    fn persistent_lists_queues() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("work", "j1", "p1", 3))
            .unwrap();
        engine
            .push_job(QueueJob::new("jobs", "j2", "p2", 3))
            .unwrap();
        let mut queues = engine.list_queues().unwrap();
        queues.sort();
        assert_eq!(queues, vec!["jobs", "work"]);
    }

    #[test]
    fn persistent_claim_reclaims_expired_lease() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("embed", "job-1", "doc-1", 3))
            .unwrap();
        let first = engine
            .claim_job_with_options("embed", QueueClaimOptions::new(0))
            .unwrap()
            .unwrap();
        let second = engine.claim_job("embed").unwrap().unwrap();
        assert_eq!(first.status, QueueJobStatus::Leased);
        assert_eq!(second.status, QueueJobStatus::Leased);
        assert_eq!(second.attempts, 2);
    }

    #[test]
    fn persistent_priority_ordering() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("q", "low", "body", 3).with_priority(0))
            .unwrap();
        engine
            .push_job(QueueJob::new("q", "high", "body", 3).with_priority(10))
            .unwrap();
        engine
            .push_job(QueueJob::new("q", "mid", "body", 3).with_priority(5))
            .unwrap();
        let first = engine.claim_job("q").unwrap().unwrap();
        assert_eq!(first.id, "high", "highest priority claimed first");
        let second = engine.claim_job("q").unwrap().unwrap();
        assert_eq!(second.id, "mid", "medium priority claimed second");
        let third = engine.claim_job("q").unwrap().unwrap();
        assert_eq!(third.id, "low", "lowest priority claimed last");
    }

    // ── LinkStore ─────────────────────────────────────────────────────────

    #[test]
    fn persistent_create_get_delete_link() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("n", "a", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("n", "b", "{}"))
            .unwrap();
        let link = engine
            .create_link(Link::new("n/a", "connects", "n/b"))
            .unwrap();
        assert!(!link.id.is_empty());
        let fetched = engine.get_link(&link.id).unwrap().unwrap();
        assert_eq!(fetched.id, link.id);
        assert!(engine.delete_link(&link.id).unwrap());
        assert!(engine.get_link(&link.id).unwrap().is_none());
    }

    #[test]
    fn persistent_neighbor_query() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("n", "a", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("n", "b", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("n", "c", "{}"))
            .unwrap();
        engine
            .create_link(Link::new("n/a", "knows", "n/b"))
            .unwrap();
        engine
            .create_link(Link::new("n/a", "knows", "n/c"))
            .unwrap();
        let outgoing = engine
            .get_neighbors("n/a", LinkDirection::Outgoing, LinkQueryOptions::default())
            .unwrap();
        assert_eq!(outgoing.len(), 2);
        let incoming = engine
            .get_neighbors("n/b", LinkDirection::Incoming, LinkQueryOptions::default())
            .unwrap();
        assert_eq!(incoming.len(), 1);
    }

    #[test]
    fn persistent_link_count() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("n", "a", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("n", "b", "{}"))
            .unwrap();
        assert_eq!(engine.count_links().unwrap(), 0);
        engine
            .create_link(Link::new("n/a", "knows", "n/b"))
            .unwrap();
        assert_eq!(engine.count_links().unwrap(), 1);
    }

    // ── Searcher (naive — Tantivy is feature-gated) ──────────────────────

    #[test]
    fn persistent_search_objects_and_events() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("docs", "a", r#"{"text":"hello world"}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new(
                "docs",
                "b",
                r#"{"text":"goodbye world"}"#,
            ))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("audit", "test", "hello event"))
            .unwrap();
        let results = engine.search("hello", SearchOptions::default()).unwrap();
        assert_eq!(results.len(), 2);
        let kinds: Vec<&str> = results.iter().map(|h| h.kind.as_str()).collect();
        assert!(kinds.contains(&"object"));
        assert!(kinds.contains(&"event"));
    }

    #[test]
    fn persistent_search_with_collections() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("docs", "a", r#"{"text":"hello world"}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("notes", "b", r#"{"text":"hello there"}"#))
            .unwrap();
        let opts = SearchOptions {
            collections: Some(vec!["docs".into()]),
            ..Default::default()
        };
        let results = engine.search("hello", opts).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].collection, "docs");
    }

    // ── Tantivy search (feature-gated) ──────────────────────────────────

    #[cfg(feature = "search")]
    #[test]
    fn persistent_search_indexes_on_put() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new(
                "docs",
                "a",
                r#"{"text":"unique_search_term_xyz"}"#,
            ))
            .unwrap();
        let results = engine
            .search("unique_search_term_xyz", SearchOptions::default())
            .unwrap();
        assert_eq!(
            results.len(),
            1,
            "search must find indexed content immediately after put"
        );
        assert_eq!(results[0].id, "a");
    }

    #[cfg(feature = "search")]
    #[test]
    fn persistent_search_removes_on_delete() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new(
                "docs",
                "to-delete",
                r#"{"text":"deletable_content"}"#,
            ))
            .unwrap();
        // Should be findable after put
        assert_eq!(
            engine
                .search("deletable_content", SearchOptions::default())
                .unwrap()
                .len(),
            1
        );
        // Delete and verify it's gone from search
        engine.delete_object("docs", "to-delete").unwrap();
        let after = engine
            .search("deletable_content", SearchOptions::default())
            .unwrap();
        assert_eq!(
            after.len(),
            0,
            "deleted object must not appear in search results"
        );
    }

    #[cfg(feature = "search")]
    #[test]
    fn persistent_search_deleted_batch_removes_from_index() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new(
                "docs",
                "a",
                r#"{"text":"batch_deleted_a"}"#,
            ))
            .unwrap();
        engine
            .put_object(MemoryObject::new(
                "docs",
                "b",
                r#"{"text":"batch_deleted_b"}"#,
            ))
            .unwrap();
        assert_eq!(
            engine
                .search("batch_deleted", SearchOptions::default())
                .unwrap()
                .len(),
            2
        );
        let keys = vec![
            ("docs".to_string(), "a".to_string()),
            ("docs".to_string(), "b".to_string()),
        ];
        engine.delete_objects_batch(&keys).unwrap();
        let after = engine
            .search("batch_deleted", SearchOptions::default())
            .unwrap();
        assert_eq!(
            after.len(),
            0,
            "batch-deleted objects must be removed from search index"
        );
    }

    // ── AggregateStore ────────────────────────────────────────────────────

    #[test]
    fn persistent_aggregate_count_sum_avg() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("stats", "a", r#"{"val":10}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("stats", "b", r#"{"val":20}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("stats", "c", r#"{"val":30}"#))
            .unwrap();
        let count = engine
            .aggregate(
                "stats",
                &AggregateOptions {
                    function: AggregateFunction::Count,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(count.total, 3.0);
        let sum = engine
            .aggregate(
                "stats",
                &AggregateOptions {
                    function: AggregateFunction::Sum,
                    field: Some("val".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(sum.total, 60.0);
        let avg = engine
            .aggregate(
                "stats",
                &AggregateOptions {
                    function: AggregateFunction::Avg,
                    field: Some("val".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(avg.total, 20.0);
    }

    #[test]
    fn persistent_aggregate_group_by() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new(
                "sales",
                "a",
                r#"{"region":"EU","val":100}"#,
            ))
            .unwrap();
        engine
            .put_object(MemoryObject::new(
                "sales",
                "b",
                r#"{"region":"US","val":200}"#,
            ))
            .unwrap();
        engine
            .put_object(MemoryObject::new(
                "sales",
                "c",
                r#"{"region":"EU","val":50}"#,
            ))
            .unwrap();
        let result = engine
            .aggregate(
                "sales",
                &AggregateOptions {
                    function: AggregateFunction::Sum,
                    field: Some("val".into()),
                    group_by: Some("region".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(result.total, 350.0);
        assert_eq!(result.groups.len(), 2);
        for group in &result.groups {
            match group.key.as_str() {
                "EU" => assert_eq!(group.value, 150.0),
                "US" => assert_eq!(group.value, 200.0),
                _ => panic!("unexpected group key"),
            }
        }
    }

    #[test]
    fn persistent_timeseries_bucketing() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("events", "a", r#"{"val":1}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("events", "b", r#"{"val":2}"#))
            .unwrap();
        let result = engine
            .timeseries(
                "events",
                &TimeSeriesOptions {
                    function: AggregateFunction::Count,
                    bucket: TimeBucket::Day,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(result.buckets.len(), 1);
        assert_eq!(result.buckets[0].value, 2.0);
    }

    // ── ready_jobs index behavior (Persistent-specific) ───────────────────────

    #[test]
    fn persistent_ready_jobs_indexes_on_push() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("q", "j1", "body", 3))
            .unwrap();
        let prefix = b"q\0";
        let count = engine.ready_jobs.prefix(prefix).count();
        assert_eq!(count, 1, "ready_jobs must have one entry after push");
    }

    #[test]
    fn persistent_ready_jobs_removed_on_claim() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("q", "j1", "body", 3))
            .unwrap();
        engine.claim_job("q").unwrap();
        let prefix = b"q\0";
        let count = engine.ready_jobs.prefix(prefix).count();
        assert_eq!(
            count, 0,
            "ready_jobs must be empty after claiming the only job"
        );
    }

    #[test]
    fn persistent_ready_jobs_priority_order() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("q", "low", "body", 3).with_priority(0))
            .unwrap();
        engine
            .push_job(QueueJob::new("q", "high", "body", 3).with_priority(10))
            .unwrap();
        // ready_jobs should iterate in priority order (highest first)
        let prefix = b"q\0";
        let keys: Vec<Vec<u8>> = engine
            .ready_jobs
            .prefix(prefix)
            .map(|kv| {
                let (k, _) = guard_data(kv).unwrap();
                k
            })
            .collect();
        assert_eq!(keys.len(), 2);
        // First key should contain "high" — it has higher priority
        let first_key_str = String::from_utf8_lossy(&keys[0]);
        assert!(
            first_key_str.contains("high"),
            "first ready entry should be high-priority job; got {first_key_str}"
        );
    }

    #[test]
    fn persistent_ready_jobs_fifo_order() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("q", "first", "body", 3))
            .unwrap();
        // Slight delay so created_at differs
        std::thread::sleep(std::time::Duration::from_millis(5));
        engine
            .push_job(QueueJob::new("q", "second", "body", 3))
            .unwrap();
        let prefix = b"q\0";
        let keys: Vec<Vec<u8>> = engine
            .ready_jobs
            .prefix(prefix)
            .map(|kv| {
                let (k, _) = guard_data(kv).unwrap();
                k
            })
            .collect();
        assert_eq!(keys.len(), 2);
        let first_key_str = String::from_utf8_lossy(&keys[0]);
        assert!(
            first_key_str.contains("first"),
            "first ready entry should be FIFO; got {first_key_str}"
        );
    }

    #[test]
    fn persistent_ready_jobs_reindex_on_nack() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("q", "j1", "body", 3))
            .unwrap();
        engine.claim_job("q").unwrap();
        let prefix = b"q\0";
        assert_eq!(engine.ready_jobs.prefix(prefix).count(), 0);
        // Nack with no delay — should re-index into ready_jobs
        engine
            .nack_job_with_options("q", "j1", QueueNackOptions::new(0))
            .unwrap();
        assert_eq!(
            engine.ready_jobs.prefix(prefix).count(),
            1,
            "ready_jobs must have entry after nack with retry"
        );
    }

    #[test]
    fn persistent_ready_jobs_reindex_on_lease_expire() {
        let (mut engine, _dir) = setup();
        engine
            .push_job(QueueJob::new("q", "j1", "body", 3))
            .unwrap();
        // Claim with zero lease so it immediately expires
        engine
            .claim_job_with_options("q", QueueClaimOptions::new(0))
            .unwrap();
        // The claim method should have reaped the expired lease and re-indexed
        let prefix = b"q\0";
        let _count = engine.ready_jobs.prefix(prefix).count();
        // claim_job called next will reap expired lease and return the job
        let claimed = engine.claim_job("q").unwrap();
        assert!(
            claimed.is_some(),
            "job should be claimable after lease expires"
        );
        let job = claimed.unwrap();
        assert_eq!(job.attempts, 2, "second attempt after lease expiry");
        assert_eq!(
            engine.ready_jobs.prefix(prefix).count(),
            0,
            "ready_jobs must be empty after re-claiming"
        );
    }

    // ── VectorStore ───────────────────────────────────────────────────────

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_search_returns_by_cosine_similarity() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(
                MemoryObject::new("docs", "a", r#"{"text":"alpha"}"#)
                    .with_vector(vec![1.0, 0.0, 0.0]),
            )
            .unwrap();
        engine
            .put_object(
                MemoryObject::new("docs", "b", r#"{"text":"beta"}"#)
                    .with_vector(vec![0.0, 1.0, 0.0]),
            )
            .unwrap();

        let results = engine
            .vector_search("docs", &[0.9, 0.1, 0.0], VectorSearchOptions::default())
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "a");
        assert!(results[0].score > results[1].score);
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_search_respects_filter() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(
                MemoryObject::new("docs", "a", r#"{"tag":"x"}"#).with_vector(vec![1.0, 0.0]),
            )
            .unwrap();
        engine
            .put_object(
                MemoryObject::new("docs", "b", r#"{"tag":"y"}"#).with_vector(vec![0.0, 1.0]),
            )
            .unwrap();

        let results = engine
            .vector_search(
                "docs",
                &[1.0, 0.0],
                VectorSearchOptions {
                    filter: Some(serde_json::json!({"tag": "x"})),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "a");
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_search_excludes_deleted_objects() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("docs", "a", "{}").with_vector(vec![1.0, 0.0]))
            .unwrap();
        engine.delete_object("docs", "a").unwrap();
        let results = engine
            .vector_search("docs", &[1.0, 0.0], VectorSearchOptions::default())
            .unwrap();
        assert!(results.is_empty());
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_search_respects_top_k() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(
                MemoryObject::new("docs", "a", r#"{"text":"alpha"}"#).with_vector(vec![1.0, 0.0]),
            )
            .unwrap();
        engine
            .put_object(
                MemoryObject::new("docs", "b", r#"{"text":"beta"}"#).with_vector(vec![0.0, 1.0]),
            )
            .unwrap();

        let results = engine
            .vector_search(
                "docs",
                &[1.0, 0.0],
                VectorSearchOptions {
                    top_k: Some(1),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "a");
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_search_empty_collection_returns_empty() {
        let (engine, _dir) = setup();
        let results = engine
            .vector_search("docs", &[1.0, 0.0, 0.0], VectorSearchOptions::default())
            .unwrap();
        assert!(results.is_empty());
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_search_rejects_dimension_mismatch() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("docs", "a", "{}").with_vector(vec![1.0, 0.0]))
            .unwrap();

        let error = engine
            .vector_search("docs", &[1.0, 0.0, 0.0], VectorSearchOptions::default())
            .unwrap_err();
        assert!(
            matches!(error, ThingdError::InvalidInput(message) if message.contains("dimension"))
        );
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_search_rejects_empty_query() {
        let (engine, _dir) = setup();
        let error = engine
            .vector_search("docs", &[], VectorSearchOptions::default())
            .unwrap_err();
        assert!(matches!(error, ThingdError::InvalidInput(message) if message.contains("empty")));
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_put_object_without_vector_does_not_store_vector() {
        let (mut engine, _dir) = setup();
        engine
            .put_object(MemoryObject::new("docs", "a", "{}"))
            .unwrap();
        let results = engine
            .vector_search("docs", &[1.0, 0.0, 0.0], VectorSearchOptions::default())
            .unwrap();
        assert!(results.is_empty());
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_search_persists_across_engine_reopen() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            engine
                .put_object(
                    MemoryObject::new("docs", "a", r#"{"text":"persist"}"#)
                        .with_vector(vec![1.0, 0.0, 0.0]),
                )
                .unwrap();
        }
        {
            let engine = PersistentEngine::open(dir.path()).unwrap();
            let results = engine
                .vector_search("docs", &[1.0, 0.0, 0.0], VectorSearchOptions::default())
                .unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].id, "a");
        }
    }

    // ── Reopen tests ─────────────────────────────────────────────────────────

    #[test]
    fn persistent_event_sequence_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let stream = "test-stream";
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            let e1 = engine
                .append_event(MemoryEvent::new(stream, "t1", "{}"))
                .unwrap();
            assert_eq!(e1.sequence, 1);
            let e2 = engine
                .append_event(MemoryEvent::new(stream, "t2", "{}"))
                .unwrap();
            assert_eq!(e2.sequence, 2);
        }
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            // Next event should continue at sequence 3
            let e3 = engine
                .append_event(MemoryEvent::new(stream, "t3", "{}"))
                .unwrap();
            assert_eq!(
                e3.sequence, 3,
                "sequence must continue from durable max after reopen"
            );
            // Sequence 1 should not be overwritten
            let events = engine
                .list_events(Some(stream), ListEventsOptions::default())
                .unwrap();
            assert_eq!(events.len(), 3);
        }
    }

    #[test]
    fn persistent_event_idempotency_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let stream = "test-stream";
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            let mut e = MemoryEvent::new(stream, "t1", r#"{"x":1}"#);
            e.idempotency_key = "key-1".to_string();
            let e1 = engine.append_event(e).unwrap();
            assert_eq!(e1.sequence, 1);
        }
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            // Same idempotency key — must return existing event, not duplicate
            let mut e = MemoryEvent::new(stream, "t1", r#"{"x":1}"#);
            e.idempotency_key = "key-1".to_string();
            let e2 = engine.append_event(e).unwrap();
            assert_eq!(
                e2.sequence, 1,
                "idempotency must be preserved across reopen"
            );
            // New event should continue
            let e3 = engine
                .append_event(MemoryEvent::new(stream, "t2", "{}"))
                .unwrap();
            assert_eq!(e3.sequence, 2);
        }
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            engine
                .put_object(
                    MemoryObject::new("docs", "a", r#"{"text":"persist"}"#)
                        .with_vector(vec![1.0, 0.0, 0.0]),
                )
                .unwrap();
        }
        {
            let engine = PersistentEngine::open(dir.path()).unwrap();
            let results = engine
                .vector_search("docs", &[1.0, 0.0, 0.0], VectorSearchOptions::default())
                .unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].id, "a");
        }
    }

    #[cfg(feature = "vectors")]
    #[test]
    fn persistent_vector_removed_on_update_without_vector_reopen() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            engine
                .put_object(
                    MemoryObject::new("docs", "a", r#"{"v":1}"#).with_vector(vec![1.0, 0.0, 0.0]),
                )
                .unwrap();
        }
        {
            let mut engine = PersistentEngine::open(dir.path()).unwrap();
            // Update without vector — old vector must be removed
            engine
                .put_object(MemoryObject::new("docs", "a", r#"{"v":2}"#))
                .unwrap();
        }
        {
            let engine = PersistentEngine::open(dir.path()).unwrap();
            let results = engine
                .vector_search("docs", &[1.0, 0.0, 0.0], VectorSearchOptions::default())
                .unwrap();
            assert_eq!(results.len(), 0, "vector must survive reopen and removal");
        }
    }

    // ── Shared contract tests ───────────────────────────────────────────────

    fn setup_persistent() -> (PersistentEngine, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let engine = PersistentEngine::open(dir.path()).unwrap();
        (engine, dir)
    }

    #[test]
    fn contract_object_lifecycle() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_object_lifecycle(&mut engine);
    }

    #[test]
    fn contract_vector_lifecycle() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_vector_lifecycle(&mut engine);
    }

    #[test]
    fn contract_event_idempotency() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_event_idempotency(&mut engine);
    }

    #[test]
    fn contract_queue_lifecycle() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_queue_lifecycle(&mut engine);
    }

    #[test]
    fn contract_delayed_job() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_delayed_job(&mut engine);
    }

    #[test]
    fn contract_lease_expiration() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_lease_expiration(&mut engine);
    }

    #[test]
    fn contract_nack_dead_letter() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_nack_dead_letter(&mut engine);
    }

    #[test]
    fn contract_search() {
        let (mut engine, _dir) = setup_persistent();
        crate::contract_tests::test_contract_search(&mut engine);
    }
}
