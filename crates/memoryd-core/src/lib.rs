//! Core primitives for memoryd.
//!
//! This crate starts with the public shape of the engine: objects, events,
//! and queues. The current implementation is intentionally in-memory so the
//! API can settle before the durable storage layer is introduced.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::{BTreeMap, VecDeque};

/// An object stored in a memoryd collection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryObject {
    /// Collection name, such as `decisions`, `documents`, or `customers`.
    pub collection: String,
    /// Stable object identifier inside the collection.
    pub id: String,
    /// Serialized object body.
    pub body: String,
}

/// An append-only event stored in a memoryd stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryEvent {
    /// Stream name, such as `project:memoryd` or `customer:cus_123`.
    pub stream: String,
    /// Event kind, such as `decision.made`.
    pub event_type: String,
    /// Serialized event body.
    pub body: String,
}

/// A queued unit of work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueJob {
    /// Queue name.
    pub queue: String,
    /// Stable job identifier.
    pub id: String,
    /// Serialized job payload.
    pub body: String,
    /// Number of attempts already made.
    pub attempts: u32,
    /// Maximum attempts before the job should be considered failed.
    pub max_attempts: u32,
}

/// In-memory engine used to prove the public API shape.
#[derive(Default)]
pub struct MemoryEngine {
    objects: BTreeMap<(String, String), MemoryObject>,
    events: Vec<MemoryEvent>,
    queues: BTreeMap<String, VecDeque<QueueJob>>,
}

impl MemoryEngine {
    /// Create a new empty in-memory engine.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace an object.
    pub fn put_object(
        &mut self,
        collection: impl Into<String>,
        id: impl Into<String>,
        body: impl Into<String>,
    ) {
        let object = MemoryObject {
            collection: collection.into(),
            id: id.into(),
            body: body.into(),
        };

        self.objects
            .insert((object.collection.clone(), object.id.clone()), object);
    }

    /// Read an object by collection and id.
    pub fn get_object(&self, collection: &str, id: &str) -> Option<&MemoryObject> {
        self.objects.get(&(collection.to_owned(), id.to_owned()))
    }

    /// Delete an object by collection and id.
    pub fn delete_object(&mut self, collection: &str, id: &str) -> Option<MemoryObject> {
        self.objects.remove(&(collection.to_owned(), id.to_owned()))
    }

    /// Append an event to a stream.
    pub fn append_event(
        &mut self,
        stream: impl Into<String>,
        event_type: impl Into<String>,
        body: impl Into<String>,
    ) {
        self.events.push(MemoryEvent {
            stream: stream.into(),
            event_type: event_type.into(),
            body: body.into(),
        });
    }

    /// Return all events in append order.
    pub fn events(&self) -> &[MemoryEvent] {
        &self.events
    }

    /// Push a job onto the back of a named queue.
    pub fn push_job(
        &mut self,
        queue: impl Into<String>,
        id: impl Into<String>,
        body: impl Into<String>,
        max_attempts: u32,
    ) {
        let queue = queue.into();
        let job = QueueJob {
            queue: queue.clone(),
            id: id.into(),
            body: body.into(),
            attempts: 0,
            max_attempts,
        };

        self.queues.entry(queue).or_default().push_back(job);
    }

    /// Claim the next ready job from a queue.
    pub fn claim_job(&mut self, queue: &str) -> Option<QueueJob> {
        self.queues.get_mut(queue).and_then(VecDeque::pop_front)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_reads_objects() {
        let mut engine = MemoryEngine::new();

        engine.put_object("decisions", "rust-core", "{\"text\":\"Use Rust\"}");

        let object = engine.get_object("decisions", "rust-core").unwrap();
        assert_eq!(object.collection, "decisions");
        assert_eq!(object.id, "rust-core");
    }

    #[test]
    fn appends_events() {
        let mut engine = MemoryEngine::new();

        engine.append_event("project:memoryd", "decision.made", "MCP-native object storage");

        assert_eq!(engine.events().len(), 1);
        assert_eq!(engine.events()[0].event_type, "decision.made");
    }

    #[test]
    fn claims_queue_jobs_fifo() {
        let mut engine = MemoryEngine::new();

        engine.push_job("embed", "job-1", "doc-1", 3);
        engine.push_job("embed", "job-2", "doc-2", 3);

        assert_eq!(engine.claim_job("embed").unwrap().id, "job-1");
        assert_eq!(engine.claim_job("embed").unwrap().id, "job-2");
        assert!(engine.claim_job("embed").is_none());
    }
}
