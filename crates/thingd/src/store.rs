//! Storage traits implemented by thingd storage adapters.

use crate::model::{
    AggregateOptions, AggregateResult, CollectionSchema, IndexDefinition, ListEventsOptions,
    ListObjectsOptions, MigrationRecord, PutObjectOptions, SchemaOptions, StoredSchema,
    TimeSeriesOptions, TimeSeriesResult,
};
use crate::{
    MemoryEvent, MemoryObject, QueueClaimOptions, QueueJob, QueueNackOptions, ThingdError,
    ThingdResult, VectorSearchHit, VectorSearchOptions,
};

/// Read-only storage counts used by diagnostics and operator tooling.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageDiagnostics {
    /// Number of stored objects.
    pub objects: u64,
    /// Number of stored events.
    pub events: u64,
    /// Number of stored links.
    pub links: u64,
    /// Number of queues.
    pub queues: u64,
    /// Number of active queue jobs.
    pub active_jobs: u64,
    /// Number of dead-letter jobs.
    pub dead_jobs: u64,
    /// Current primary-storage journal bytes, when available.
    pub journal_bytes: u64,
    /// Number of journals currently retained by the backend, when available.
    pub journal_count: u64,
}

/// Runtime maintenance state for a durable store.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageMaintenanceStatus {
    /// `idle`, `rebuilding_search`, `compacting`, `degraded`, or `failed`.
    pub state: String,
    /// Rebuild generation currently being processed, if any.
    pub generation: u64,
    /// Number of bounded recovery retries attempted.
    pub retry_count: u64,
    /// Last maintenance error, when recovery is degraded or failed.
    pub error: Option<String>,
}

impl Default for StorageMaintenanceStatus {
    fn default() -> Self {
        Self {
            state: "idle".to_string(),
            generation: 0,
            retry_count: 0,
            error: None,
        }
    }
}

/// Explicit retention request. No records are deleted when `dry_run` is true.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionOptions {
    /// Delete eligible records older than this Unix timestamp in milliseconds.
    pub before_unix_ms: i64,
    /// Report eligible records without modifying the store.
    #[serde(default)]
    pub dry_run: bool,
    /// Run a major storage compaction after deletion.
    #[serde(default)]
    pub compact: bool,
    /// Permit pruning the protected replication change stream up to the safe
    /// checkpoint shared by active replicas.
    #[serde(default)]
    pub include_replication: bool,
}

/// Result of an explicit retention operation.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionReport {
    /// Events eligible for or removed by retention.
    pub events: u64,
    /// Completed queue jobs eligible for or removed by retention.
    pub completed_jobs: u64,
    /// Dead-letter jobs eligible for or removed by retention.
    pub dead_jobs: u64,
    /// Replication events skipped because no safe checkpoint was available.
    pub skipped_replication_events: u64,
    /// Minimum active replica checkpoint used for safe pruning, if available.
    pub safe_replication_cursor: Option<u64>,
    /// Whether compaction was requested and completed.
    pub compacted: bool,
    /// Whether this was a dry run.
    pub dry_run: bool,
}

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

    /// Apply an object received from a replication source while preserving the
    /// source metadata carried by the object. Normal application writes should
    /// continue to use `put_object` so the local store owns versioning.
    ///
    /// # Errors
    ///
    /// Returns a storage error or a conflict when the target version does not
    /// match the supplied optimistic-locking expectation.
    fn put_object_with_source_metadata(
        &mut self,
        object: MemoryObject,
        options: PutObjectOptions,
    ) -> ThingdResult<MemoryObject> {
        self.put_object_with_options(object, options)
    }

    /// Read an object by collection and id.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot read the object.
    fn get_object(&self, collection: &str, id: &str) -> ThingdResult<Option<MemoryObject>>;

    /// Read multiple objects by collection and id in a single query.
    ///
    /// Returns objects in the same order as the input keys. Missing IDs
    /// produce `None` entries, preserving the order of the request.
    ///
    /// The default implementation loops calling `get_object`. The `SQLite`
    /// adapter overrides this with a single `WHERE id IN (...)` query.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot read the objects.
    fn get_objects_batch(
        &self,
        collection: &str,
        ids: &[String],
    ) -> ThingdResult<Vec<Option<MemoryObject>>> {
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            results.push(self.get_object(collection, id)?);
        }
        Ok(results)
    }

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

    /// Apply explicit retention. Adapters that cannot safely compact return an
    /// unsupported-operation error rather than deleting records by default.
    ///
    /// # Errors
    ///
    /// Returns an error when retention is unsupported or the backing store
    /// cannot read or delete records.
    fn retain(&mut self, _options: RetentionOptions) -> ThingdResult<RetentionReport> {
        Err(ThingdError::Storage(
            "retention is not supported by this adapter".to_string(),
        ))
    }

    /// Count objects in a specific collection.
    ///
    /// Uses `SELECT COUNT(*) FROM objects WHERE collection = ?` for O(log n)
    /// performance via the existing `idx_objects_collection` index.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot count objects.
    fn count_objects_in_collection(&self, _collection: &str) -> ThingdResult<u64> {
        self.count_objects()
    }

    /// List all unique collection names.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot list collections.
    fn list_collections(&self) -> ThingdResult<Vec<String>>;

    /// Create a functional index on a JSON body field for a collection.
    ///
    /// Creates a `SQLite` expression index on `json_extract(body, '$.field')`
    /// filtered to the given collection. Subsequent `list_objects` calls with
    /// `filter: { field: value }` will use this index for O(log n) lookups
    /// instead of full table scans.
    ///
    /// This is a no-op for in-memory stores (they already scan in-memory maps).
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot create the index.
    fn create_index(&mut self, _collection: &str, _field: &str) -> ThingdResult<()> {
        Ok(())
    }

    /// List all custom functional indexes.
    ///
    /// Returns `(collection, field)` pairs for each created index.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot list indexes.
    fn list_indexes(&self) -> ThingdResult<Vec<(String, String)>> {
        Ok(vec![])
    }

    /// Create or replace a functional index definition.
    ///
    /// # Errors
    ///
    /// Returns an error when the definition is invalid or existing data
    /// violates a unique definition.
    fn create_index_definition(&mut self, index: IndexDefinition) -> ThingdResult<()> {
        if index.collection.is_empty() || index.field.is_empty() {
            return Err(ThingdError::InvalidInput(
                "index collection and field are required".to_string(),
            ));
        }
        if index.unique {
            return Err(ThingdError::InvalidInput(
                "unique indexes are not supported by this adapter".to_string(),
            ));
        }
        self.create_index(&index.collection, &index.field)
    }

    /// Remove a functional index definition.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot remove the definition.
    fn delete_index(&mut self, _collection: &str, _field: &str) -> ThingdResult<bool> {
        Ok(false)
    }

    /// List index definitions, including uniqueness semantics.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot list definitions.
    fn list_index_definitions(&self) -> ThingdResult<Vec<IndexDefinition>> {
        Ok(self
            .list_indexes()?
            .into_iter()
            .map(|(collection, field)| IndexDefinition {
                collection,
                field,
                unique: false,
            })
            .collect())
    }

    /// Reflect the schema of all or one collection by sampling stored objects.
    ///
    /// Returns inferred field names, types, and sample values. When `collection`
    /// is `None`, returns schemas for all collections. When `Some(name)`, returns
    /// the schema for that collection or an empty vec if not found.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot read objects.
    fn schema(
        &self,
        collection: Option<&str>,
        options: &SchemaOptions,
    ) -> ThingdResult<Vec<CollectionSchema>>;
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
    /// Returns `true` if the stream is protected from deletion and external
    /// mutation. Protected streams (e.g. `"__thingd:mcp:audit"`) reject
    /// `delete_last_event` and `delete_stream` calls.
    fn is_protected_stream(&self, stream: &str) -> bool {
        let _ = stream;
        false
    }
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
    /// Returns `ThingdError::Protected` when the stream is protected.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot delete the event.
    fn delete_last_event(&mut self, stream: &str) -> ThingdResult<Option<MemoryEvent>> {
        if self.is_protected_stream(stream) {
            return Err(ThingdError::Protected(format!(
                "stream '{stream}' is protected and cannot be modified"
            )));
        }
        Err(ThingdError::Storage(
            "delete_last_event is not supported by this adapter".into(),
        ))
    }

    /// Delete all events in a stream.
    ///
    /// Returns the number of events deleted. This is useful for cleaning
    /// up completed or expired event streams (e.g. finished game matches).
    ///
    /// Returns `ThingdError::Protected` when the stream is protected.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot delete the events.
    fn delete_stream(&mut self, stream: &str) -> ThingdResult<u64> {
        if self.is_protected_stream(stream) {
            return Err(ThingdError::Protected(format!(
                "stream '{stream}' is protected and cannot be modified"
            )));
        }
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

/// Aggregation operations.
pub trait AggregateStore {
    /// Run a general aggregation query over objects in a collection.
    ///
    /// Supports count, sum, avg, min, max with optional `group_by`.
    ///
    /// # Errors
    ///
    /// Returns an error when the aggregation query fails.
    fn aggregate(
        &self,
        collection: &str,
        options: &AggregateOptions,
    ) -> ThingdResult<AggregateResult>;

    /// Run a time-bucketed aggregation query.
    ///
    /// Groups objects by hour/day/week/month and applies an aggregation function.
    ///
    /// # Errors
    ///
    /// Returns an error when the time-series query fails.
    fn timeseries(
        &self,
        collection: &str,
        options: &TimeSeriesOptions,
    ) -> ThingdResult<TimeSeriesResult>;
}

/// Vector search operations.
///
/// # Examples
///
/// ```rust
/// use thingd::{MemoryEngine, ObjectStore, VectorStore, MemoryObject, VectorSearchOptions};
///
/// let mut store = MemoryEngine::new();
/// store.put_object(
///     MemoryObject::new("docs", "a", r#"{"text":"hello"}"#)
///         .with_vector(vec![1.0, 0.0, 0.0]),
/// ).unwrap();
///
/// let results = store.vector_search("docs", &[0.9, 0.1, 0.0], VectorSearchOptions::default()).unwrap();
/// assert_eq!(results.len(), 1);
/// ```
pub trait VectorStore {
    /// Search objects by cosine similarity to the query vector.
    ///
    /// Returns results sorted by descending score (highest similarity first).
    /// When no vectors exist in the collection, returns an empty vec.
    ///
    /// # Errors
    ///
    /// Returns an error when the query vector is empty or its dimension does
    /// not match stored vectors.
    fn vector_search(
        &self,
        collection: &str,
        query_vector: &[f32],
        options: VectorSearchOptions,
    ) -> ThingdResult<Vec<VectorSearchHit>>;

    /// Add or update a vector for an object.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot persist the vector.
    fn add_vector(&mut self, collection: &str, id: &str, vector: &[f32]) -> ThingdResult<()>;

    /// Remove a vector for an object.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot remove the vector.
    fn remove_vector(&mut self, collection: &str, id: &str) -> ThingdResult<()>;
}

/// Full storage interface expected from thingd engine adapters.
pub trait ThingStore:
    EventLog
    + ObjectStore
    + QueueStore
    + Searcher
    + LinkStore
    + AggregateStore
    + VectorStore
    + SchemaStore
{
    /// Return whether the adapter has an asynchronous derived search rebuild
    /// that should be progressed by the hosting runtime.
    fn search_rebuild_required(&self) -> bool {
        false
    }

    /// Process one bounded asynchronous search rebuild batch.
    ///
    /// # Errors
    ///
    /// Returns a storage error when the adapter cannot advance the rebuild.
    fn search_rebuild_step(&mut self, _batch_size: usize) -> ThingdResult<bool> {
        Ok(true)
    }

    /// Retry a degraded search rebuild when bounded recovery permits another generation.
    fn retry_search_rebuild(&mut self) -> bool {
        false
    }

    /// Return backend-specific asynchronous search rebuild status, when supported.
    #[cfg(feature = "persistent")]
    fn search_rebuild_status(&self) -> Option<crate::SearchRebuildStatus> {
        None
    }

    /// Return backend-specific storage maintenance state.
    fn storage_maintenance_status(&self) -> crate::StorageMaintenanceStatus {
        crate::StorageMaintenanceStatus::default()
    }

    /// Compact the primary durable store when supported.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend cannot persist or compact its data.
    fn compact_storage(&mut self) -> ThingdResult<()> {
        Err(ThingdError::Storage(
            "storage compaction is unavailable".to_string(),
        ))
    }

    /// Return bounded storage diagnostics. Adapters may add backend-specific
    /// details through their own APIs without changing this stable summary.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot read its counts.
    fn storage_diagnostics(&self) -> ThingdResult<StorageDiagnostics> {
        Ok(StorageDiagnostics {
            objects: self.count_objects()?,
            events: self.count_events()?,
            links: self.count_links()?,
            queues: self.list_queues()?.len() as u64,
            active_jobs: self.count_active_jobs()?,
            dead_jobs: self.count_dead_jobs()?,
            journal_bytes: 0,
            journal_count: 0,
        })
    }
}

impl ThingStore for crate::MemoryEngine {}

#[cfg(feature = "persistent")]
impl ThingStore for crate::PersistentEngine {
    fn search_rebuild_required(&self) -> bool {
        self.search_rebuild_required()
    }

    fn search_rebuild_step(&mut self, batch_size: usize) -> ThingdResult<bool> {
        self.search_rebuild_step(batch_size)
    }

    fn retry_search_rebuild(&mut self) -> bool {
        self.retry_search_rebuild()
    }

    fn search_rebuild_status(&self) -> Option<crate::SearchRebuildStatus> {
        self.search_rebuild_status()
    }

    fn storage_maintenance_status(&self) -> crate::StorageMaintenanceStatus {
        self.storage_maintenance_status()
    }

    fn compact_storage(&mut self) -> ThingdResult<()> {
        self.compact_storage()
    }

    fn storage_diagnostics(&self) -> ThingdResult<StorageDiagnostics> {
        Ok(StorageDiagnostics {
            objects: self.count_objects()?,
            events: self.count_events()?,
            links: self.count_links()?,
            queues: self.list_queues()?.len() as u64,
            active_jobs: self.count_active_jobs()?,
            dead_jobs: self.count_dead_jobs()?,
            journal_bytes: self.journal_bytes(),
            journal_count: self.journal_count(),
        })
    }
}

/// Durable schema and migration metadata operations.
pub trait SchemaStore {
    /// Read the last applied canonical schema, if any.
    ///
    /// # Errors
    ///
    /// Returns an error when metadata cannot be read.
    fn get_schema_document(&self) -> ThingdResult<Option<StoredSchema>>;

    /// Store the canonical schema metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when metadata cannot be written.
    fn put_schema_document(&mut self, schema: StoredSchema) -> ThingdResult<()>;

    /// List applied migrations in deterministic order.
    ///
    /// # Errors
    ///
    /// Returns an error when migration metadata cannot be read.
    fn list_migrations(&self) -> ThingdResult<Vec<MigrationRecord>>;

    /// Record an applied migration.
    ///
    /// # Errors
    ///
    /// Returns an error when migration metadata cannot be written.
    fn record_migration(&mut self, migration: MigrationRecord) -> ThingdResult<()>;
}
