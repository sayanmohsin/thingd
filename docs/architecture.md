# Architecture

`memoryd` is split into a Rust core and thin developer-facing packages.

```txt
Rust core
  object store
  event log
  queue engine
  graph links
  locks and leases
  workflow DAG
  semantic cache
  tool-call ledger
  search indexes
  storage adapters

Node SDK
  app-facing API
  worker consumers
  remote sidecar client
  local process management

CLI
  developer inspection
  operator diagnostics
  script-friendly admin output

MCP server
  agent-facing tools
  safe read/write surface

Server/sidecar
  localhost app bridge
  Kubernetes peer discovery
  leader write forwarding
  future event replication
```

The public Node SDK should remain the app-facing contract. Native bindings should sit underneath that SDK rather than creating a separate API surface.
Remote sidecar mode also stays underneath that SDK through the `remote` driver,
which talks to the HTTP MCP runtime.

## Storage Model

The durable engine should be append-friendly and rebuildable:

```txt
objects
events
queue_jobs
links
leases
workflow_runs
cache_entries
tool_calls
snapshots
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

The first durable adapter is `SqliteMemoryStore`, enabled by the Rust crate's
`sqlite` feature. It currently persists objects, events, and trait-level queue
jobs with `rusqlite`. Queue claim, ack, nack, retry delay, delayed
availability, lease expiration, and dead-letter updates are transactional.
Schema version tracking lives in `memoryd_schema_migrations`.

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

Sidecar cluster mode is planned as a runtime layer above SQLite, not a
multi-primary SQLite design. Each app talks to a local `memoryd` sidecar. The
current bridge scaffold exposes peer metadata and can run as `single`,
`leader`, or `follower`. Followers forward MCP traffic to the configured leader.
Follower replica catch-up is still planned.

For the detailed API, environment, Kubernetes, and phase plan, read
[sidecar-cluster.md](./sidecar-cluster.md).

## AI-Native Primitive Direction

`memoryd` should prioritize workflow primitives that help agents understand,
retrieve, coordinate, and audit work. The planned order is graph links, hybrid
search, locks/leases/semaphores, workflow DAGs, semantic cache, tool-call
ledger, and compaction snapshots.

For target APIs, storage shapes, MCP surfaces, and phase planning, read
[ai-primitives.md](./ai-primitives.md).

## MCP Server Direction

`packages/memoryd-mcp` wraps the public SDK as MCP tools. It provides stdio for
local MCP clients and Streamable HTTP for remote-capable runtimes. Write tools
append audit events by default. The Docker runtime starts the HTTP MCP endpoint,
exposes bridge status endpoints, and persists data under `/data`.

For current tools and local usage, read [mcp-server.md](./mcp-server.md) and
[docker-runtime.md](./docker-runtime.md). Runtime env vars are centralized in
[runtime-env.md](./runtime-env.md).

## CLI Direction

The MCP package has runtime entrypoints, and `packages/memoryd-cli` now provides
the first-pass `memoryd` admin/operator CLI. The CLI uses the public SDK for
local and remote access and can inspect objects, events, queues, dead jobs, MCP
tools, and runtime status. The next CLI phase should add operator polish before
any inspector UI is built.

For command phases and handoff details, read [cli.md](./cli.md).

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

The native binding package now has an initial private `napi-rs` bridge. The public SDK can opt into it with `driver: "native"` after the native package is built locally. The default SDK path remains the in-memory store until native prebuilds and release packaging are ready.
