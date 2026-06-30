//! Storage traits implemented by thingd storage adapters.

use crate::model::{ListEventsOptions, ListObjectsOptions, PutObjectOptions};
use crate::{
    MemoryEvent, MemoryObject, QueueClaimOptions, QueueJob, QueueNackOptions, ThingdError,
    ThingdResult,
};

/// Object storage operations.
///
/// # Examples
///
/// ```rust
/// use thingd::{MemoryEngine, ObjectStore, MemoryObject};
///
/// let mut store = MemoryEngine::new();
/// let obj = MemoryObject::new("users", "alice", r#"{"name":"Alice"}"#);
/// store.put_object(obj).unwrap();
///
/// let user = store.get_object("users", "alice").unwrap();
/// assert!(user.is_some());
/// assert_eq!(store.count_objects().unwrap(), 1);
/// ```
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
    /// **Atomicity:** The `SQLite` adapter wraps all writes in a single
    /// transaction — a failure rolls back all changes. The in-memory
    /// default implementation loops calling `put_object` without a
    /// transaction, so a partial failure may leave some objects written.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot persist any object.
    fn put_objects_batch(&mut self, objects: Vec<MemoryObject>) -> ThingdResult<Vec<MemoryObject>> {
        let mut results = Vec::with_capacity(objects.len());
        for object in objects {
            results.push(self.put_object(object)?);
        }
        Ok(results)
    }

    /// Insert or replace an object with explicit options.
    ///
    /// When `options.index` is `false`, the FTS search index is not updated.
    /// Use this when only metadata changes (e.g. timestamp dedup) and the body
    /// text is identical — avoids wasted FTS DELETE + INSERT.
    ///
    /// When `options.expected_version` is `Some(v)`, the operation succeeds only
    /// if the current version of the object equals `v` (optimistic locking / CAS).
    /// Returns `ThingdError::Conflict` on version mismatch.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot persist the object.
    fn put_object_with_options(
        &mut self,
        object: MemoryObject,
        options: PutObjectOptions,
    ) -> ThingdResult<MemoryObject> {
        let _ = options;
        self.put_object(object)
    }

    /// Read an object by collection and id.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot read the object.
    fn get_object(&self, collection: &str, id: &str) -> ThingdResult<Option<MemoryObject>>;

    /// List objects in one or more collections, with optional filtering, limit, and offset.
    ///
    /// Pass an empty `ListObjectsOptions` to return all objects across all collections.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot list objects.
    fn list_objects(
        &self,
        collections: Option<&[String]>,
        options: &ListObjectsOptions,
    ) -> ThingdResult<Vec<MemoryObject>>;

    /// Delete an object by collection and id.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot delete the object.
    fn delete_object(&mut self, collection: &str, id: &str) -> ThingdResult<bool>;

    /// Delete multiple objects in a single transaction.
    ///
    /// Returns the number of deleted objects. The `SQLite` adapter emits a bulk
    /// `DELETE` statement in one transaction. The default implementation loops
    /// calling `delete_object`.
    ///
    /// **Atomicity:** The `SQLite` adapter wraps all deletes in a single
    /// transaction — a failure rolls back all deletions. The in-memory
    /// default implementation loops calling `delete_object` without a
    /// transaction, so a partial failure may leave some objects deleted
    /// and others not.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot delete any object.
    fn delete_objects_batch(&mut self, keys: &[(String, String)]) -> ThingdResult<u64> {
        let mut count = 0u64;
        for (collection, id) in keys {
            if self.delete_object(collection, id)? {
                count += 1;
            }
        }
        Ok(count)
    }

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
///
/// # Examples
///
/// ```rust
/// use thingd::{MemoryEngine, EventLog, MemoryEvent, ListEventsOptions};
///
/// let mut store = MemoryEngine::new();
/// let event = MemoryEvent::new("audit", "user.created", r#"{"user":"alice"}"#);
/// store.append_event(event).unwrap();
///
/// let events = store.list_events(None, ListEventsOptions::default()).unwrap();
/// assert_eq!(events.len(), 1);
/// assert_eq!(events[0].event_type, "user.created");
/// ```
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
    fn append_events_batch(&mut self, events: Vec<MemoryEvent>) -> ThingdResult<Vec<MemoryEvent>> {
        let mut results = Vec::with_capacity(events.len());
        for event in events {
            results.push(self.append_event(event)?);
        }
        Ok(results)
    }

    /// List events, optionally filtered by stream, with pagination.
    ///
    /// Events are returned in ascending sequence order (oldest first).
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot read events.
    fn list_events(
        &self,
        stream: Option<&str>,
        options: ListEventsOptions,
    ) -> ThingdResult<Vec<MemoryEvent>>;

    /// Delete the most recent event from a stream.
    ///
    /// Returns the deleted event, or `None` if the stream was empty or
    /// did not exist. This is useful for implementing undo patterns in
    /// event-sourced applications.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot delete the event.
    fn delete_last_event(&mut self, _stream: &str) -> ThingdResult<Option<MemoryEvent>> {
        Err(ThingdError::Storage(
            "delete_last_event is not supported by this adapter".into(),
        ))
    }

    /// Delete all events in a stream.
    ///
    /// Returns the number of events deleted. This is useful for cleaning
    /// up completed or expired event streams (e.g. finished game matches).
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot delete the events.
    fn delete_stream(&mut self, _stream: &str) -> ThingdResult<u64> {
        Err(ThingdError::Storage(
            "delete_stream is not supported by this adapter".into(),
        ))
    }

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
///
/// # Examples
///
/// ```rust
/// use thingd::{MemoryEngine, QueueStore, QueueJob, QueueJobStatus};
///
/// let mut store = MemoryEngine::new();
/// let job = QueueJob::new("emails", "job-1", r#"{"to":"alice@example.com"}"#, 3);
/// store.push_job(job).unwrap();
///
/// let claimed = store.claim_job("emails").unwrap();
/// assert!(claimed.is_some());
/// let job = claimed.unwrap();
/// assert_eq!(job.status, QueueJobStatus::Leased);
///
/// let completed = store.ack_job("emails", &job.id).unwrap();
/// assert_eq!(completed.unwrap().status, QueueJobStatus::Completed);
/// ```
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
    fn push_jobs_batch(&mut self, jobs: Vec<QueueJob>) -> ThingdResult<Vec<QueueJob>> {
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
///
/// # Examples
///
/// ```rust
/// use thingd::{MemoryEngine, ObjectStore, Searcher, MemoryObject, SearchOptions};
///
/// let mut store = MemoryEngine::new();
/// store.put_object(MemoryObject::new("docs", "readme", "Getting started guide")).unwrap();
///
/// let results = store.search("getting started", SearchOptions::default()).unwrap();
/// assert!(!results.is_empty());
/// ```
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

/// Graph link operations.
pub trait LinkStore {
    /// Create a new graph link.
    ///
    /// # Errors
    ///
    /// Returns an error when the link cannot be persisted.
    fn create_link(&mut self, link: crate::Link) -> ThingdResult<crate::Link>;

    /// Delete a graph link by id.
    ///
    /// # Errors
    ///
    /// Returns an error when the link cannot be deleted.
    fn delete_link(&mut self, id: &str) -> ThingdResult<bool>;

    /// Get a graph link by id.
    ///
    /// # Errors
    ///
    /// Returns an error when the link cannot be read.
    fn get_link(&self, id: &str) -> ThingdResult<Option<crate::Link>>;

    /// Get neighbors of a reference (outgoing, incoming, or both).
    ///
    /// # Errors
    ///
    /// Returns an error when neighbors cannot be queried.
    fn get_neighbors(
        &self,
        reference: &str,
        direction: crate::LinkDirection,
        options: crate::LinkQueryOptions,
    ) -> ThingdResult<Vec<crate::Link>>;

    /// Count total links.
    ///
    /// # Errors
    ///
    /// Returns an error when count fails.
    fn count_links(&self) -> ThingdResult<u64>;
}

/// Full storage interface expected from thingd engine adapters.
pub trait ThingStore: EventLog + ObjectStore + QueueStore + Searcher + LinkStore {}

impl<T> ThingStore for T where T: EventLog + ObjectStore + QueueStore + Searcher + LinkStore {}
