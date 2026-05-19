# Persistence And Native Bindings

This document captures the Phase 3 direction for turning `memoryd` from a TypeScript in-memory proof into a Rust-backed local engine.

## Current Decision

Keep the public package as:

```txt
@sayanmohsin/memoryd
```

Add native support underneath it rather than making users learn a second API.

The intended layering is:

```txt
Node app
  |
  v
@sayanmohsin/memoryd
  TypeScript API and types
  MemoryStore interface
  in-memory fallback/proof store
  native store adapter later
  |
  v
@sayanmohsin/memoryd-native
  napi-rs binding package
  |
  v
crates/memoryd-core
  object store traits
  event log traits
  queue store traits
  storage adapters
```

## Rust Boundary

`crates/memoryd-core` owns the durable engine boundary:

- object model types
- event model types
- queue model types
- storage traits
- in-memory engine used for tests and API design

The important traits are:

```txt
ObjectStore
EventLog
QueueStore
MemoryStore
```

Future storage adapters should implement those traits instead of exposing storage-specific APIs directly to Node.js.

## Storage Options

### SQLite

Pros:

- mature and portable
- excellent local durability story
- JSON, indexes, FTS, transactions
- good path to LiteFS/libSQL style read replicas later

Cons:

- needs careful queue leasing implementation
- vector search requires extension strategy
- schema/migration discipline required

Current fit: best first durable backend candidate.

### redb

Pros:

- Rust-native embedded key-value store
- simple file-based deployment
- no SQL layer

Cons:

- less familiar to app developers
- query/search/indexing has to be built above it
- weaker future story for SQL-compatible inspection

Current fit: possible low-level event/object store, but not first choice.

### Append log plus derived indexes

Pros:

- great for auditability and rebuildable indexes
- maps well to events and agent timelines
- useful for replication later

Cons:

- more custom engine work
- compaction and snapshots become our responsibility
- slower path to first durable MVP

Current fit: useful internal pattern, not a standalone first storage backend.

## Recommended v0 Persistence Path

Start with SQLite as the first durable adapter.

Use SQLite for:

- objects
- events
- queue jobs
- metadata indexes
- future FTS tables

Keep vector search and multi-pod replication out of the first durable milestone.

## N-API Direction

Use `napi-rs` for embedded Node.js bindings when Phase 4 begins.

Expected shape:

```txt
MemoryD.open("./memoryd.db")
  -> TypeScript SDK
  -> NativeMemoryStore
  -> napi-rs binding
  -> memoryd-core SQLite adapter
```

The TypeScript SDK should keep the same app-facing methods:

```txt
put
get
delete
search
events.append
events.list
queue.push
queue.claim
queue.ack
queue.nack
queue.dead
```

The native binding should satisfy the existing Node tests before it becomes the default store.

## Non-goals For This Phase

- no SQLite implementation yet
- no native binary build yet
- no prebuild matrix yet
- no server/sidecar mode yet
- no MCP implementation yet

## Phase 4 Candidate Scope

The next phase should likely be:

1. Add a SQLite-backed Rust adapter.
2. Add Rust tests for object/event/queue persistence.
3. Add a `napi-rs` binding that can open a database file.
4. Add a TypeScript `NativeMemoryStore` adapter.
5. Run the existing SDK tests against both in-memory and native stores.
