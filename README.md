# thingd

[![npm downloads (SDK)](https://img.shields.io/npm/dm/@thingd/sdk?label=SDK&logo=npm&color=ff6a00)](https://www.npmjs.com/package/@thingd/sdk)
[![npm downloads (CLI)](https://img.shields.io/npm/dm/@thingd/cli?label=CLI&logo=npm&color=ff6a00)](https://www.npmjs.com/package/@thingd/cli)
[![Crates.io](https://img.shields.io/crates/v/thingd?label=engine&logo=rust&color=ff6a00)](https://crates.io/crates/thingd)
[![Docker pulls](https://img.shields.io/docker/pulls/sayanmohsin/thingd?label=Docker&logo=docker&color=ff6a00)](https://hub.docker.com/r/sayanmohsin/thingd)
[![GitHub stars](https://img.shields.io/github/stars/sayanmohsin/thingd?label=Stars&logo=github&color=ff6a00)](https://github.com/sayanmohsin/thingd)

A fast object-first data engine for applications and AI agents.

🌐 [sayanmohsin.github.io/thingd](https://sayanmohsin.github.io/thingd) — landing page

thingd is a high-performance object-first data engine built for modern applications and AI agents. It combines persistent storage, durable queues, event streams, full-text search, and MCP-native access into a single system that can run embedded, standalone, or in a cluster.

thingd stores versioned JSON objects in collections, with built-in durable queues, append-only event streams, and full-text search — no stitching together separate infrastructure. The same API works in-memory, persisted locally, or connected to a remote sidecar.

## Status

`thingd` is in early-to-mid stage prototype (0.x track). The core engine,
SDK, CLI, and MCP server are functional and tested, but the project is not
production-ready yet.

### Shipped

- **Rust engine** (`thingd` — crates.io) — memory + SQLite adapters, FTS5 search, queue lifecycle, graph links, SQLite schema migrations
- **Node.js SDK** (`@thingd/sdk`) — three drivers: memory (default in-memory TS store), native (napi-rs Rust SQLite), remote/cloud (Streamable HTTP MCP)
- **CLI** (`@thingd/cli`) — TUI dashboard, 30+ subcommands (search, objects, events, queues, export/import/snapshot, doctor, bench, install for Cursor/Claude Desktop)
- **MCP server** — 27 tools, stdio + Streamable HTTP, audit events, collection allowlists, read-only mode
- **Docker** — multi-stage image, compose + K8s for leader/follower cluster
- **CI/tooling** — semantic-release, biome, lefthook, doc tests

### What's next

- Production packaging and deployment hardening
- Public native driver prebuilds
- Vector search integration
- Priority queues and advanced scheduling

The default public Node.js SDK path uses the TypeScript in-memory store for
API exploration and local integration tests. The Rust core has SQLite-backed
object, event, and queue persistence behind the `sqlite` feature. Node apps can
use the cloud driver to talk to a `thingd` sidecar through `THINGD_URL`.

| Entry point | Default driver | Default path |
| --- | --- | --- |
| `ThingD.open()` from npm (today) | memory | n/a |
| `thingd mcp` / `mcp-http` | native (when built) | `~/.thingd/data.db` |
| `THINGD_URL` set | remote | sidecar |

## Why thingd?

SQLite is excellent. It is small, fast, local, durable, and easy to deploy. But modern apps often need a friendlier layer above raw SQL.

Modern app workflows commonly need to:

- store object-shaped records without designing relational tables first
- search memory semantically and by keyword
- keep an append-only history of decisions, events, and changes
- run background jobs for embeddings, summarization, indexing, and tool calls
- coordinate multiple workers or pods with leases and retries
- expose safe read/write tools through MCP
- keep local state portable and inspectable

`thingd` aims to provide those primitives as a tiny local runtime.

## What thingd is

`thingd` is intended to be:

- an open source Apache-2.0 project
- an object-shaped local data layer for apps
- a searchable memory store
- a durable queue engine for background jobs
- an event log for timelines and audit trails
- a search layer across text, metadata, and vectors
- an MCP server for controlled programmatic access
- a Rust core with a friendly TypeScript/Node.js SDK
- a sidecar/server runtime shape for Kubernetes-style deployments

## What thingd is not

`thingd` is not trying to replace mature databases.

It is not:

- a full Postgres replacement
- a magic multi-primary SQLite clone
- a Redis replacement for massive distributed workloads
- a hosted vector database
- a finished production system today

The goal is a practical local-first engine for small and medium apps, devtools, internal tools, edge deployments, and persistent memory systems.

## Core primitives

```txt
thingd
  docs        object-shaped JSON records
  events      append-only timelines and audit trails
  search      full-text, metadata, and vector-ready retrieval
  queue       durable jobs, retries, leases, delays, and DLQ
  graph       links between objects, memories, sources, and decisions
  locks       leases for multi-worker and multi-pod coordination
  workflow    DAGs for multi-step background jobs and pipelines
  cache       semantic cache for model/tool outputs
  ledger      tool-call history, latency, cost, and replay data
  snapshots   compaction summaries linked to raw events
  mcp         programmatic tools and resources
```

## Installation

### npm (SDK)

```bash
npm install @thingd/sdk
```

### Rust (crate)

```toml
[dependencies]
thingd = { version = "0.34", features = ["sqlite"] }
```

### Subpath imports

```typescript
// Full SDK (Node.js: MCP + REST + stores + native binding)
import { ThingD } from "@thingd/sdk";

// Lightweight HTTP client (browser + Node.js, zero dependencies)
import { ThingD } from "@thingd/sdk/client";

// Pure in-memory store (browser + Node.js, zero dependencies)
import { ThingD } from "@thingd/sdk/memory";

// Types only (for type-safe dependency injection)
import type { ThingDConnection } from "@thingd/sdk/types";
```

### Docker (sidecar runtime)

```bash
docker pull sayanmohsin/thingd
docker run -p 8757:8757 sayanmohsin/thingd
```

See the [Docker Hub](https://hub.docker.com/r/sayanmohsin/thingd) page for all tags and [deploy/docker-compose.yml](./deploy/docker-compose.yml) for production configuration.

## Example API

This is the target developer experience.

```ts
import { ThingD } from "@thingd/sdk";

const db = await ThingD.open(":memory:");

await db.put("decisions", {
  id: "rust-core",
  text: "Use Rust for the core engine and TypeScript for the developer API.",
  project: "thingd",
  confidence: 0.9,
});

const decision = await db.get("decisions", "rust-core");

await db.events.append("project:thingd", {
  type: "decision.made",
  text: "thingd will be object-shaped and MCP-native.",
  actor: "sayan",
});

await db.queue("embed").push({
  object: "decisions/rust-core",
});

const hits = await db.search("why did we choose rust?", {
  collections: ["decisions"],
  limit: 5,
});
```

For the local Rust-backed SQLite path, build the private native package and
request the native driver:

```bash
pnpm --filter thingd-native build
```

```ts
const db = await ThingD.open({
  path: "./thingd.db",
  driver: "native",
});
```

For sidecar mode, point the SDK at the HTTP MCP runtime:

```bash
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=change-me
```

```ts
const db = await ThingD.open();
```

Or configure it explicitly:

```ts
const db = await ThingD.open({
  url: "http://127.0.0.1:8757/mcp",
  driver: "cloud",
  authToken: "change-me",
});
```

## Object storage

Objects are stored as JSON-like records grouped by collection.

```ts
await db.put("customers", {
  id: "cus_123",
  name: "Acme",
  plan: "pro",
  metadata: {
    region: "us-east",
    owner: "sales",
  },
});
```

Objects should be:

- easy for app code to mutate
- easy for tools and scripts to read
- indexable by metadata
- convertible into searchable text
- linkable to events, jobs, and other objects

The intended internal shape is:

```txt
object id
collection
JSON body
text representation
metadata
timestamps
source
links
version
```

## Events and timelines

Every meaningful mutation can produce an event. Events make memory easier to understand and audit.

```ts
await db.events.append("customer:cus_123", {
  type: "plan.changed",
  text: "Customer upgraded from starter to pro.",
  from: "starter",
  to: "pro",
});
```

Events are useful for:

- audit trails
- activity timelines
- rebuilding indexes
- sync and replication
- answering questions like "what changed?" or "why did this happen?"

## Querying objects

List objects with filtering, sorting, limit, and offset:

```ts
// Filter by field value
const active = await db.listObjects("tasks", { filter: { status: "active" } });

// Sort by field
const sorted = await db.listObjects("tasks", { sortBy: { field: "created_at", direction: "desc" } });

// Paginate
const page1 = await db.listObjects("tasks", { limit: 10, offset: 0 });
const page2 = await db.listObjects("tasks", { limit: 10, offset: 10 });
```

## Batch operations

Bulk create or delete objects in a single call:

```ts
// Batch create
const results = await db.putBatch("tasks", [
  { id: "task-1", title: "Implement search" },
  { id: "task-2", title: "Add graph links" },
  { id: "task-3", title: "Write docs" },
]);

// Batch delete
const deleted = await db.deleteBatch("tasks", ["task-1", "task-2"]);
```

## Graph links

Create directed relationships between any two references:

```ts
// Create a link
const link = await db.links.create("users/alice", "authored", "docs/readme");

// Query neighbors
const outgoing = await db.links.neighbors("users/alice", "Outgoing");
const incoming = await db.links.neighbors("docs/readme", "Incoming");

// Filter by link type
const authored = await db.links.neighbors("users/alice", "Outgoing", { linkType: "authored" });

// Count all links
const count = await db.countLinks();
```

## Durable queues

`thingd` includes queue primitives because apps constantly need background work:

- chunk a document
- create embeddings
- summarize a thread
- retry a failed tool call
- rebuild a search index
- compact old memory
- run a background task

Target API:

```ts
const queue = db.queue("embed");

await queue.push(
  { object: "docs/doc_123" },
  {
    idempotencyKey: "embed:docs/doc_123:v1",
    maxAttempts: 5,
    delayMs: 0,
  }
);

const job = await queue.claim({
  leaseMs: 30_000,
});

if (job) {
  try {
    await embedDocument(job.payload.object);
    await queue.ack(job.id);
  } catch (error) {
    await queue.nack(job.id, {
      delayMs: 5_000,
      error: error instanceof Error ? error.message : String(error),
    });
  }
}

const deadLetters = await queue.dead();
```

Queue semantics:

- at-least-once delivery
- leases / visibility timeouts
- explicit `ack` and `nack`
- retries with backoff
- dead-letter queue
- delayed jobs
- idempotency keys
- worker heartbeats
- priority queues later

`thingd` should make the safe path obvious: jobs may run more than once, so consumers should be idempotent.

## Search

Search should be hybrid by design.

```txt
keyword / full-text search
+ metadata filters
+ recency scoring
+ graph links
+ vector similarity
+ permission filters
```

Target API:

```ts
const hits = await db.search("customers who upgraded after a failed deployment", {
  collections: ["customers", "events"],
  filter: {
    plan: "pro",
  },
  limit: 10,
});
```

**Current behavior:** Search is powered by a high-performance database-native SQLite **FTS5** virtual table with Porter word stemming, custom metadata key-value filters, and dynamic recency-weighted ranking.

## MCP-native access

MCP is a core part of the design. The database ships with stdio and Streamable HTTP MCP server entrypoints so tools can read and write through explicit operations instead of guessing internal schemas.

Current tools: see the [MCP tools reference](docs/api-spec/mcp-tools.md) for all 27 tools with schemas and examples.

Run the automatic zero-config setup for Claude Desktop and Cursor:

```bash
# Installs/updates Claude Desktop config automatically and prints Cursor configuration
thingd install
```

Or run the stdio MCP server manually (which automatically persists to `~/.thingd/data.db` by default):

```bash
thingd mcp --driver native
```

To bridge Claude to a remote `thingd` cluster:

```bash
thingd mcp --url https://your-thingd.com/mcp --auth-token your-secret
```

Run the HTTP runtime:

```bash
pnpm build
THINGD_AUTH_TOKEN=change-me pnpm serve:mcp
```



Build the Docker runtime:

```bash
docker build -t thingd:local .
```

See [docs/mcp-server.md](./docs/mcp-server.md) and [docs/docker-runtime.md](./docs/docker-runtime.md) for the current MCP boundary and runtime details.

Smoke-test the Docker runtime:

```bash
pnpm smoke:docker
```

The MCP layer now appends audit events for write tools to
`__thingd:mcp:audit`. Tool callers can pass optional `actor` and `source`
fields, and runtime defaults can be set with:

```txt
THINGD_MCP_AUDIT=true
THINGD_MCP_ACTOR=mcp-client
THINGD_MCP_SOURCE=thingd-mcp
THINGD_MCP_AUDIT_STREAM=__thingd:mcp:audit
```

The HTTP runtime refuses to bind to non-loopback hosts without
`THINGD_AUTH_TOKEN`, unless `THINGD_ALLOW_UNAUTHENTICATED=true` is set for a
local experiment.

Bridge mode is env-driven:

```txt
THINGD_CLUSTER_MODE=single|leader|follower
THINGD_CLUSTER_LEADER_URL=http://thingd-leader:8757
THINGD_CLUSTER_FORWARD_AUTH_TOKEN=change-me
THINGD_CLUSTER_DISCOVERY=none|static|kubernetes
THINGD_CLUSTER_PEERS=http://thingd-0:8757,http://thingd-1:8757
THINGD_CLUSTER_LEADER_ELECTION=false
THINGD_CLUSTER_LEADER_ELECTION_MAX_FAILURES=3
```

Followers automatically forward MCP write traffic to the configured leader and run a background pull catch-up replication thread to keep their local read replicas in sync. With `THINGD_CLUSTER_LEADER_ELECTION=true`, followers auto-promote the next peer in the ordered peer list to leader when the current leader becomes unreachable for `THINGD_CLUSTER_LEADER_ELECTION_MAX_FAILURES` consecutive replication cycles.

The MCP layer should continue to enforce:

- allowed collections
- read/write permissions
- tool-level validation
- safe mutation boundaries
- source and actor attribution

## REST API

thingd also exposes a REST API on port 4100 under the `/v1` prefix, for apps that prefer HTTP over MCP:

```bash
# health check
curl http://localhost:4100/v1/health

# put an object
curl -X PUT http://localhost:4100/v1/objects/users/user-001 \
  -H "Content-Type: application/json" \
  -d '{"name": "Alice", "role": "admin"}'

# search
curl -X POST http://localhost:4100/v1/search \
  -H "Content-Type: application/json" \
  -d '{"query": "alice"}'

# list objects with filter and sort
curl "http://localhost:4100/v1/objects?collection=users&filter.role=admin&sortBy=created_at&sortDir=desc"
```

Full REST reference: [docs/api-spec/rest-api.md](./docs/api-spec/rest-api.md)

## Sidecar and cluster mode

The long-term deployment model has two simple modes:

```txt
embedded:
  Node app -> native Rust binding -> SQLite file

sidecar:
  Node app -> localhost thingd sidecar -> SQLite file
```

Cluster mode should be owned by the sidecar, not by app code:

```txt
Pod A thingd sidecar = leader
Pod B thingd sidecar = follower, forwards writes
Pod C thingd sidecar = follower, forwards writes
```

Apps keep using `ThingD`; deployment decides whether `ThingD.open()` uses an
embedded store or connects to `THINGD_URL`.

```ts
const db = await ThingD.open();
```

With `THINGD_URL` set, this uses the remote SDK driver and talks to the local
sidecar over Streamable HTTP MCP.

See [docs/mcp-server.md](./docs/mcp-server.md#bridge-mode) for the bridge
environment, [docs/runtime-env.md](./docs/runtime-env.md) for all env vars, and
the [deploy](./deploy) examples for Kubernetes and proxy shapes.

## Multi-pod direction

The honest multi-pod stance:

`thingd` should not pretend local files magically support many pods writing to the same database file.

The practical path is:

```txt
v1: embedded local mode
v2: sidecar/server mode with one primary writer
v3: many Node consumers using leases
v4: local read replicas
v5: tenant or queue partitioning
v6: consensus only if demand proves it is worth the complexity
```

For queues, multi-pod coordination is handled with leases:

```txt
ready job -> claimed by one worker -> ack
ready job -> claimed by one worker -> worker dies -> lease expires -> retry
ready job -> too many failures -> dead-letter queue
```

For local object memory, the first distributed design should be primary-writer plus read replicas, not multi-primary writes.

## Architecture

```txt
Node.js app
  |
  | thingd
  v
Rust core
  |-- object store
  |-- event log
  |-- queue engine
  |-- search indexes
  |-- storage adapters
      |-- in-memory engine
      |-- SQLite objects/events/queues adapter
  |
  +-- MCP server
```

Planned package layout:

```txt
crates/
  thingd/            Rust engine primitives

packages/
  thingd/            Node.js SDK
  thingd-native/     Private native Node.js binding package
  thingd-cli/        Interactive Dashboard, JSON CLI, & MCP servers

examples/
  node-basic/         Minimal Node.js example
  nestjs-basic/       NestJS API example
```

Full documentation: [docs/](./docs/)

## Examples

- [cursor-agent-memory](./examples/cursor-agent-memory) — 5-minute quickstart, `.cursorrules`, scheduler heartbeat, and MCP registration for Cursor and Claude Desktop.
- [Node basic](./examples/node-basic) shows the intended SDK shape.
- [NestJS basic](./examples/nestjs-basic) shows how `thingd` can sit behind a normal NestJS module, service, and controller setup.

Quickstart guide: **[docs/QUICKSTART.md](./docs/QUICKSTART.md)**

## Local testing without npm publish

You can test the Node.js package locally before publishing anything to npm.

From the repository root:

```bash
pnpm install
pnpm build
pnpm test:node
pnpm test:package
```

`pnpm test:package` builds `thingd`, creates a local package tarball, installs that tarball into a temporary app, imports the package, and runs a smoke test. This is the closest local check to "will this work after npm publish?" without publishing anything.

The included examples can consume the local package through the workspace/file dependency. For the NestJS example:

```bash
cd examples/nestjs-basic
pnpm start:dev
```

For a separate Node.js app outside this repository, install the local package by path:

```bash
pnpm add /path/to/thingd/packages/thingd
```

Or add it to that app's `package.json`:

```json
{
  "dependencies": {
    "@thingd/sdk": "file:/path/to/thingd/packages/thingd"
  }
}
```

Publishing to npm is only needed once you want other machines or users to install the package normally.

## Tooling and standards

Project conventions live in checked-in files so this repo stays easy to work on:

- [biome.json](./biome.json) controls TypeScript, JavaScript, and JSON formatting/linting.
- [rustfmt.toml](./rustfmt.toml) controls Rust formatting.
- [Cargo.toml](./Cargo.toml) defines workspace Rust and Clippy lints.

Documentation: see [docs/](./docs/) for quickstart, MCP server reference, API spec, agent setup, patterns, FAQ, and architecture.

Useful commands:

```bash
pnpm check
pnpm check:write
pnpm test:local
pnpm test:cli
pnpm rust:fmt:check
pnpm rust:clippy
pnpm test
```

Rust checks run all crate features, including the SQLite adapter:

```bash
pnpm rust:check
pnpm bench:rust
pnpm bench:rust:smoke
pnpm test:rust
```

## Releases

`thingd` uses [semantic-release](https://semantic-release.gitbook.io) on `main`
with conventional commits to determine version bumps, publish npm packages, and
create GitHub releases.

See [release.md](docs/release.md) for the full release process, required secrets,
native prebuild workflow, and troubleshooting.

Before enabling publish, run:

```bash
pnpm test:local
pnpm release:dry-run
```

## Comparison

| Tool | Great at | Why thingd is different |
| --- | --- | --- |
| SQLite | local relational storage | object API, MCP tools, events, queues, full-text search |
| MongoDB | flexible documents | local-first tiny runtime, Rust core, MCP-native |
| Redis / BullMQ | fast queues and workers | durable local engine without requiring Redis |
| Postgres job queues | reliable jobs on Postgres | lighter local deployment for Node apps |
| LanceDB / vector DBs | vector search | broader memory runtime with events and queues |
| MCP servers | exposing tools to clients | storage engine designed around MCP from the start |

## Development

Development planning and phase tracking is maintained in the private
**thingd-cloud** repo.

Start with the local Node/package gate:

```bash
git clone https://github.com/sayanmohsin/thingd.git
cd thingd
pnpm install
pnpm test:local
```

## Design principles

- Keep the local developer experience simple.
- Prefer boring durable storage under the hood.
- Expose object-shaped APIs to apps and services.
- Make every important mutation explainable through events.
- Treat vector search as one retrieval signal, not the whole memory system.
- Use at-least-once queues and make idempotency easy.
- Be honest about distributed systems tradeoffs.

## License

`thingd` is open source under the Apache-2.0 license. See [LICENSE](./LICENSE).
