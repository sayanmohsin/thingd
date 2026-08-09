# Architecture

thingd is a **polymorphic data engine** — one binary that adapts its storage backend to the deployment context. The same MCP protocol, SDK, and REST API work whether you're running in-memory on a laptop, persisted on a server, or embedded in a browser WASM context.

## Core principle: everything behind the trait

All storage operations go through the `ThingStore` trait — six sub-traits that define the complete data surface:

```
ObjectStore       put/get/list/delete objects in collections
QueueStore        push/claim/ack/nack durable jobs
EventLog          append/read events per stream
Searcher          full-text search with metadata filters
LinkStore         create/query directed graph edges
AggregateStore    aggregate, group-by, time bucketing
```

These six traits are unified under the `ThingStore` supertrait. The engine never imports concrete backend types outside of the factory:

```rust
pub type SharedEngine = Arc<Mutex<Box<dyn ThingStore + Send>>>;

pub fn create_engine(db_path: &str) -> Result<Box<dyn ThingStore + Send>> {
    if db_path == ":memory:" || db_path.is_empty() {
        return Ok(Box::new(MemoryEngine::new()));
    }
    Ok(Box::new(PersistentEngine::open_with_options(db_path, options)?))
}
```

## Two storage backends

thingd ships with two backends. A third backend means implementing the six traits:

```
Native binary:    PersistentEngine (durable local storage) + MemoryEngine (cache/warm)
WASM binary:      InMemory (browser/edge, no file I/O available)
```

| Backend | Type | Persist | WASM | Use case |
|---------|------|---------|------|----------|
| **MemoryEngine** | `BTreeMap` + `Vec` | No | Yes | Cache, WASM, testing ~675K ops/s |
| **PersistentEngine** | durable local storage | Yes | No | Production and single-node deployments |

### persistent storage layout

The persistent engine stores objects, events, queues, links, search data, and vectors
under one configured storage directory. The layout is an implementation detail and
may change between engine versions:

| Area | Key format | Value | Operations |
|----------|-----------|-------|------------|
| `objects` | `{collection}\0{id}` | serialized `MemoryObject` | prefix scan by collection, point get/put/delete |
| `events` | `{stream}\0{seq:8BE}` | serialized `MemoryEvent` | prefix scan by stream, sequence counter |
| `queue_jobs` | `{queue}\0{id}` | serialized `QueueJob` | prefix scan by queue, point update for claim/ack/nack |
| `links_by_id` | `{link_id}` | serialized `Link` | point get/put/delete |
| `links_from` | `{from_ref}\0{type}\0{link_id}` | `()` | prefix scan for outgoing neighbors |
| `links_to` | `{to_ref}\0{type}\0{link_id}` | `()` | prefix scan for incoming neighbors |

Callers should use the public store traits and backup/restore commands rather than
depending on internal files or directory entries.

The `ThingStore` trait ensures the storage backend is invisible above the engine layer. If a different persistence implementation emerges, it can implement the six traits without changing callers.

## Full-text search: Tantivy

thingd uses **Tantivy** (pure Rust, BM25 ranking) for full-text search.

```
Search index directory:  {data_dir}/search/
On write:               index (collection, id, body) in Tantivy
On delete:              remove from Tantivy
On startup:             verify index consistency, rebuild if stale
```

Tantivy is used by the persistent backend for full-text search and is feature-gated
with `search`. The index is derived state: if an older SDK created an incompatible
schema (including one without `doc_key`), thingd discards only the Tantivy
directory and rebuilds it from the durable object and event records. Primary data
is not migrated or discarded.

## Vector search: persisted cosine similarity

thingd stores optional object vectors in persistent storage and ranks matches by cosine similarity. The current implementation scans the collection exactly; HNSW/ANN is a future scale milestone.

```
Storage:    vectors persisted alongside objects
Features:   metadata filtering and exact cosine ranking
Use case:   agent semantic memory, similarity search over embeddings
WASM:       in-memory vector search remains the browser-compatible path
```

New MCP tools when vectors are enabled:

- `thing_vector_search` — kNN search by embedding vector with optional metadata filter

Vector search is additive — it doesn't replace keyword FTS. Search queries can combine both signals.

## WASM/browser target

thingd compiles to `wasm32-unknown-unknown` with the **InMemoryEngine** backend. No file I/O is available in WASM — all data lives in memory for the session duration.

```
Target:       wasm32-unknown-unknown
Backend:      InMemoryEngine (HashMap + Vec)
Transport:    MCP over stdio, or embedded as a JS module
Use cases:    Browser agent clients, state cache, edge workers
```

Future: an IndexedDB persist adapter for browser WASM, enabling cross-session durability in the browser.

## Cloud hosting

[thingd Cloud](https://thingd.cloud) is the managed hosted version of thingd. Same engine, same API — zero infrastructure to manage. Create projects, issue API keys, and connect agents to hosted thingd instances through an HTTPS MCP endpoint with audit logging, scoped access control, and tenant isolation.

### Replication boundary

Thingd-to-Thingd replication is provider-neutral: one instance is explicitly
configured as the authoritative source and another as the replica. The source
and target may both be local, self-hosted, or hosted. The protocol carries
objects, deletes, application events, source metadata, cursors, and idempotency
keys; stale cursors recover through snapshots. Replicas reject direct writes,
and divergent target state is quarantined rather than overwritten.

`thingd.cloud` is the protected/default hosted provider. Cloud-specific tenant,
instance-selection, policy, and audit checks belong to thingd-cloud; the open
source engine only implements the provider-neutral replication contract.

## MCP server

The Node SDK MCP server ships **48 tools** across objects, events, queues, search, graph links, aggregation, schema, NLQ, vectors, and scheduling. The Rust sidecar ships 38 engine tools; every sidecar tool goes through the same `ThingStore` trait. Native persistence options, including encryption, are resolved before either MCP runtime starts.

```
Object CRUD:    thing_get, thing_put, thing_delete, thing_objects_list
Batch ops:      thing_objects_put_batch, thing_objects_delete_batch, thing_objects_get_batch
Search:         thing_search
Events:         thing_events_append, thing_events_list
Queues:         thing_queue_push, thing_queue_claim, thing_queue_ack, thing_queue_nack
                thing_queue_list, thing_queue_dead
Graph links:    thing_link_create, thing_link_get, thing_link_delete
                thing_link_neighbors, thing_link_count
Discovery:      thing_list_collections, thing_list_streams, thing_list_queues
Counts:         thing_count_objects, thing_count_objects_in_collection
                thing_count_events, thing_count_active_jobs, thing_count_dead_jobs
Indexes:        thing_create_index, thing_list_indexes
Analytics:      thing_aggregate, thing_timeseries
Schema + NLQ:   thing_schema, thing_nlq
Vector:         thing_vector_upsert, thing_vector_search (when vectors enabled)
```

MCP transport: stdio (local agents) and Streamable HTTP (remote agents).

See [mcp-server.md](./mcp-server.md) for the full reference.

## Deployment modes

```
┌───────────────────────────────────────────────────────────────┐
│                     thingd binary                              │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │ CLI + TUI   │  │ MCP (stdio)  │  │ Server/REST  │          │
│  │ (operator)  │  │ (AI agents)  │  │ (HTTP/gRPC)  │          │
│  └──────┬──────┘  └──────┬───────┘  └──────┬───────┘          │
│         │               │                  │                   │
│  ┌──────┴───────────────┴──────────────────┴────────────────┐ │
│  │                ThingStore trait (6 sub-traits)             │ │
│  └──────┬───────────────────────────────┬────────────────────┘ │
│         │ InMemory                     │ persistent                 │
│         │ (cache, WASM)                │ (production, persist) │
│         │ in-memory speed              │ durable local storage  │
│         │ + Tantivy (FTS)              │ + Tantivy (FTS)       │
│         │ + in-memory vectors          │ + persistent vectors       │
│         └──────────────────────────────┘───────────────────────┘
```

| Mode | How | Backend |
|------|-----|---------|
| **Rust embedded** | `use thingd::{PersistentEngine, ObjectStore}` | persistent or InMemory |
| **Node.js embedded** | `new ThingD({driver:"native"})` | persistent or InMemory |
| **MCP sidecar** | `thingd mcp` | PersistentEngine with optional storage codec |
| **REST sidecar** | `thingd-server` (axum) | PersistentEngine |
| **Docker** | `docker run thingd/thingd-server` | PersistentEngine on a persistent volume |
| **Browser/edge** | `@thingd/client` + SDK memory store | InMemory |
| **WASM agent** | compiled to `wasm32-unknown-unknown` | InMemory |
| **Cluster** | leader/follower via Raft (`open-raft`) | persistent + Raft log |
| **thingd.cloud** | managed hosted | persistent per workspace |

## Multi-pod / clustering

Clustering uses **Raft consensus** via `open-raft`:

```
Leader:     accepts writes, replicates log to followers
Followers:  serve read queries, forward writes to leader
```

Queue operations (`claim_job`, `ack_job`) require linearizability — they go through the leader. Read operations (`get_object`, `search`) dispatch to any follower.

The cluster log is stored alongside the persistent data. The state machine is the
`ThingStore` trait — clustering wraps it, not replaces it.

## Package layout

```
crates/
  thingd/              Rust engine — traits, InMemory, persistent, Tantivy, vectors
  thingd-server/       Rust sidecar — axum REST + MCP + cluster

packages/
  thingd/              Node.js SDK (@thingd/sdk) — three driver backends
  thingd-client/       Zero-dep HTTP REST client (@thingd/client)
  thingd-native/       Private napi-rs native binding
  thingd-cli/          CLI, TUI dashboard, MCP servers
```

## Swappable backend guarantee

To add a new storage backend:

1. Implement the six traits (`ObjectStore`, `QueueStore`, `EventLog`, `Searcher`, `LinkStore`, `AggregateStore`)
2. Add one match arm in `create_engine()`
3. No other code changes

The **only** files that know about concrete backend types:

- `crates/thingd/src/lib.rs` — re-exports the backend
- `crates/thingd-server/src/engine.rs` — `create_engine()` factory

Everything else uses `Box<dyn ThingStore + Send>` and never sees the concrete type.
