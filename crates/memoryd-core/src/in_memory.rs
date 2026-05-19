//! In-memory storage adapter used for API design and tests.

use std::collections::{BTreeMap, VecDeque};

use crate::{
    u64_to_i64, unix_timestamp_millis, EventLog, MemoryEvent, MemoryObject, MemorydError,
    MemorydResult, ObjectKey, ObjectStore, QueueClaimOptions, QueueJob, QueueJobStatus,
    QueueNackOptions, QueueStore,
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
    fn put_object(&mut self, mut object: MemoryObject) -> MemorydResult<MemoryObject> {
        let version = self
            .objects
            .get(&object.key)
            .map_or(1, |existing| existing.version + 1);

        object.version = version;
        self.objects.insert(object.key.clone(), object.clone());

        Ok(object)
    }

    fn get_object(&self, collection: &str, id: &str) -> MemorydResult<Option<MemoryObject>> {
        Ok(self.objects.get(&ObjectKey::new(collection, id)).cloned())
    }

    fn list_objects(&self, collections: Option<&[String]>) -> MemorydResult<Vec<MemoryObject>> {
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

    fn delete_object(&mut self, collection: &str, id: &str) -> MemorydResult<bool> {
        Ok(self
            .objects
            .remove(&ObjectKey::new(collection, id))
            .is_some())
    }
}

impl EventLog for MemoryEngine {
    fn append_event(&mut self, mut event: MemoryEvent) -> MemorydResult<MemoryEvent> {
        self.next_event_sequence += 1;
        event.sequence = self.next_event_sequence;
        self.events.push(event.clone());

        Ok(event)
    }

    fn list_events(&self, stream: Option<&str>) -> MemorydResult<Vec<MemoryEvent>> {
        let events = self
            .events
            .iter()
            .filter(|event| stream.is_none_or(|target| event.stream == target))
            .cloned()
            .collect();

        Ok(events)
    }
}

impl QueueStore for MemoryEngine {
    fn push_job(&mut self, job: QueueJob) -> MemorydResult<QueueJob> {
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
    ) -> MemorydResult<Option<QueueJob>> {
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

    fn ack_job(&mut self, queue: &str, id: &str) -> MemorydResult<Option<QueueJob>> {
        let Some(job) = self.find_job_mut(queue, id) else {
            return Ok(None);
        };

        if job.status != QueueJobStatus::Leased {
            return Err(MemorydError::Conflict(format!(
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
    ) -> MemorydResult<Option<QueueJob>> {
        let Some(job) = self.find_job_mut(queue, id) else {
            return Ok(None);
        };

        if job.status != QueueJobStatus::Leased {
            return Err(MemorydError::Conflict(format!(
                "job {id} must be leased before nack"
            )));
        }

        let now = unix_timestamp_millis();
        job.leased_at_ms = None;
        job.lease_expires_at_ms = None;
        job.status = if job.attempts >= job.max_attempts {
            job.dead_at_ms = Some(now);
            QueueJobStatus::Dead
        } else {
            job.available_at_ms = now.saturating_add(u64_to_i64(options.delay_ms));
            QueueJobStatus::Ready
        };

        Ok(Some(job.clone()))
    }

    fn list_jobs(&self, queue: &str) -> MemorydResult<Vec<QueueJob>> {
        Ok(self
            .queues
            .get(queue)
            .map_or_else(Vec::new, |jobs| jobs.iter().cloned().collect()))
    }

    fn list_dead_jobs(&self, queue: &str) -> MemorydResult<Vec<QueueJob>> {
        Ok(self.queues.get(queue).map_or_else(Vec::new, |jobs| {
            jobs.iter()
                .filter(|job| job.status == QueueJobStatus::Dead)
                .cloned()
                .collect()
        }))
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

#[cfg(test)]
mod tests {
    use super::*;

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
                "project:memoryd",
                "decision.made",
                "MCP-native object storage",
            ))
            .unwrap();

        assert_eq!(event.sequence, 1);
        assert_eq!(
            engine.list_events(Some("project:memoryd")).unwrap().len(),
            1
        );
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
}
