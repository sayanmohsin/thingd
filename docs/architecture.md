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
    if is_fjall_path(db_path) {
        return Ok(Box::new(FjallEngine::open(db_path)?));
    }
    Ok(Box::new(FjallEngine::open(db_path)?))
}
```

## Two storage backends

thingd ships with two backends. A third backend means implementing the six traits:

```
Native binary:    Fjall (persistent LSM-tree) + InMemory (cache/warm)
WASM binary:      InMemory (browser/edge, no file I/O available)
```

| Backend | Type | Persist | WASM | Use case |
|---------|------|---------|------|----------|
| **MemoryEngine** | `BTreeMap` + `Vec` | No | Yes | Cache, WASM, testing ~675K ops/s |
| **FjallEngine** | LSM-tree (pure Rust) | Yes (WAL) | No | Production, multi-GB datasets, 100K+ ops/s |

No SQLite. No RocksDB. No redb. Two backends. Pure Rust.

### Fjall partition layout

A single `fjall::Database` directory holds all data in isolated keyspaces:

| Keyspace | Key format | Value | Operations |
|----------|-----------|-------|------------|
| `objects` | `{collection}\0{id}` | serialized `MemoryObject` | prefix scan by collection, point get/put/delete |
| `events` | `{stream}\0{seq:8BE}` | serialized `MemoryEvent` | prefix scan by stream, sequence counter |
| `queue_jobs` | `{queue}\0{id}` | serialized `QueueJob` | prefix scan by queue, point update for claim/ack/nack |
| `links_by_id` | `{link_id}` | serialized `Link` | point get/put/delete |
| `links_from` | `{from_ref}\0{type}\0{link_id}` | `()` | prefix scan for outgoing neighbors |
| `links_to` | `{to_ref}\0{type}\0{link_id}` | `()` | prefix scan for incoming neighbors |

Prefix scans enable efficient collection-scoped queries. The `objects` partition's `{collection}\0{id}` key layout turns `list_objects("users")` into a fast prefix iteration without a full scan.

### Why Fjall over SQLite

- LSМ-tree architecture handles 100K+ write ops/s (SQLite bottlenecks at ~50K)
- Pure Rust — zero C dependencies, `cargo build` just works
- No cmake, no C toolchain, no ARM cross-compile pain
- ~3-5MB binary overhead vs SQLite's bundled C (also ~3-5MB)
- Daily development with active benchmarks and performance tracking
- MVCC snapshots for concurrent readers

The `ThingStore` trait ensures the storage backend is invisible above the engine layer. If a better backend emerges, implement the six traits and swap.

## Full-text search: Tantivy

thingd uses **Tantivy** (pure Rust, BM25 ranking) for full-text search, replacing the SQLite FTS5 that was tied to the old SQLite backend.

```
Search index directory:  {data_dir}/search/
On write:               index (collection, id, body) in Tantivy
On delete:              remove from Tantivy
On startup:             verify index consistency, rebuild if stale
```

Tantivy is always available with the Fjall backend. Feature-gated with `search`.

## Vector search: embedvec

thingd integrates **embedvec** (0.8.0+) for semantic vector search, using the same Fjall database instance for HNSW graph persistence.

```
embedvec:   HNSW index persisted in Fjall partitions
Features:   metadata filtering, E8/H4 lattice quantization (up to 24.8x compression)
Use case:   agent semantic memory, similarity search over embeddings
WASM:       embedvec has a `wasm` feature for in-memory vector search in browsers
```

New MCP tools when vectors are enabled:

- `thing_vector_upsert` — store embedding + metadata reference to an object
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

## MCP server

thingd ships **46 SDK MCP tools** across objects, events, queues, search, graph links, aggregation, schema, NLQ, and scheduling. The Rust sidecar exposes the 36 core tools. Every core tool goes through the same `ThingStore` trait — the backend choice is invisible to the agent.

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
│         │ InMemory                     │ Fjall                 │
│         │ (cache, WASM)                │ (production, persist) │
│         │ ~675K ops/s                  │ ~100K+ ops/s + WAL    │
│         │ + Tantivy (FTS)              │ + Tantivy (FTS)       │
│         │ + embedvec (vectors, WASM)   │ + embedvec (vectors)  │
│         └──────────────────────────────┘───────────────────────┘
```

| Mode | How | Backend |
|------|-----|---------|
| **Rust embedded** | `use thingd::{FjallEngine, ObjectStore}` | Fjall or InMemory |
| **Node.js embedded** | `new ThingD({driver:"native"})` | Fjall or InMemory |
| **MCP sidecar** | `thingd mcp` | Fjall (persisted) |
| **REST sidecar** | `thingd-server` (axum) | Fjall (persisted) |
| **Docker** | `docker run thingd/thingd-server` | Fjall (persistent volume) |
| **Browser/edge** | `@thingd/client` + SDK memory store | InMemory |
| **WASM agent** | compiled to `wasm32-unknown-unknown` | InMemory |
| **Cluster** | static leader/follower forwarding and election | Fjall per node |
| **thingd.cloud** | managed hosted | Fjall per workspace |

## Multi-pod / clustering

Clustering uses static leader/follower configuration:

```
Leader:     accepts local reads and writes
Followers:  forward MCP writes to the configured leader
```

Leader election, when enabled, promotes the next peer from the ordered static
peer list after repeated leader failures. This is not Raft or another consensus
protocol; operators must provide fencing and split-brain protection. The current
follower mode does not replicate a local store or expose a replication log route.

## Package layout

```
crates/
  thingd/              Rust engine — traits, InMemory, Fjall, Tantivy, embedvec
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
