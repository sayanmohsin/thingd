//! In-memory storage adapter used for API design and tests.

use std::collections::{BTreeMap, VecDeque};

use crate::{
    EventLog, MemoryEvent, MemoryObject, MemorydError, MemorydResult, ObjectKey, ObjectStore,
    QueueJob, QueueJobStatus, QueueStore,
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

    fn claim_job(&mut self, queue: &str) -> MemorydResult<Option<QueueJob>> {
        let Some(jobs) = self.queues.get_mut(queue) else {
            return Ok(None);
        };

        let Some(job) = jobs
            .iter_mut()
            .find(|candidate| candidate.status == QueueJobStatus::Ready)
        else {
            return Ok(None);
        };

        job.status = QueueJobStatus::Leased;
        job.attempts += 1;

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

        Ok(Some(job.clone()))
    }

    fn nack_job(&mut self, queue: &str, id: &str) -> MemorydResult<Option<QueueJob>> {
        let Some(job) = self.find_job_mut(queue, id) else {
            return Ok(None);
        };

        if job.status != QueueJobStatus::Leased {
            return Err(MemorydError::Conflict(format!(
                "job {id} must be leased before nack"
            )));
        }

        job.status = if job.attempts >= job.max_attempts {
            QueueJobStatus::Dead
        } else {
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
}
