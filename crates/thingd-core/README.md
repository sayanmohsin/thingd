# thingd-core

Core storage primitives for [thingd](https://github.com/sayanmohsin/thingd) — an
object-shaped local memory engine for apps and agents.

This crate owns the storage boundary: object CRUD, append-only events, job
queues, full-text search, and graph links. The default engine is in-memory for
API design and testing. The optional `sqlite` feature enables the
`rusqlite`-backed `SqliteThingStore` for durable storage.

## Installation

```toml
[dependencies]
thingd-core = { version = "0.1", features = ["sqlite"] }
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `sqlite` | No | Enables `rusqlite`-backed `SqliteThingStore` with FTS5 search |
| `connectors` | No | Enables CSV/JSON file connectors for data import |

## Quick Start

```rust
use thingd_core::{MemoryEngine, ObjectStore, MemoryObject};

let mut engine = MemoryEngine::new();

// Store an object
let obj = MemoryObject::new("users", "alice", r#"{"name":"Alice"}"#);
engine.put_object(obj).unwrap();

// Retrieve it
let user = engine.get_object("users", "alice").unwrap();
assert_eq!(user.unwrap().body, r#"{"name":"Alice"}"#);
```

## Traits

The crate is built around composable traits:

- **`ObjectStore`** — CRUD for objects in named collections
- **`EventLog`** — Append-only event streams
- **`QueueStore`** — Job queues with lease/ack/nack lifecycle
- **`Searcher`** — Full-text search across objects and events
- **`LinkStore`** — Graph links between objects
- **`ThingStore`** — Super-trait combining all of the above

## License

Apache-2.0 — see [LICENSE](../../LICENSE).
