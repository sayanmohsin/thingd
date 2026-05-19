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
