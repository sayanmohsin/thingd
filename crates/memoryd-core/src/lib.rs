//! Core primitives for memoryd.
//!
//! This crate starts with the public shape of the engine: objects, events,
//! and queues. The current implementation is intentionally in-memory so the
//! API can settle before the durable storage layer is introduced.

use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryObject {
    pub collection: String,
    pub id: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryEvent {
    pub stream: String,
    pub event_type: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueJob {
    pub queue: String,
    pub id: String,
    pub body: String,
    pub attempts: u32,
    pub max_attempts: u32,
}

#[derive(Default)]
pub struct MemoryEngine {
    objects: BTreeMap<(String, String), MemoryObject>,
    events: Vec<MemoryEvent>,
    queues: BTreeMap<String, VecDeque<QueueJob>>,
}

impl MemoryEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put_object(&mut self, collection: impl Into<String>, id: impl Into<String>, body: impl Into<String>) {
        let object = MemoryObject {
            collection: collection.into(),
            id: id.into(),
            body: body.into(),
        };

        self.objects
            .insert((object.collection.clone(), object.id.clone()), object);
    }

    pub fn get_object(&self, collection: &str, id: &str) -> Option<&MemoryObject> {
        self.objects.get(&(collection.to_owned(), id.to_owned()))
    }

    pub fn delete_object(&mut self, collection: &str, id: &str) -> Option<MemoryObject> {
        self.objects.remove(&(collection.to_owned(), id.to_owned()))
    }

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

    pub fn events(&self) -> &[MemoryEvent] {
        &self.events
    }

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
