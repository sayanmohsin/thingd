# Architecture

`memoryd` is split into a Rust core and thin developer-facing packages.

```txt
Rust core
  object store
  event log
  queue engine
  search indexes
  storage adapters

Node SDK
  app-facing API
  worker consumers
  local process management

MCP server
  agent-facing tools
  safe read/write surface
```

The public Node SDK should remain the app-facing contract. Native bindings should sit underneath that SDK rather than creating a separate API surface.

## Storage Model

The durable engine should be append-friendly and rebuildable:

```txt
objects
events
queue_jobs
links
indexes
```

Objects are the primary app-facing shape. Events explain how state changed. Indexes are derived and can be rebuilt.

`crates/memoryd-core` defines the current storage boundary with these traits:

```txt
ObjectStore
EventLog
QueueStore
MemoryStore
```

Future durable adapters should implement those traits.

The first durable adapter is `SqliteMemoryStore`, enabled by the Rust crate's `sqlite` feature. It currently persists objects and events with `rusqlite`; queue persistence remains in the next phase because leasing and dead-letter behavior need transactional implementation.

## Queue Model

Queues should use at-least-once delivery:

```txt
ready -> leased -> completed
ready -> leased -> retry
ready -> leased -> dead-letter
```

The first multi-pod coordination story is a single primary writer with many consumers. Exactly-once delivery is not promised; idempotency keys and dedupe keys should be part of the API.

## Multi-pod Direction

The practical path is:

1. single-node embedded mode
2. sidecar/server mode with one primary writer
3. local read replicas
4. tenant or queue partitioning
5. consensus only if real demand proves it is worth the complexity

## Native Binding Direction

The expected embedded path is:

```txt
@sayanmohsin/memoryd
  TypeScript public API
  native store adapter

@sayanmohsin/memoryd-native
  napi-rs binding package

crates/memoryd-core
  durable engine traits and adapters
```

The native binding package is a scaffold only right now.
The Rust core has SQLite object/event persistence, but the public Node SDK does not call it yet.
