//! Storage traits implemented by memoryd storage adapters.

use crate::{
    MemoryEvent, MemoryObject, MemorydResult, QueueClaimOptions, QueueJob, QueueNackOptions,
};

/// Object storage operations.
pub trait ObjectStore {
    /// Insert or replace an object.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot persist the object.
    fn put_object(&mut self, object: MemoryObject) -> MemorydResult<MemoryObject>;

    /// Read an object by collection and id.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot read the object.
    fn get_object(&self, collection: &str, id: &str) -> MemorydResult<Option<MemoryObject>>;

    /// Delete an object by collection and id.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot delete the object.
    fn delete_object(&mut self, collection: &str, id: &str) -> MemorydResult<bool>;
}

/// Append-only event log operations.
pub trait EventLog {
    /// Append an event to a stream.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot append the event.
    fn append_event(&mut self, event: MemoryEvent) -> MemorydResult<MemoryEvent>;

    /// List events, optionally filtered by stream.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot read events.
    fn list_events(&self, stream: Option<&str>) -> MemorydResult<Vec<MemoryEvent>>;
}

/// Queue storage operations.
pub trait QueueStore {
    /// Push a job onto a queue.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot persist the job.
    fn push_job(&mut self, job: QueueJob) -> MemorydResult<QueueJob>;

    /// Claim the next ready job from a queue.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot claim a job.
    fn claim_job(&mut self, queue: &str) -> MemorydResult<Option<QueueJob>> {
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
    ) -> MemorydResult<Option<QueueJob>>;

    /// Acknowledge a leased job as completed.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot update the job.
    fn ack_job(&mut self, queue: &str, id: &str) -> MemorydResult<Option<QueueJob>>;

    /// Reject a leased job for retry or dead-letter routing.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot update the job.
    fn nack_job(&mut self, queue: &str, id: &str) -> MemorydResult<Option<QueueJob>> {
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
    ) -> MemorydResult<Option<QueueJob>>;

    /// List all jobs in a queue.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot read queue jobs.
    fn list_jobs(&self, queue: &str) -> MemorydResult<Vec<QueueJob>>;

    /// List dead-letter jobs in a queue.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot read dead-letter jobs.
    fn list_dead_jobs(&self, queue: &str) -> MemorydResult<Vec<QueueJob>>;
}

/// Full storage interface expected from memoryd engine adapters.
pub trait MemoryStore: EventLog + ObjectStore + QueueStore {}

impl<T> MemoryStore for T where T: EventLog + ObjectStore + QueueStore {}
