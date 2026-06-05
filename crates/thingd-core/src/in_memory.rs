//! In-memory storage adapter used for API design and tests.

use std::collections::{BTreeMap, VecDeque};

use crate::{
    now_iso_string, u64_to_i64, unix_timestamp_millis, EventLog, MemoryEvent, MemoryObject,
    ObjectKey, ObjectStore, QueueClaimOptions, QueueJob, QueueJobStatus, QueueNackOptions,
    QueueStore, ThingdError, ThingdResult,
};

/// In-memory engine used to prove the storage boundary.
#[derive(Default)]
pub struct MemoryEngine {
    objects: BTreeMap<ObjectKey, MemoryObject>,
    events: Vec<MemoryEvent>,
    queues: BTreeMap<String, VecDeque<QueueJob>>,
    next_event_sequence: u64,
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

    fn get_object(&self, collection: &str, id: &str) -> ThingdResult<Option<MemoryObject>> {
        Ok(self.objects.get(&ObjectKey::new(collection, id)).cloned())
    }

    fn list_objects(&self, collections: Option<&[String]>) -> ThingdResult<Vec<MemoryObject>> {
        let objects = self
            .objects
            .values()
            .filter(|object| {
                collections.is_none_or(|allowed| allowed.contains(&object.key.collection))
            })
            .cloned()
            .collect();

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
}

impl EventLog for MemoryEngine {
    fn append_event(&mut self, mut event: MemoryEvent) -> ThingdResult<MemoryEvent> {
        self.next_event_sequence += 1;
        event.sequence = self.next_event_sequence;
        if event.created_at.is_empty() {
            event.created_at = now_iso_string();
        }
        self.events.push(event.clone());

        Ok(event)
    }

    fn list_events(&self, stream: Option<&str>) -> ThingdResult<Vec<MemoryEvent>> {
        let events = self
            .events
            .iter()
            .filter(|event| stream.is_none_or(|target| event.stream == target))
            .cloned()
            .collect();

        Ok(events)
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
            if let Some(ref collections) = options.collections {
                if !collections.contains(&object.key.collection) {
                    continue;
                }
            }

            // Apply metadata filter
            if let Some(ref filter) = options.filter {
                if !matches_filter_memory(&object.body, filter) {
                    continue;
                }
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
            if let Some(ref collections) = options.collections {
                if !collections.contains(&event.stream) {
                    continue;
                }
            }

            // Apply metadata filter
            if let Some(ref filter) = options.filter {
                if !matches_filter_memory(&event.body, filter) {
                    continue;
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Searcher;
    use crate::SearchOptions;

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
            .list_objects(Some(&["decisions".to_string()]))
            .unwrap();

        assert_eq!(engine.list_objects(None).unwrap().len(), 2);
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
        assert_eq!(engine.list_events(Some("project:thingd")).unwrap().len(), 1);
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
}
