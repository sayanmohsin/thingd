# thingd

[![npm downloads (SDK)](https://img.shields.io/npm/dm/@thingd/sdk?label=SDK&logo=npm&color=ff6a00)](https://www.npmjs.com/package/@thingd/sdk)
[![npm downloads (CLI)](https://img.shields.io/npm/dm/@thingd/cli?label=CLI&logo=npm&color=ff6a00)](https://www.npmjs.com/package/@thingd/cli)
[![Crates.io](https://img.shields.io/crates/v/thingd?label=engine&logo=rust&color=ff6a00)](https://crates.io/crates/thingd)
[![Docker pulls](https://img.shields.io/docker/pulls/sayanmohsin/thingd?label=Docker&logo=docker&color=ff6a00)](https://hub.docker.com/r/sayanmohsin/thingd)
[![GitHub stars](https://img.shields.io/github/stars/sayanmohsin/thingd?label=Stars&logo=github&color=ff6a00)](https://github.com/sayanmohsin/thingd)

A fast object-first data engine for applications and AI agents.

🌐 [sayanmohsin.github.io/thingd](https://sayanmohsin.github.io/thingd) — landing page
☁️ [thingd.cloud](https://thingd.cloud) — managed cloud

thingd is a high-performance object-first data engine built for modern applications and AI agents. It combines persistent storage, durable queues, event streams, full-text search, and MCP-native access into a single system that can run embedded, standalone, or in a cluster.

thingd stores versioned JSON objects in collections, with built-in durable queues, append-only event streams, and full-text search — no stitching together separate infrastructure. The same API works in-memory, persisted locally, or connected to a remote sidecar.

## Status

`thingd` is in early-to-mid stage prototype (0.x track). The core engine,
SDK, CLI, and MCP server are functional and tested, but the project is not
production-ready yet.

See the [public documentation](./docs/) for the engine feature set. Cloud-only planning and roadmap material is maintained privately in the thingd-cloud repository.

### Shipped

- **Rust engine** (`thingd` — crates.io) — memory + Fjall adapters, Tantivy FTS, embedvec vector search, queue lifecycle, graph links, aggregate analytics, NLQ
- **Node.js SDK** (`@thingd/sdk`) — three drivers: memory (default in-memory TS store), native (napi-rs Rust SQLite), cloud (remote HTTP REST)
- **Browser/Edge client** (`@thingd/client`) — zero-dependency REST client for browsers, Cloudflare Workers, AWS Lambda, Bun, Deno
- **CLI** (`@thingd/cli`) — TUI dashboard, 30+ subcommands (search, objects, events, queues, export/import/snapshot/backup, doctor, bench, db maintenance). Support for importing from Postgres/MySQL via sidecar REST.
- **MCP server** — 46 SDK tools, stdio + Streamable HTTP, audit events, collection allowlists, and read-only mode. The Rust sidecar currently exposes 36 core tools; scheduler tools are SDK-only.
- **Docker** — multi-stage image, compose + K8s for leader/follower cluster
- **CI/tooling** — semantic-release, biome, lefthook, doc tests, cargo audit, cargo deny, CodeQL

### What's next

- Broader production hardening and operational tooling
- More deployment and connector integrations
- Additional SDK and MCP ergonomics based on user feedback

The default public Node.js SDK path uses the TypeScript in-memory store for
API exploration and local integration tests. The Rust core has Fjall-backed
object, event, and queue persistence (pure Rust LSM-tree, 100K+ ops/s). Node apps can
use the cloud driver to talk to a `thingd` sidecar through `THINGD_URL`.

For browsers, edge runtimes, and non-Node.js environments, use the standalone
`@thingd/client` package — a zero-dependency REST client.

| Entry point | Driver | Protocol |
| --- | --- | --- |
| `ThingD.open()` (Node.js) | memory / native / cloud | In-process / napi / HTTP REST |
| `@thingd/client` (browser/edge) | cloud | HTTP REST |
| `thingd mcp` / `mcp-http` | native | stdio MCP / Streamable HTTP |

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

See [docs/why-thingd.md](./docs/why-thingd.md) for the full rationale, use
cases, and when to skip.

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

## thingd Cloud

[thingd.cloud](https://thingd.cloud) is the managed hosted version of
thingd. Create projects, issue API keys, and connect agents to hosted thingd
instances through an HTTPS MCP endpoint with audit logging, scoped access
control, and tenant isolation. Same engine, zero infrastructure to run.

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
thingd = { version = "0.71", features = ["fjall", "search"] }
```

### Subpath imports

```typescript
// Full SDK (Node.js: MCP + REST + stores + native binding)
import { ThingD } from "@thingd/sdk";

// HTTP REST client (Node.js, Bun, Deno)
import { HttpThingStore, openThingD } from "@thingd/sdk/client";

// Pure in-memory store (browser + Node.js, zero dependencies)
import { InMemoryThingStore, openMemoryThingD } from "@thingd/sdk/memory";

// Types only (for type-safe dependency injection)
import type { ThingDConnection } from "@thingd/sdk/types";

// Zero-dependency client for browser/edge (npm install @thingd/client)
import { ThingdClient } from "@thingd/client";
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

For sidecar mode, point the SDK at the HTTP REST endpoint:

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
  url: "http://127.0.0.1:8757",
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

**Current behavior:** Search is powered by Tantivy — a pure Rust full-text search engine with BM25 ranking, custom metadata filters, and dynamic recency-weighted ranking.

## MCP-native access

thingd ships with 46 built-in SDK MCP tools (search,
objects, events, queues, links, aggregate, schema, NLQ, vector). Every primitive is accessible through stdio
or Streamable HTTP — see the [MCP tools reference](docs/api-spec/mcp-tools.md)
for all tools with schemas and examples.

```bash
# Auto-configure Claude Desktop / Cursor for local sidecar
thingd install

# Connect to a remote thingd instance
thingd mcp --driver native

# Connect to thingd Cloud
thingd mcp connect
```

MCP is one access layer — not the product. You can use thingd entirely through
the [Node.js SDK](#node-js-sdk), [CLI](#cli), or [REST API](#rest-api) without
ever touching MCP.

See [docs/mcp-server.md](./docs/mcp-server.md) for hardening, env vars, cluster
bridge, and the full MCP reference.

## REST API

thingd also exposes a REST API under the `/v1` prefix on the same port, for apps that prefer HTTP over MCP:

```bash
# health check
curl http://localhost:8757/v1/health

# put an object
curl -X PUT http://localhost:8757/v1/objects/users/user-001 \
  -H "Content-Type: application/json" \
  -d '{"name": "Alice", "role": "admin"}'

# search
curl -X POST http://localhost:8757/v1/search \
  -H "Content-Type: application/json" \
  -d '{"query": "alice"}'

# list objects with filter and sort
curl "http://localhost:8757/v1/objects?collection=users&filter.role=admin&sortBy=created_at&sortDir=desc"
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
sidecar over HTTP REST.

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
Node.js / Browser / Edge app
  |
  | @thingd/sdk  |  @thingd/client  |  thingd mcp
  | (Node SDK)   |  (HTTP REST)     |  (MCP tools)
  v
thingd-server (Rust sidecar) or thingd.cloud
  |
  | HTTP REST (/v1/*)  |  MCP (/mcp)
  v
Rust core (crates/thingd)
  |-- object store
  |-- event log
  |-- queue engine
  |-- search indexes (Tantivy FTS + embedvec vector)
  |-- storage adapters
      |-- MemoryEngine (cache, WASM)
      |-- FjallEngine (persistent LSM-tree)
```

Package layout:

```txt
crates/
  thingd/            Rust engine primitives
  thingd-server/     Rust sidecar binary (REST + MCP + cluster)

packages/
  thingd/            Node.js SDK (@thingd/sdk)
  thingd-client/     Zero-dep REST client (@thingd/client)
  thingd-native/     Native Node.js binding (napi-rs to Rust engine)
  thingd-cli/        Interactive Dashboard, JSON CLI, & MCP servers

examples/
  node-basic/         Minimal Node.js example
  nestjs-basic/       NestJS API example
  bun-hono/           Bun + Hono + HTTP REST example
```

Full documentation: [docs/](./docs/)

## Examples

- [cursor-agent-memory](./examples/cursor-agent-memory) — 5-minute quickstart, `.cursorrules`, scheduler heartbeat, and MCP registration for Cursor and Claude Desktop.
- [Node basic](./examples/node-basic) shows the intended SDK shape.
- [NestJS basic](./examples/nestjs-basic) shows how `thingd` can sit behind a normal NestJS module, service, and controller setup.

## Use Cases

- [Clipboard History](./docs/use-cases/clipbuf.md) — Search your clipboard through MCP tools, backed by thingd objects and full-text search.
- [Screenshot OCR Pipeline](./docs/use-cases/dartcam.md) — Queue-based OCR pipeline with searchable text extraction.
- [Desktop Agent](./docs/use-cases/desktop-agent.md) — Frontend-less task manager with queue-based reminders.
- [Cross-Device Sync](./docs/use-cases/cloud-backup.md) — Sync bookmarks and data across devices using thingd.cloud REST API.

Quickstart guide: **[docs/quickstart.md](./docs/quickstart.md)**

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
| SQLite | local relational storage | object API, MCP tools, events, queues, FTS + vector search, graph |
| MongoDB | flexible documents | local-first tiny binary, Rust core, MCP-native, built-in queues |
| Redis / BullMQ | fast queues and workers | durable local engine with persist, same API for queues + storage |
| Postgres job queues | reliable jobs on Postgres | lighter local deployment, no connection pool needed |
| LanceDB / vector DBs | vector search | broader engine: vectors + objects + queues + events + graph |
| MCP servers | exposing tools to clients | storage engine designed around MCP from the start |
| RedDB | multi-model database | MIT/Apache open source, pure Rust, pluggable backends |

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

## Author

Built by [Sayan Mohsin](https://sayanmohsin.com) in Toronto, Canada.
- [GitHub](https://github.com/sayanmohsin)
- [Portfolio](https://sayanmohsin.com)
- [thingd Cloud](https://thingd.cloud)
