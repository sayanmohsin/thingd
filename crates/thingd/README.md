# thingd

[![Crates.io](https://img.shields.io/crates/v/thingd?logo=rust&color=ff6a00)](https://crates.io/crates/thingd)
[![Documentation](https://img.shields.io/docsrs/thingd?logo=rust&color=00c4d4)](https://docs.rs/thingd)
[![License](https://img.shields.io/crates/l/thingd?color=ff6a00)](./LICENSE)

Core engine for [thingd](https://github.com/sayanmohsin/thingd) — an
object-first data engine for applications and AI agents.

This crate provides the storage boundary: object CRUD, append-only events,
durable job queues, full-text search, and graph links. It ships with two
engines: an in-memory engine for fast prototyping and testing, and an optional
persistent engine for durable local storage.

## Why thingd?

Modern apps and AI agents commonly need:

- **Object storage** without designing relational schemas first
- **Full-text search** across objects and events
- **Append-only event logs** for audit trails and timelines
- **Durable job queues** with leases, retries, and dead-letter handling
- **Graph links** between objects, memories, and decisions

Today you stitch these together from 3-5 separate tools. thingd gives you
all five primitives behind a single composable trait interface.

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `persistent` | Yes | Enables the durable persistent engine |
| `connectors` | No | Enables CSV/JSON file connectors for data import |

## Quick Start

### In-memory engine (zero setup)

```rust
use thingd::{MemoryEngine, ObjectStore, MemoryObject};

let mut engine = MemoryEngine::new();

engine.put_object(MemoryObject::new("users", "alice", r#"{"name":"Alice"}"#)).unwrap();

let user = engine.get_object("users", "alice").unwrap();
assert_eq!(user.unwrap().body, r#"{"name":"Alice"}"#);
```

### persistent engine (durable storage)

```rust
use thingd::{PersistentEngine, ObjectStore, MemoryObject};

let mut db = PersistentEngine::open("/tmp/thingd-data").unwrap();

db.put_object(MemoryObject::new("users", "alice", r#"{"name":"Alice"}"#)).unwrap();

let user = db.get_object("users", "alice").unwrap();
assert_eq!(user.unwrap().body, r#"{"name":"Alice"}"#);
```

### Full-text search

```rust
use thingd::{PersistentEngine, ObjectStore, MemoryObject, Searcher};

let mut db = PersistentEngine::open("/tmp/thingd-data").unwrap();

db.put_object(MemoryObject::new("docs", "readme", "# Hello\nThis is the project README")).unwrap();
db.put_object(MemoryObject::new("docs", "api", "# API Reference\nEndpoints for the REST API")).unwrap();

let hits = db.search("project", Default::default()).unwrap();
assert!(!hits.is_empty());
```

### Append-only events

```rust
use thingd::{PersistentEngine, EventLog, MemoryEvent};

let mut db = PersistentEngine::open("/tmp/thingd-data").unwrap();

db.append_event(MemoryEvent::new("project:thingd", "decision.made", r#"{"text":"Use Rust for the core"}"#)).unwrap();
```

### Durable job queues

```rust
use thingd::{PersistentEngine, QueueStore, QueueJob, QueueClaimOptions};

let mut db = PersistentEngine::open("/tmp/thingd-data").unwrap();

let job = QueueJob::new("embeddings", "job-1", r#"{"doc_id":"readme"}"#, 3).into();
db.push_job(job).unwrap();

let claimed = db.claim_job_with_options("embeddings", QueueClaimOptions::new(30_000)).unwrap();
assert!(claimed.is_some());
db.ack_job("embeddings", &claimed.unwrap().id).unwrap();
```

### Graph links

```rust
use thingd::{PersistentEngine, LinkStore, Link, LinkDirection};

let mut db = PersistentEngine::open("/tmp/thingd-data").unwrap();

db.create_link(Link::new("users/alice", "authored", "docs/readme")).unwrap();

let neighbors = db.get_neighbors("users/alice", LinkDirection::Outgoing, Default::default()).unwrap();
assert_eq!(neighbors.len(), 1);
```

## Traits

The crate is built around composable traits:

| Trait | Description |
|-------|-------------|
| `ObjectStore` | CRUD for versioned JSON objects in named collections |
| `EventLog` | Append-only event streams with sequence numbers |
| `QueueStore` | Job queues with lease/ack/nack lifecycle, retries, dead-letter |
| `Searcher` | Full-text search with collection filters and recency ranking |
| `LinkStore` | Typed graph links between objects |
| `ThingStore` | Super-trait combining all of the above |

Both `MemoryEngine` and `PersistentEngine` implement all storage traits.

## Key Types

| Type | Description |
|------|-------------|
| `MemoryObject` | A versioned JSON object keyed by `(collection, id)` |
| `MemoryEvent` | An event with stream, type, body, and sequence number |
| `QueueJob` | A job with status, attempts, lease, and retry metadata |
| `SearchHit` | A search result with kind, collection, score, and body |
| `Link` | A typed directed edge between two object references |
| `ThingdError` | Error type (`InvalidInput`, `NotFound`, `Conflict`, `Storage`) |

## Comparison

| Tool | Great at | Why thingd is different |
|------|----------|------------------------------|
| PersistentEngine | durable local storage | object API, events, queues, search, graph, vectors |
| MongoDB | flexible documents | local-first, Rust, no server process |
| Redis / BullMQ | fast queues | durable local storage without Redis |
| LanceDB | vector search | broader memory runtime with events and queues |
| Diesel / SQLx | type-safe SQL | higher-level object/queue/event API |

## License

Apache-2.0 — see [LICENSE](../../LICENSE).
