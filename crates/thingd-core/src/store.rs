//! Storage traits implemented by thingd storage adapters.

use crate::model::ListEventsOptions;
use crate::{
    MemoryEvent, MemoryObject, QueueClaimOptions, QueueJob, QueueNackOptions, ThingdResult,
};

/// Object storage operations.
pub trait ObjectStore {
    /// Insert or replace an object.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot persist the object.
    fn put_object(&mut self, object: MemoryObject) -> ThingdResult<MemoryObject>;

    /// Insert or replace multiple objects in a single transaction.
    ///
    /// This is significantly faster than calling `put_object` in a loop
    /// because it avoids per-object transaction overhead.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot persist any object.
    fn put_objects_batch(
        &mut self,
        objects: Vec<MemoryObject>,
    ) -> ThingdResult<Vec<MemoryObject>> {
        let mut results = Vec::with_capacity(objects.len());
        for object in objects {
            results.push(self.put_object(object)?);
        }
        Ok(results)
    }

    /// Read an object by collection and id.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot read the object.
    fn get_object(&self, collection: &str, id: &str) -> ThingdResult<Option<MemoryObject>>;

    /// List objects, optionally restricted to the provided collections.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot list objects.
    fn list_objects(&self, collections: Option<&[String]>) -> ThingdResult<Vec<MemoryObject>>;

    /// Delete an object by collection and id.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot delete the object.
    fn delete_object(&mut self, collection: &str, id: &str) -> ThingdResult<bool>;

    /// Count total objects across all collections.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot count objects.
    fn count_objects(&self) -> ThingdResult<u64>;

    /// List all unique collection names.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot list collections.
    fn list_collections(&self) -> ThingdResult<Vec<String>>;
}

/// Append-only event log operations.
pub trait EventLog {
    /// Append an event to a stream.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot append the event.
    fn append_event(&mut self, event: MemoryEvent) -> ThingdResult<MemoryEvent>;

    /// Append multiple events to a stream in a single transaction.
    ///
    /// This is significantly faster than calling `append_event` in a loop.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot append any event.
    fn append_events_batch(
        &mut self,
        events: Vec<MemoryEvent>,
    ) -> ThingdResult<Vec<MemoryEvent>> {
        let mut results = Vec::with_capacity(events.len());
        for event in events {
            results.push(self.append_event(event)?);
        }
        Ok(results)
    }

    /// List events, optionally filtered by stream, with pagination.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot read events.
    fn list_events(
        &self,
        stream: Option<&str>,
        options: ListEventsOptions,
    ) -> ThingdResult<Vec<MemoryEvent>>;

    /// Count total events across all streams.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot count events.
    fn count_events(&self) -> ThingdResult<u64>;

    /// List all unique stream names.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot list streams.
    fn list_streams(&self) -> ThingdResult<Vec<String>>;
}

/// Queue storage operations.
pub trait QueueStore {
    /// Push a job onto a queue.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot persist the job.
    fn push_job(&mut self, job: QueueJob) -> ThingdResult<QueueJob>;

    /// Push multiple jobs onto a queue in a single transaction.
    ///
    /// This is significantly faster than calling `push_job` in a loop.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot persist any job.
    fn push_jobs_batch(
        &mut self,
        jobs: Vec<QueueJob>,
    ) -> ThingdResult<Vec<QueueJob>> {
        let mut results = Vec::with_capacity(jobs.len());
        for job in jobs {
            results.push(self.push_job(job)?);
        }
        Ok(results)
    }

    /// Claim the next ready job from a queue.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot claim a job.
    fn claim_job(&mut self, queue: &str) -> ThingdResult<Option<QueueJob>> {
        self.claim_job_with_options(queue, QueueClaimOptions::default())
    }

    /// Claim the next ready job from a queue with explicit options.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot claim a job.
    fn claim_job_with_options(
        &mut self,
        queue: &str,
        options: QueueClaimOptions,
    ) -> ThingdResult<Option<QueueJob>>;

    /// Acknowledge a leased job as completed.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot update the job.
    fn ack_job(&mut self, queue: &str, id: &str) -> ThingdResult<Option<QueueJob>>;

    /// Claim and immediately ack a job in a single transaction.
    ///
    /// This is faster than calling `claim_job` + `ack_job` separately
    /// because it avoids per-operation transaction overhead.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot claim or ack the job.
    fn claim_and_ack(
        &mut self,
        queue: &str,
        options: QueueClaimOptions,
    ) -> ThingdResult<Option<QueueJob>> {
        if let Some(job) = self.claim_job_with_options(queue, options)? {
            self.ack_job(queue, &job.id)
        } else {
            Ok(None)
        }
    }

    /// Reject a leased job for retry or dead-letter routing.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot update the job.
    fn nack_job(&mut self, queue: &str, id: &str) -> ThingdResult<Option<QueueJob>> {
        self.nack_job_with_options(queue, id, QueueNackOptions::default())
    }

    /// Reject a leased job for retry or dead-letter routing with explicit options.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot update the job.
    fn nack_job_with_options(
        &mut self,
        queue: &str,
        id: &str,
        options: QueueNackOptions,
    ) -> ThingdResult<Option<QueueJob>>;

    /// List all jobs in a queue.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot read queue jobs.
    fn list_jobs(&self, queue: &str) -> ThingdResult<Vec<QueueJob>>;

    /// List dead-letter jobs in a queue.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot read dead-letter jobs.
    fn list_dead_jobs(&self, queue: &str) -> ThingdResult<Vec<QueueJob>>;

    /// List all unique queue names.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot list queues.
    fn list_queues(&self) -> ThingdResult<Vec<String>>;

    /// Count total active jobs across all queues.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot count active jobs.
    fn count_active_jobs(&self) -> ThingdResult<u64>;

    /// Count total dead-letter jobs across all queues.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot count dead jobs.
    fn count_dead_jobs(&self) -> ThingdResult<u64>;
}

/// Search operations.
pub trait Searcher {
    /// Search memory objects and event logs by query text.
    ///
    /// # Errors
    ///
    /// Returns an error when search query fails.
    fn search(
        &self,
        query: &str,
        options: crate::SearchOptions,
    ) -> ThingdResult<Vec<crate::SearchHit>>;
}

/// Full storage interface expected from thingd engine adapters.
pub trait ThingStore: EventLog + ObjectStore + QueueStore + Searcher {}

impl<T> ThingStore for T where T: EventLog + ObjectStore + QueueStore + Searcher {}
