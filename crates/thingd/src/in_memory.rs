//! In-memory storage adapter used for API design and tests.

use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::model::{ListEventsOptions, ListObjectsOptions};
use crate::{
    AggregateFunction, AggregateGroupResult, AggregateOptions, AggregateResult, CollectionSchema,
    EventLog, FieldSchema, Link, LinkDirection, LinkQueryOptions, LinkStore, MemoryEvent,
    MemoryObject, ObjectKey, ObjectStore, QueueClaimOptions, QueueJob, QueueJobStatus,
    QueueNackOptions, QueueStore, SchemaOptions, ThingdError, ThingdResult, TimeSeriesBucket,
    TimeSeriesOptions, TimeSeriesResult, now_iso_string, u64_to_i64, unix_timestamp_millis,
};

/// In-memory engine used to prove the storage boundary.
///
/// # Examples
///
/// ```rust
/// use thingd::{MemoryEngine, ObjectStore, EventLog, MemoryObject, MemoryEvent};
///
/// let mut engine = MemoryEngine::new();
///
/// engine.put_object(MemoryObject::new("users", "alice", r#"{"name":"Alice"}"#)).unwrap();
/// engine.append_event(MemoryEvent::new("audit", "login", r#"{"user":"alice"}"#)).unwrap();
///
/// assert_eq!(engine.count_objects().unwrap(), 1);
/// assert_eq!(engine.count_events().unwrap(), 1);
/// ```
#[derive(Default)]
pub struct MemoryEngine {
    objects: BTreeMap<ObjectKey, MemoryObject>,
    events: Vec<MemoryEvent>,
    queues: BTreeMap<String, VecDeque<QueueJob>>,
    links: Vec<Link>,
    next_event_sequence: u64,
    next_link_id: u64,
    event_idempotency_keys: HashMap<(String, String), u64>,
}

impl MemoryEngine {
    /// Create a new empty in-memory engine.
    pub fn new() -> Self {
        Self::default()
    }
}

impl ObjectStore for MemoryEngine {
    fn put_object(&mut self, mut object: MemoryObject) -> ThingdResult<MemoryObject> {
        let now = now_iso_string();
        let version = self
            .objects
            .get(&object.key)
            .map_or(1, |existing| existing.version + 1);

        object.version = version;
        object.updated_at.clone_from(&now);
        if object.created_at.is_empty() {
            object.created_at = now;
        }
        self.objects.insert(object.key.clone(), object.clone());

        Ok(object)
    }

    fn put_object_with_options(
        &mut self,
        object: MemoryObject,
        options: crate::PutObjectOptions,
    ) -> ThingdResult<MemoryObject> {
        if let Some(expected) = options.expected_version {
            let current = self.objects.get(&object.key).map(|o| o.version);
            if current != Some(expected) {
                return Err(ThingdError::Conflict(format!(
                    "Version mismatch for {}/{}: expected {expected}, got {:?}",
                    object.key.collection, object.key.id, current,
                )));
            }
        }
        // Delegate to put_object for version increment, FTS handling, etc.
        self.put_object(object)
    }

    fn get_object(&self, collection: &str, id: &str) -> ThingdResult<Option<MemoryObject>> {
        Ok(self.objects.get(&ObjectKey::new(collection, id)).cloned())
    }

    fn list_objects(
        &self,
        collections: Option<&[String]>,
        options: &ListObjectsOptions,
    ) -> ThingdResult<Vec<MemoryObject>> {
        let mut objects: Vec<MemoryObject> = self
            .objects
            .values()
            .filter(|object| {
                collections.is_none_or(|allowed| allowed.contains(&object.key.collection))
            })
            .filter(|object| {
                if options.filter.is_empty() {
                    return true;
                }
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
            })
            .cloned()
            .collect();

        // Apply sort if requested
        if let Some(ref sort_by) = options.sort_by {
            use crate::model::SortDirection;
            let asc = sort_by.direction == SortDirection::Asc;
            objects.sort_by(|a, b| {
                let cmp = if sort_by.field.starts_with("$.") {
                    // Sort by JSON body field
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
        Ok(self
            .objects
            .remove(&ObjectKey::new(collection, id))
            .is_some())
    }

    fn count_objects(&self) -> ThingdResult<u64> {
        Ok(self.objects.len() as u64)
    }

    fn count_objects_in_collection(&self, collection: &str) -> ThingdResult<u64> {
        let count = self
            .objects
            .keys()
            .filter(|key| key.collection == collection)
            .count();
        Ok(count as u64)
    }

    fn list_collections(&self) -> ThingdResult<Vec<String>> {
        let mut collections: Vec<String> = self
            .objects
            .keys()
            .map(|key| key.collection.clone())
            .collect();
        collections.sort();
        collections.dedup();
        Ok(collections)
    }

    fn schema(
        &self,
        collection: Option<&str>,
        options: &SchemaOptions,
    ) -> ThingdResult<Vec<CollectionSchema>> {
        #[allow(clippy::option_if_let_else)]
        let collections = if let Some(name) = collection {
            vec![name.to_string()]
        } else {
            let mut cols: Vec<String> = self.objects.keys().map(|k| k.collection.clone()).collect();
            cols.sort();
            cols.dedup();
            cols
        };

        let mut schemas = Vec::new();
        for col_name in collections {
            let objects: Vec<&MemoryObject> = self
                .objects
                .values()
                .filter(|o| o.key.collection == col_name)
                .collect();

            let object_count = objects.len() as u64;
            if object_count == 0 {
                continue;
            }

            let sample_size = options.sample_size.unwrap_or(50);
            let sampled: Vec<&MemoryObject> = objects.iter().take(sample_size).copied().collect();
            let fields = infer_fields(&sampled);

            schemas.push(CollectionSchema {
                name: col_name,
                object_count,
                fields,
            });
        }

        Ok(schemas)
    }
}

impl EventLog for MemoryEngine {
    fn is_protected_stream(&self, stream: &str) -> bool {
        stream == "__thingd:mcp:audit"
    }
    fn append_event(&mut self, mut event: MemoryEvent) -> ThingdResult<MemoryEvent> {
        // Idempotency check: if idempotency_key is set and known, return existing event
        if !event.idempotency_key.is_empty()
            && let Some(&existing_seq) = self
                .event_idempotency_keys
                .get(&(event.stream.clone(), event.idempotency_key.clone()))
        {
            // Find and return the existing event with this sequence
            if let Some(existing) = self.events.iter().find(|e| e.sequence == existing_seq) {
                return Ok(existing.clone());
            }
        }

        self.next_event_sequence += 1;
        event.sequence = self.next_event_sequence;
        if event.created_at.is_empty() {
            event.created_at = now_iso_string();
        }

        // Track idempotency key
        if !event.idempotency_key.is_empty() {
            self.event_idempotency_keys.insert(
                (event.stream.clone(), event.idempotency_key.clone()),
                event.sequence,
            );
        }

        self.events.push(event.clone());
        Ok(event)
    }

    fn list_events(
        &self,
        stream: Option<&str>,
        options: ListEventsOptions,
    ) -> ThingdResult<Vec<MemoryEvent>> {
        let events = self
            .events
            .iter()
            .filter(|event| stream.is_none_or(|target| event.stream == target))
            .filter(|event| options.from_sequence.is_none_or(|seq| event.sequence > seq))
            .filter(|event| {
                options
                    .since
                    .as_ref()
                    .is_none_or(|since| event.created_at.as_str() >= since.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();

        Ok(match options.limit {
            Some(limit) => events
                .into_iter()
                .take(usize::try_from(limit).unwrap_or(usize::MAX))
                .collect(),
            None => events,
        })
    }

    fn count_events(&self) -> ThingdResult<u64> {
        Ok(self.events.len() as u64)
    }

    fn list_streams(&self) -> ThingdResult<Vec<String>> {
        let mut streams: Vec<String> = self
            .events
            .iter()
            .map(|event| event.stream.clone())
            .collect();
        streams.sort();
        streams.dedup();
        Ok(streams)
    }

    fn delete_last_event(&mut self, stream: &str) -> ThingdResult<Option<MemoryEvent>> {
        if self.is_protected_stream(stream) {
            return Err(ThingdError::Protected(format!(
                "stream '{stream}' is protected and cannot be modified"
            )));
        }
        let pos = self.events.iter().rposition(|e| e.stream == stream);
        match pos {
            Some(idx) => Ok(Some(self.events.remove(idx))),
            None => Ok(None),
        }
    }

    fn delete_stream(&mut self, stream: &str) -> ThingdResult<u64> {
        if self.is_protected_stream(stream) {
            return Err(ThingdError::Protected(format!(
                "stream '{stream}' is protected and cannot be modified"
            )));
        }
        let before = self.events.len();
        self.events.retain(|e| e.stream != stream);
        Ok((before - self.events.len()) as u64)
    }
}

impl QueueStore for MemoryEngine {
    fn push_job(&mut self, job: QueueJob) -> ThingdResult<QueueJob> {
        let jobs = self.queues.entry(job.queue.clone()).or_default();

        if let Some(existing) = jobs.iter().find(|candidate| candidate.id == job.id) {
            return Ok(existing.clone());
        }

        jobs.push_back(job.clone());
        Ok(job)
    }

    fn claim_job_with_options(
        &mut self,
        queue: &str,
        options: QueueClaimOptions,
    ) -> ThingdResult<Option<QueueJob>> {
        self.release_expired_leases(queue);

        let Some(jobs) = self.queues.get_mut(queue) else {
            return Ok(None);
        };

        let now = unix_timestamp_millis();
        let Some(job) = jobs.iter_mut().find(|candidate| {
            candidate.status == QueueJobStatus::Ready && candidate.available_at_ms <= now
        }) else {
            return Ok(None);
        };

        job.status = QueueJobStatus::Leased;
        job.attempts += 1;
        job.leased_at_ms = Some(now);
        job.lease_expires_at_ms = Some(now.saturating_add(u64_to_i64(options.lease_ms)));

        Ok(Some(job.clone()))
    }

    fn ack_job(&mut self, queue: &str, id: &str) -> ThingdResult<Option<QueueJob>> {
        let Some(job) = self.find_job_mut(queue, id) else {
            return Ok(None);
        };

        if job.status != QueueJobStatus::Leased {
            return Err(ThingdError::Conflict(format!(
                "job {id} must be leased before ack"
            )));
        }

        job.status = QueueJobStatus::Completed;
        job.completed_at_ms = Some(unix_timestamp_millis());

        Ok(Some(job.clone()))
    }

    fn nack_job_with_options(
        &mut self,
        queue: &str,
        id: &str,
        options: QueueNackOptions,
    ) -> ThingdResult<Option<QueueJob>> {
        let Some(job) = self.find_job_mut(queue, id) else {
            return Ok(None);
        };

        if job.status != QueueJobStatus::Leased {
            return Err(ThingdError::Conflict(format!(
                "job {id} must be leased before nack"
            )));
        }

        let now = unix_timestamp_millis();
        job.leased_at_ms = None;
        job.lease_expires_at_ms = None;
        if !options.error.is_empty() {
            job.last_error = options.error;
        }
        job.status = if job.attempts >= job.max_attempts {
            job.dead_at_ms = Some(now);
            QueueJobStatus::Dead
        } else {
            job.available_at_ms = now.saturating_add(u64_to_i64(options.delay_ms));
            QueueJobStatus::Ready
        };

        Ok(Some(job.clone()))
    }

    fn list_jobs(&self, queue: &str) -> ThingdResult<Vec<QueueJob>> {
        Ok(self
            .queues
            .get(queue)
            .map_or_else(Vec::new, |jobs| jobs.iter().cloned().collect()))
    }

    fn list_dead_jobs(&self, queue: &str) -> ThingdResult<Vec<QueueJob>> {
        Ok(self.queues.get(queue).map_or_else(Vec::new, |jobs| {
            jobs.iter()
                .filter(|job| job.status == QueueJobStatus::Dead)
                .cloned()
                .collect()
        }))
    }

    fn list_queues(&self) -> ThingdResult<Vec<String>> {
        let mut queues: Vec<String> = self.queues.keys().cloned().collect();
        queues.sort();
        Ok(queues)
    }

    fn count_active_jobs(&self) -> ThingdResult<u64> {
        let count = self
            .queues
            .values()
            .flat_map(|jobs| jobs.iter())
            .filter(|job| job.status != QueueJobStatus::Dead)
            .count();
        Ok(count as u64)
    }

    fn count_dead_jobs(&self) -> ThingdResult<u64> {
        let count = self
            .queues
            .values()
            .flat_map(|jobs| jobs.iter())
            .filter(|job| job.status == QueueJobStatus::Dead)
            .count();
        Ok(count as u64)
    }
}

impl MemoryEngine {
    fn find_job_mut(&mut self, queue: &str, id: &str) -> Option<&mut QueueJob> {
        self.queues
            .get_mut(queue)?
            .iter_mut()
            .find(|job| job.id == id)
    }

    fn release_expired_leases(&mut self, queue: &str) {
        let now = unix_timestamp_millis();

        for job in self.queues.get_mut(queue).into_iter().flatten() {
            if job.status == QueueJobStatus::Leased
                && job
                    .lease_expires_at_ms
                    .is_some_and(|lease_expires_at_ms| lease_expires_at_ms <= now)
            {
                job.status = QueueJobStatus::Ready;
                job.leased_at_ms = None;
                job.lease_expires_at_ms = None;
            }
        }
    }
}

impl crate::store::Searcher for MemoryEngine {
    fn search(
        &self,
        query: &str,
        options: crate::SearchOptions,
    ) -> ThingdResult<Vec<crate::SearchHit>> {
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

        // 1. Search objects
        for object in self.objects.values() {
            // Apply collection filter
            if let Some(ref collections) = options.collections
                && !collections.contains(&object.key.collection)
            {
                continue;
            }

            // Apply metadata filter
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
                hits.push(crate::SearchHit {
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

        // 2. Search events
        for event in &self.events {
            // Apply collection filter
            if let Some(ref collections) = options.collections
                && !collections.contains(&event.stream)
            {
                continue;
            }

            // Apply metadata filter
            if let Some(ref filter) = options.filter
                && !matches_filter_memory(&event.body, filter)
            {
                continue;
            }

            let text_to_search =
                format!("{} {} {}", event.stream, event.event_type, event.body).to_lowercase();
            let matches_all = query_words.iter().all(|word| text_to_search.contains(word));

            if matches_all {
                hits.push(crate::SearchHit {
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

        // Limit results if requested
        if let Some(limit) = options.limit {
            hits.truncate(limit);
        }

        Ok(hits)
    }
}

impl LinkStore for MemoryEngine {
    fn create_link(&mut self, mut link: Link) -> ThingdResult<Link> {
        self.next_link_id += 1;
        link.id = format!("link-{}", self.next_link_id);
        if link.created_at.is_empty() {
            link.created_at = now_iso_string();
        }
        self.links.push(link.clone());
        Ok(link)
    }

    fn delete_link(&mut self, id: &str) -> ThingdResult<bool> {
        let len_before = self.links.len();
        self.links.retain(|l| l.id != id);
        Ok(self.links.len() < len_before)
    }

    fn get_link(&self, id: &str) -> ThingdResult<Option<Link>> {
        Ok(self.links.iter().find(|l| l.id == id).cloned())
    }

    fn get_neighbors(
        &self,
        reference: &str,
        direction: LinkDirection,
        options: LinkQueryOptions,
    ) -> ThingdResult<Vec<Link>> {
        let neighbors: Vec<Link> = self
            .links
            .iter()
            .filter(|link| {
                let matches_direction = match direction {
                    LinkDirection::Outgoing => link.from_ref == reference,
                    LinkDirection::Incoming => link.to_ref == reference,
                    LinkDirection::Both => link.from_ref == reference || link.to_ref == reference,
                };
                let matches_type = options
                    .link_type
                    .as_deref()
                    .is_none_or(|t| link.link_type == t);
                matches_direction && matches_type
            })
            .cloned()
            .collect();

        Ok(match options.limit {
            Some(limit) => neighbors.into_iter().take(limit).collect(),
            None => neighbors,
        })
    }

    fn count_links(&self) -> ThingdResult<u64> {
        Ok(self.links.len() as u64)
    }
}

impl crate::store::AggregateStore for MemoryEngine {
    fn aggregate(
        &self,
        collection: &str,
        options: &AggregateOptions,
    ) -> ThingdResult<AggregateResult> {
        let objects: Vec<&MemoryObject> =
            self.objects
                .values()
                .filter(|o| o.key.collection == collection)
                .filter(|o| {
                    if options.filter.is_empty() {
                        return true;
                    }
                    let Ok(body) = serde_json::from_str::<serde_json::Value>(&o.body) else {
                        return false;
                    };
                    options.filter.iter().all(|(key, expected)| {
                        body.get(key.as_str()).is_some_and(|v| v == expected)
                    })
                })
                .collect();

        if let Some(group_field) = &options.group_by {
            let mut groups: std::collections::HashMap<String, Vec<&MemoryObject>> =
                std::collections::HashMap::new();
            for obj in &objects {
                let key = extract_field_str(&obj.body, group_field);
                groups.entry(key).or_default().push(obj);
            }

            let mut group_results: Vec<AggregateGroupResult> = groups
                .iter()
                .map(|(key, objs)| AggregateGroupResult {
                    key: key.clone(),
                    value: compute_aggregate(objs, options.function, options.field.as_deref()),
                })
                .collect();
            group_results.sort_by(|a, b| a.key.cmp(&b.key));

            let total: f64 = group_results.iter().map(|g| g.value).sum();
            Ok(AggregateResult {
                total,
                groups: group_results,
            })
        } else {
            let total = compute_aggregate(&objects, options.function, options.field.as_deref());
            Ok(AggregateResult {
                total,
                groups: Vec::new(),
            })
        }
    }

    fn timeseries(
        &self,
        collection: &str,
        options: &TimeSeriesOptions,
    ) -> ThingdResult<TimeSeriesResult> {
        let format = options.bucket.strftime_format();
        let objects: Vec<&MemoryObject> =
            self.objects
                .values()
                .filter(|o| o.key.collection == collection)
                .filter(|o| {
                    if options.filter.is_empty() {
                        return true;
                    }
                    let Ok(body) = serde_json::from_str::<serde_json::Value>(&o.body) else {
                        return false;
                    };
                    options.filter.iter().all(|(key, expected)| {
                        body.get(key.as_str()).is_some_and(|v| v == expected)
                    })
                })
                .filter(|o| {
                    if options.from.is_none() && options.to.is_none() {
                        return true;
                    }
                    let in_range = |ts: &str| -> bool {
                        if let Some(ref from) = options.from
                            && ts < from.as_str()
                        {
                            return false;
                        }
                        if let Some(ref to) = options.to
                            && ts >= to.as_str()
                        {
                            return false;
                        }
                        true
                    };
                    in_range(&o.created_at)
                })
                .collect();

        // Bucket by created_at using chrono
        let mut buckets: std::collections::HashMap<String, Vec<&MemoryObject>> =
            std::collections::HashMap::new();
        for obj in &objects {
            let label = format_timestamp(&obj.created_at, format);
            buckets.entry(label).or_default().push(obj);
        }

        let mut result_buckets: Vec<TimeSeriesBucket> = buckets
            .iter()
            .map(|(label, objs)| TimeSeriesBucket {
                label: label.clone(),
                value: compute_aggregate(objs, options.function, options.field.as_deref()),
            })
            .collect();
        result_buckets.sort_by(|a, b| a.label.cmp(&b.label));

        Ok(TimeSeriesResult {
            buckets: result_buckets,
        })
    }
}

fn matches_filter_memory(body_str: &str, filter: &serde_json::Value) -> bool {
    let Ok(body) = serde_json::from_str::<serde_json::Value>(body_str) else {
        return false;
    };

    let Some(filter_obj) = filter.as_object() else {
        return true;
    };

    for (k, v) in filter_obj {
        if body.get(k) != Some(v) {
            return false;
        }
    }
    true
}

/// Extract a field value as a string from a JSON body.
fn extract_field_str(body_str: &str, field: &str) -> String {
    let Ok(body) = serde_json::from_str::<serde_json::Value>(body_str) else {
        return String::new();
    };
    match body.get(field) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    }
}

/// Extract a field value as f64 from a JSON body.
fn extract_field_f64(body_str: &str, field: &str) -> Option<f64> {
    let body = serde_json::from_str::<serde_json::Value>(body_str).ok()?;
    match body.get(field) {
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(serde_json::Value::String(s)) => s.parse::<f64>().ok(),
        _ => None,
    }
}

/// Compute an aggregate value over a slice of objects.
fn compute_aggregate(
    objects: &[&MemoryObject],
    function: AggregateFunction,
    field: Option<&str>,
) -> f64 {
    match function {
        AggregateFunction::Count => objects.len() as f64,
        AggregateFunction::Sum => {
            let field = field.unwrap_or_default();
            objects
                .iter()
                .filter_map(|o| extract_field_f64(&o.body, field))
                .sum()
        },
        AggregateFunction::Avg => {
            let field = field.unwrap_or_default();
            let values: Vec<f64> = objects
                .iter()
                .filter_map(|o| extract_field_f64(&o.body, field))
                .collect();
            if values.is_empty() {
                0.0
            } else {
                values.iter().sum::<f64>() / values.len() as f64
            }
        },
        AggregateFunction::Min => {
            let field = field.unwrap_or_default();
            objects
                .iter()
                .filter_map(|o| extract_field_f64(&o.body, field))
                .fold(f64::INFINITY, f64::min)
        },
        AggregateFunction::Max => {
            let field = field.unwrap_or_default();
            objects
                .iter()
                .filter_map(|o| extract_field_f64(&o.body, field))
                .fold(f64::NEG_INFINITY, f64::max)
        },
    }
}

/// Format a timestamp string to a bucket label using the given strftime format.
fn format_timestamp(ts: &str, format: &str) -> String {
    // Parse ISO 8601 timestamp and reformat
    let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) else {
        return ts.to_string();
    };
    let utc = dt.with_timezone(&chrono::Utc);
    utc.format(format).to_string()
}

/// Infer field schemas from sampled object bodies.
fn infer_fields(objects: &[&MemoryObject]) -> Vec<FieldSchema> {
    use std::collections::BTreeMap;
    let mut field_map: BTreeMap<String, (String, bool, Vec<serde_json::Value>)> = BTreeMap::new();

    for obj in objects {
        let body: serde_json::Value =
            serde_json::from_str(&obj.body).unwrap_or(serde_json::Value::Null);
        let map = match &body {
            serde_json::Value::Object(m) => m,
            _ => continue,
        };

        for (key, value) in map {
            let entry = field_map
                .entry(key.clone())
                .or_insert_with(|| (infer_json_type(value), false, Vec::new()));

            if value.is_null() {
                entry.1 = true;
            }

            if entry.2.len() < 3 && !value.is_null() {
                entry.2.push(value.clone());
            }

            if entry.0 != infer_json_type(value) && !value.is_null() {
                entry.0 = "unknown".to_string();
            }
        }
    }

    field_map
        .into_iter()
        .map(
            |(name, (field_type, nullable, sample_values))| FieldSchema {
                name,
                field_type,
                nullable,
                sample_values,
            },
        )
        .collect()
}

/// Infer the JSON type string for a value.
fn infer_json_type(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(_) => "boolean".to_string(),
        serde_json::Value::Number(_) => "number".to_string(),
        serde_json::Value::String(s) => {
            if s.len() > 10
                && (s.contains('T') || s.contains('-'))
                && chrono::DateTime::parse_from_rfc3339(s).is_ok()
            {
                "date".to_string()
            } else {
                "string".to_string()
            }
        },
        serde_json::Value::Array(_) => "array".to_string(),
        serde_json::Value::Object(_) => "object".to_string(),
    }
}

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
    // Simple SQL LIKE pattern match: % matches any chars, _ matches single char
    let parts = pattern.split('%');
    let mut pos = 0;
    for part in parts {
        if part.is_empty() {
            continue;
        }
        // Handle _ wildcard within the part (simplified: treat as exact match for non-% parts)
        if let Some(idx) = s[pos..].find(part) {
            pos += idx + part.len();
        } else {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{LinkStore, Searcher};
    use crate::{Link, ListObjectsOptions, SearchOptions};

    #[test]
    fn stores_and_reads_objects() {
        let mut engine = MemoryEngine::new();

        let object = engine
            .put_object(MemoryObject::new(
                "decisions",
                "rust-core",
                "{\"text\":\"Use Rust\"}",
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
    fn lists_objects_with_optional_collection_filter() {
        let mut engine = MemoryEngine::new();

        engine
            .put_object(MemoryObject::new("decisions", "rust-core", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("notes", "agent-guide", "{}"))
            .unwrap();

        let filtered = engine
            .list_objects(
                Some(&["decisions".to_string()]),
                &ListObjectsOptions::default(),
            )
            .unwrap();

        assert_eq!(
            engine
                .list_objects(None, &ListObjectsOptions::default())
                .unwrap()
                .len(),
            2
        );
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].key.collection, "decisions");
    }

    #[test]
    fn appends_events_with_sequence_numbers() {
        let mut engine = MemoryEngine::new();

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
    fn deletes_last_event_from_stream() {
        let mut engine = MemoryEngine::new();

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

        // match:2 unaffected
        let match2 = engine
            .list_events(Some("match:2"), ListEventsOptions::default())
            .unwrap();
        assert_eq!(match2.len(), 1);
    }

    #[test]
    fn returns_none_when_delete_last_event_on_empty_stream() {
        let mut engine = MemoryEngine::new();
        assert!(engine.delete_last_event("nonexistent").unwrap().is_none());
    }

    #[test]
    fn deletes_stream_and_returns_count() {
        let mut engine = MemoryEngine::new();

        engine
            .append_event(MemoryEvent::new("match:1", "turn.recorded", "{}"))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("match:1", "turn.recorded", "{}"))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("match:2", "turn.recorded", "{}"))
            .unwrap();

        let count = engine.delete_stream("match:1").unwrap();
        assert_eq!(count, 2);

        let remaining = engine
            .list_events(Some("match:1"), ListEventsOptions::default())
            .unwrap();
        assert_eq!(remaining.len(), 0);

        // match:2 unaffected
        let match2 = engine
            .list_events(Some("match:2"), ListEventsOptions::default())
            .unwrap();
        assert_eq!(match2.len(), 1);
    }

    #[test]
    fn returns_zero_for_delete_stream_on_empty_stream() {
        let mut engine = MemoryEngine::new();
        assert_eq!(engine.delete_stream("nonexistent").unwrap(), 0);
    }

    #[test]
    fn claims_and_acks_queue_jobs() {
        let mut engine = MemoryEngine::new();

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
    fn nacks_jobs_to_dead_letter_after_max_attempts() {
        let mut engine = MemoryEngine::new();

        engine
            .push_job(QueueJob::new("embed", "job-1", "doc-1", 1))
            .unwrap();

        engine.claim_job("embed").unwrap().unwrap();
        let nacked = engine.nack_job("embed", "job-1").unwrap().unwrap();

        assert_eq!(nacked.status, QueueJobStatus::Dead);
        assert_eq!(engine.list_dead_jobs("embed").unwrap().len(), 1);
    }

    #[test]
    fn does_not_claim_delayed_jobs_before_available() {
        let mut engine = MemoryEngine::new();

        engine
            .push_job(QueueJob::new("embed", "job-1", "doc-1", 3).delay_by_ms(60_000))
            .unwrap();

        assert!(engine.claim_job("embed").unwrap().is_none());
    }

    #[test]
    fn reclaims_jobs_after_lease_expiration() {
        let mut engine = MemoryEngine::new();

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
    fn nacks_jobs_with_retry_delay() {
        let mut engine = MemoryEngine::new();

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
    fn counts_objects_events_and_jobs() {
        let mut engine = MemoryEngine::new();

        assert_eq!(engine.count_objects().unwrap(), 0);
        assert_eq!(engine.count_events().unwrap(), 0);
        assert_eq!(engine.count_active_jobs().unwrap(), 0);
        assert_eq!(engine.count_dead_jobs().unwrap(), 0);

        engine
            .put_object(MemoryObject::new("col-a", "o1", r#"{"v":1}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("col-a", "o2", r#"{"v":2}"#))
            .unwrap();
        engine
            .put_object(MemoryObject::new("col-b", "o3", r#"{"v":3}"#))
            .unwrap();
        assert_eq!(engine.count_objects().unwrap(), 3);

        engine
            .append_event(MemoryEvent::new("s1", "t1", "e1"))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("s1", "t2", "e2"))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("s2", "t3", "e3"))
            .unwrap();
        assert_eq!(engine.count_events().unwrap(), 3);

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
    fn lists_collections_streams_and_queues() {
        let mut engine = MemoryEngine::new();

        assert!(engine.list_collections().unwrap().is_empty());
        assert!(engine.list_streams().unwrap().is_empty());
        assert!(engine.list_queues().unwrap().is_empty());

        engine
            .put_object(MemoryObject::new("col-a", "x", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("col-b", "y", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("col-a", "z", "{}"))
            .unwrap();
        let collections = engine.list_collections().unwrap();
        assert_eq!(collections, vec!["col-a", "col-b"]);

        engine
            .append_event(MemoryEvent::new("s1", "t", "e1"))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("s2", "t", "e2"))
            .unwrap();
        let streams = engine.list_streams().unwrap();
        assert_eq!(streams, vec!["s1", "s2"]);

        engine
            .push_job(QueueJob::new("work", "j1", "p1", 3))
            .unwrap();
        engine
            .push_job(QueueJob::new("jobs", "j2", "p2", 3))
            .unwrap();
        let queues = engine.list_queues().unwrap();
        assert_eq!(queues, vec!["jobs", "work"]);
    }

    #[test]
    fn search_respects_filter_and_limit() {
        let mut engine = MemoryEngine::new();

        engine
            .put_object(MemoryObject::new(
                "docs",
                "a",
                r#"{"text":"hello world","tag":"greeting"}"#,
            ))
            .unwrap();
        engine
            .put_object(MemoryObject::new(
                "docs",
                "b",
                r#"{"text":"hello there","tag":"greeting"}"#,
            ))
            .unwrap();
        engine
            .put_object(MemoryObject::new(
                "docs",
                "c",
                r#"{"text":"goodbye world","tag":"farewell"}"#,
            ))
            .unwrap();

        let all = engine.search("world", SearchOptions::default()).unwrap();
        assert_eq!(all.len(), 2);

        let limited = engine
            .search(
                "world",
                SearchOptions {
                    limit: Some(1),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(limited.len(), 1);

        let filtered = engine
            .search(
                "hello",
                SearchOptions {
                    collections: Some(vec!["docs".into()]),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(filtered.len(), 2);
    }

    // ── list_objects: filter / limit / offset ─────────────────────────────

    #[test]
    fn list_objects_filter_returns_matching_objects() {
        let mut engine = MemoryEngine::new();
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
    fn list_objects_filter_no_match_returns_empty() {
        let mut engine = MemoryEngine::new();
        engine
            .put_object(MemoryObject::new("w", "a", r#"{"color":"red"}"#))
            .unwrap();

        let opts = ListObjectsOptions {
            filter: vec![("color".into(), serde_json::json!("green"))],
            ..Default::default()
        };
        let results = engine
            .list_objects(Some(&["w".to_string()]), &opts)
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn list_objects_limit_truncates_results() {
        let mut engine = MemoryEngine::new();
        for i in 0..5u32 {
            engine
                .put_object(MemoryObject::new("col", format!("id-{i}"), "{}"))
                .unwrap();
        }

        let opts = ListObjectsOptions {
            limit: Some(3),
            ..Default::default()
        };
        let results = engine
            .list_objects(Some(&["col".to_string()]), &opts)
            .unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn list_objects_offset_skips_results() {
        let mut engine = MemoryEngine::new();
        for i in 0..5u32 {
            engine
                .put_object(MemoryObject::new("col", format!("id-{i}"), "{}"))
                .unwrap();
        }

        let opts = ListObjectsOptions {
            offset: Some(3),
            ..Default::default()
        };
        let results = engine
            .list_objects(Some(&["col".to_string()]), &opts)
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn list_objects_filter_and_limit_combined() {
        let mut engine = MemoryEngine::new();
        for i in 0..4u32 {
            engine
                .put_object(MemoryObject::new(
                    "col",
                    format!("id-{i}"),
                    r#"{"status":"active"}"#,
                ))
                .unwrap();
        }
        engine
            .put_object(MemoryObject::new("col", "id-4", r#"{"status":"inactive"}"#))
            .unwrap();

        let opts = ListObjectsOptions {
            filter: vec![("status".into(), serde_json::json!("active"))],
            limit: Some(2),
            ..Default::default()
        };
        let results = engine
            .list_objects(Some(&["col".to_string()]), &opts)
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|o| o.body.contains("active")));
    }

    // ── append_event: RETURNING gives correct sequence + created_at ────────

    #[test]
    fn append_event_returns_sequence_and_timestamp() {
        let mut engine = MemoryEngine::new();

        let first = engine
            .append_event(MemoryEvent::new("s", "ev.first", r#"{"x":1}"#))
            .unwrap();
        let second = engine
            .append_event(MemoryEvent::new("s", "ev.second", r#"{"x":2}"#))
            .unwrap();

        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 2);
        assert!(!first.created_at.is_empty(), "created_at should be set");
    }

    // ── create_link: monotonic IDs survive deletes ─────────────────────────

    #[test]
    fn create_link_ids_are_unique_after_delete() {
        let mut engine = MemoryEngine::new();
        engine
            .put_object(MemoryObject::new("n", "a", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("n", "b", "{}"))
            .unwrap();
        engine
            .put_object(MemoryObject::new("n", "c", "{}"))
            .unwrap();

        let l1 = engine
            .create_link(Link::new("n/a", "connects", "n/b"))
            .unwrap();
        let l2 = engine
            .create_link(Link::new("n/b", "connects", "n/c"))
            .unwrap();

        // Delete the first link — the ID counter must NOT reset.
        engine.delete_link(&l1.id).unwrap();

        let l3 = engine
            .create_link(Link::new("n/a", "connects", "n/c"))
            .unwrap();

        assert_ne!(l3.id, l2.id, "IDs must not collide after a delete");
        assert!(l3.id > l2.id, "IDs must be monotonically increasing");
    }

    // ── list_objects: sort by created_at DESC ──────────────────────────────

    #[test]
    fn list_objects_sort_by_created_at_desc() {
        let mut engine = MemoryEngine::new();
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

    // ── list_objects: sort by id ASC ───────────────────────────────────────

    #[test]
    fn list_objects_sort_by_id_asc() {
        let mut engine = MemoryEngine::new();
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

    // ── delete_objects_batch ───────────────────────────────────────────────

    #[test]
    fn delete_objects_batch_deletes_multiple() {
        let mut engine = MemoryEngine::new();
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
        assert!(engine.get_object("w", "b").unwrap().is_none());
        assert!(engine.get_object("w", "c").unwrap().is_some());
    }

    // ── put_object_with_options: index=false ───────────────────────────────

    #[test]
    fn put_object_with_options_skip_index() {
        let mut engine = MemoryEngine::new();
        let opts = crate::PutObjectOptions {
            index: false,
            ..Default::default()
        };

        engine
            .put_object_with_options(MemoryObject::new("w", "a", r#"{"text":"hello"}"#), opts)
            .unwrap();

        let obj = engine.get_object("w", "a").unwrap();
        assert!(obj.is_some());
        assert_eq!(obj.unwrap().body, r#"{"text":"hello"}"#);
    }

    // ── optimistic locking / CAS ──────────────────────────────────────

    #[test]
    fn cas_succeeds_on_matching_version() {
        let mut engine = MemoryEngine::new();

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
    fn cas_fails_on_version_mismatch() {
        let mut engine = MemoryEngine::new();

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
    fn cas_fails_on_nonexistent_object() {
        let mut engine = MemoryEngine::new();

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
    fn cas_none_skips_check() {
        let mut engine = MemoryEngine::new();

        // Should succeed with expected_version: None (the default)
        let stored = engine
            .put_object_with_options(
                MemoryObject::new("col", "id", r#"{"v":1}"#),
                crate::PutObjectOptions::default(),
            )
            .unwrap();
        assert_eq!(stored.version, 1);
    }

    // ── event idempotency ─────────────────────────────────────────

    #[test]
    fn event_idempotency_returns_existing_event() {
        let mut engine = MemoryEngine::new();

        let mut event = MemoryEvent::new("stream", "test", r#"{"key":"val"}"#);
        event.idempotency_key = "idem-1".to_string();

        let first = engine.append_event(event.clone()).unwrap();
        assert_eq!(first.sequence, 1);

        let second = engine.append_event(event).unwrap();
        assert_eq!(second.sequence, first.sequence);
        assert_eq!(second.body, first.body);
    }

    #[test]
    fn event_idempotency_different_keys_are_distinct() {
        let mut engine = MemoryEngine::new();

        let mut event_a = MemoryEvent::new("stream", "test", r#"{"key":"a"}"#);
        event_a.idempotency_key = "idem-a".to_string();

        let mut event_b = MemoryEvent::new("stream", "test", r#"{"key":"b"}"#);
        event_b.idempotency_key = "idem-b".to_string();

        let first = engine.append_event(event_a).unwrap();
        let second = engine.append_event(event_b).unwrap();
        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 2);
    }

    #[test]
    fn protects_audit_stream_from_deletion() {
        let mut engine = MemoryEngine::new();
        engine
            .append_event(MemoryEvent::new(
                "__thingd:mcp:audit",
                "audit",
                r#"{"tool":"test"}"#,
            ))
            .unwrap();
        engine
            .append_event(MemoryEvent::new("normal", "test", "{}"))
            .unwrap();

        // Normal streams can be deleted
        engine.delete_stream("normal").unwrap();
        assert_eq!(engine.count_events().unwrap(), 1);

        // Protected stream rejects delete_stream
        let err = engine.delete_stream("__thingd:mcp:audit").unwrap_err();
        assert!(matches!(err, ThingdError::Protected(_)));

        // Protected stream rejects delete_last_event
        let err = engine.delete_last_event("__thingd:mcp:audit").unwrap_err();
        assert!(matches!(err, ThingdError::Protected(_)));

        // is_protected_stream returns true
        assert!(engine.is_protected_stream("__thingd:mcp:audit"));
        assert!(!engine.is_protected_stream("normal"));
    }
}
