# Persistence And Native Bindings

This document captures the direction for turning `memoryd` from a TypeScript in-memory proof into a Rust-backed local engine.

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
- feature-gated SQLite adapter for durable object, event, and queue storage

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

- vector search requires extension strategy
- schema/migration discipline required
- existing development databases may need explicit migrations as queue columns evolve

Current fit: selected first durable backend. The adapter uses `rusqlite` for object, event, and trait-level queue persistence, including delayed jobs, configurable lease expiration, retry delay, and dead-letter state.

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

Current implementation status:

- objects: implemented in `SqliteMemoryStore`
- events: implemented in `SqliteMemoryStore`
- queue jobs: implemented in `SqliteMemoryStore`
- Node SDK native adapter: not implemented yet
- benchmarks: `npm run bench:rust` covers the Rust object/event/queue storage path

Keep vector search and multi-pod replication out of the first durable milestone.

## N-API Direction

Use `napi-rs` for embedded Node.js bindings after the Rust SQLite adapter has the storage behavior the SDK needs.

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

- no native binary build yet
- no prebuild matrix yet
- no server/sidecar mode yet
- no MCP implementation yet
- no Node SDK native adapter yet

## Phase 4 Scope

Phase 4 starts the SQLite adapter without switching the public Node SDK to it yet.

Implemented:

1. Add a `rusqlite`-backed Rust adapter behind the `sqlite` feature.
2. Persist objects with version increments.
3. Persist append-only events with monotonic sequences.
4. Add Rust tests for object and event persistence across database reopen.
5. Run Rust CI checks with all features enabled.

Completed in Phase 5:

1. Add SQLite queue persistence for `QueueStore`.
2. Store queue jobs durably across database reopen.
3. Support idempotent queue push by `(queue, id)`.
4. Support transactional claim with `ready -> leased`.
5. Support `ack` with `leased -> completed`.
6. Support `nack` with `leased -> ready` or `leased -> dead`.
7. Add Rust tests for queue persistence, duplicate push, ack, retry, dead-letter, and reopen behavior.

Completed in Phase 6:

1. Add Rust model support for delayed jobs and configurable lease expiration.
2. Add Rust model support for retry delay on `nack`.
3. Persist queue availability, lease, completion, and dead-letter timestamps in SQLite.
4. Reclaim expired leases before queue claim.
5. Add in-memory and SQLite Rust tests for delay, lease expiration, retry delay, and queue timestamps.

Remaining:

1. Add a `napi-rs` binding that can open a database file.
2. Add a TypeScript `NativeMemoryStore` adapter.
3. Run the existing SDK tests against both in-memory and native stores.
