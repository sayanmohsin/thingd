# Architecture

`thingd` is split into a Rust core and thin developer-facing packages.

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
  event replication
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
indexes
```

Objects are the primary app-facing shape. Events explain how state changed. Indexes are derived and can be rebuilt.

`crates/thingd-core` defines the current storage boundary with these traits:

```txt
ObjectStore
EventLog
QueueStore
ThingStore
```

Future durable adapters should implement those traits.

The first durable adapter is `SqliteThingStore`, enabled by the Rust crate's
`sqlite` feature. It currently persists objects, events, and trait-level queue
jobs with `rusqlite`. Queue claim, ack, nack, retry delay, delayed
availability, lease expiration, and dead-letter updates are transactional.
Schema version tracking lives in `thingd_schema_migrations`.

## Queue Model

Queues should use at-least-once delivery:

```txt
ready -> leased -> completed
ready -> leased -> retry
ready -> leased -> dead-letter
```

The first multi-pod coordination story is a single primary writer with many consumers. Exactly-once delivery is not promised; idempotency keys and dedupe keys should be part of the API.

## Multi-pod Architecture

Sidecar cluster mode runs as a runtime layer above SQLite, not a
multi-primary SQLite design. Each app talks to a local `thingd` sidecar. The
current bridge scaffold exposes peer metadata and can run as `single`,
`leader`, or `follower`. Followers forward MCP traffic to the configured leader.
With `THINGD_CLUSTER_LEADER_ELECTION=true`, followers auto-promote the next
peer in the ordered peer list to leader when the current leader becomes
unreachable, enabling automatic failover in static deployments (e.g.,
Kubernetes StatefulSets with ordered pod names).

For the runtime API, environment, and Kubernetes deployment, see
[docker-runtime.md](./docker-runtime.md) and [mcp-server.md](./mcp-server.md).

## MCP Server

`packages/thingd-cli` wraps the public SDK as MCP tools. It provides stdio for
local MCP clients and Streamable HTTP for remote-capable runtimes via the integrated `mcp` and `mcp-http` subcommands. Write tools
append audit events by default. The Docker runtime starts the HTTP MCP endpoint,
exposes bridge status endpoints, and persists data under `/data`.

For current tools and local usage, read [mcp-server.md](./mcp-server.md) and
[docker-runtime.md](./docker-runtime.md). Runtime env vars are centralized in
[runtime-env.md](./runtime-env.md).

## CLI

The unified `packages/thingd-cli` package houses both the MCP server entrypoints and the full-featured `thingd` admin/operator CLI. The CLI uses the public SDK for
local and remote access and can inspect objects, events, queues, dead jobs, MCP
tools, and runtime status.

For commands and runtime options, read [cli-reference.md](./cli-reference.md).

## Native Binding

The embedded path is:

```txt
thingd
  TypeScript public API
  native store adapter

thingd-native
  napi-rs binding package

crates/thingd-core
  durable engine traits and adapters
```

The native binding package now has an initial private `napi-rs` bridge. The public SDK can opt into it with `driver: "native"` after the native package is built locally. The default SDK path remains the in-memory store until native prebuilds and release packaging are ready.
