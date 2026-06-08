# thingd

[![npm version (SDK)](https://badge.fury.io/js/thingd.svg)](https://www.npmjs.com/package/thingd) [![npm version (CLI)](https://badge.fury.io/js/thingd-cli.svg)](https://www.npmjs.com/package/thingd-cli)

A fast object-first data engine for applications and AI agents.

🌐 [sayanmohsin.github.io/thingd](https://sayanmohsin.github.io/thingd) — landing page

thingd is a high-performance object-first data engine built for modern applications and AI agents. It combines persistent storage, in-memory performance, durable queues, event streams, full-text search, and MCP-native access into a single system that can run embedded, standalone, or in the cloud.

thingd is a fast, object-first data engine designed for modern software and AI-native applications. It provides a simple way to store, search, process, and share data without stitching together multiple infrastructure components.

At its core, thingd stores versioned JSON objects organized into collections, making it easy to work with application data without complex schemas or heavy abstraction layers. Around that foundation, thingd includes durable queues, append-only event streams, full-text search, and AI-native tooling as built-in capabilities rather than external services.

Applications can run thingd entirely in memory for maximum speed, persist data locally for long-term storage, or connect to managed cloud deployments as they become available. The same APIs work across all deployment modes, allowing applications to move from development to production without architectural changes.

thingd is also designed for AI-native workflows. Through built-in MCP support, agents can search records, retrieve data, create objects, process jobs, and interact with application state using standardized tools instead of custom integrations. This makes thingd a shared memory layer that works equally well for application code and AI systems.

For larger deployments, thingd supports leader/follower architectures that provide a single write authority while scaling reads across replicas. The result is a system that remains simple to deploy locally while growing into production-scale environments when needed.

Instead of combining a database, queue system, search engine, event store, and agent integration layer, thingd brings them together in a single object-first platform built for speed, simplicity, and AI-native development.

## Status

`thingd` is in early design and scaffolding.

The repository currently contains:

- a Rust workspace
- Rust storage boundary traits and an in-memory Rust engine
- a feature-gated SQLite adapter for durable Rust object, event, and queue storage
- a working TypeScript Node.js SDK with an in-memory store
- an opt-in private N-API native driver that opens the Rust SQLite store locally
- a remote Node.js SDK driver that talks to the sidecar over Streamable HTTP MCP
- a feature-rich `thingd` admin/operator CLI featuring a real-time Interactive TUI Dashboard and scriptable non-interactive JSON output
- object, event, search, and queue APIs
- queue semantics for leases, `ack`, `nack`, delayed jobs, retry delays, and dead-letter jobs
- package smoke testing without publishing
- stdio and Streamable HTTP MCP server package with object, event, search, and queue tools
- Docker runtime scaffold for the HTTP MCP server
- bridge-mode env vars with leader/follower MCP forwarding
- SQLite schema version tracking and migration guardrails
- MCP audit events for write tools
- architecture, release, persistence, and integration docs

It is not production-ready yet. The default public Node.js SDK path still uses the TypeScript in-memory store for API exploration and local integration tests. The Rust core has SQLite-backed object, event, and queue persistence behind the `sqlite` feature, and the repo now has an opt-in private native driver for local testing. Node apps can also use the remote driver to talk to a `thingd` sidecar through `THINGD_URL`. Native prebuilds, production packaging, and deployment hardening are still next.

| Entry point | Default driver | Default path |
| --- | --- | --- |
| `ThingD.open()` from npm (today) | memory | n/a |
| `thingd mcp` / `mcp-http` | native (when built) | `~/.thingd/data.db` |
| `THINGD_URL` set | remote | sidecar |

Build order and doc update rules: [docs/roadmap.md](./docs/roadmap.md), [docs/doc-maintenance.md](./docs/doc-maintenance.md).

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
npm install thingd
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
import { ThingD } from "thingd";

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
  driver: "remote",
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

**Current behavior:** Search is powered by a high-performance database-native SQLite **FTS5** virtual table with Porter word stemming, custom metadata key-value filters, and dynamic recency-weighted ranking. Vector search is planned for a future release once this foundation is fully integrated.

## Advanced primitives

Beyond objects, events, and queues, `thingd` should grow workflow primitives
that are valuable for modern applications:

- graph links for source tracing and explainable retrieval
- hybrid search across text, metadata, graph links, recency, and vectors
- locks, leases, and semaphores for worker and pod coordination
- workflow DAGs for document ingestion, indexing, and processing pipelines
- semantic cache for expensive model/tool outputs
- tool-call ledger for replay, audit, latency, and cost
- compaction snapshots for long-running memory

See [docs/ai-primitives.md](./docs/ai-primitives.md) for the priority order,
target APIs, storage shapes, MCP tools, and future phases.

## MCP-native access

MCP is a core part of the design. The database ships with stdio and Streamable HTTP MCP server entrypoints so tools can read and write through explicit operations instead of guessing internal schemas.

Current tools:

```txt
thing_search
thing_get
thing_put
thing_delete
thing_events_append
thing_events_list
thing_queue_push
thing_queue_claim
thing_queue_ack
thing_queue_nack
thing_queue_list
thing_queue_dead
```

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
THINGD_CLUSTER_PEERS=http://thingd-0:8757,http://thingd-1:8757
```

Followers automatically forward MCP write traffic to the configured leader and run a background pull catch-up replication thread to keep their local read replicas in sync.

The MCP layer should continue to enforce:

- allowed collections
- read/write permissions
- tool-level validation
- safe mutation boundaries
- source and actor attribution

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

See [docs/sidecar-cluster.md](./docs/sidecar-cluster.md),
[docs/runtime-env.md](./docs/runtime-env.md), and the [deploy](./deploy)
examples for the current bridge env, Kubernetes shape, and reverse proxy shape.

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
  thingd-core/       Rust engine primitives

packages/
  thingd/            Node.js SDK
  thingd-native/     Private native Node.js binding package
  thingd-cli/        Interactive Dashboard, JSON CLI, & MCP servers

examples/
  node-basic/         Minimal Node.js example
  nestjs-basic/       NestJS API example

docs/
  vision.md
  ai-primitives.md
  architecture.md
  agent-implementation-guide.md
  coding-standards.md
  persistence-and-native-bindings.md
  sidecar-cluster.md
  benchmarks.md
  release.md
```

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
    "thingd": "file:/path/to/thingd/packages/thingd"
  }
}
```

Publishing to npm is only needed once you want other machines or users to install the package normally.

## Tooling and standards

Project conventions live in checked-in files so this private repo stays easy to work on with normal dev tools:

- [biome.json](./biome.json) controls TypeScript, JavaScript, and JSON formatting/linting.
- [rustfmt.toml](./rustfmt.toml) controls Rust formatting.
- [Cargo.toml](./Cargo.toml) defines workspace Rust and Clippy lints.
- [docs/roadmap.md](./docs/roadmap.md) is the canonical build order and phase exit criteria.
- [docs/faq.md](./docs/faq.md) answers hard questions about consistency, durability, performance, and production readiness.
- [docs/doc-maintenance.md](./docs/doc-maintenance.md) lists which docs to update when code changes.
- [docs/why-agents.md](./docs/why-agents.md) explains the agent leverage story.
- [docs/agent-patterns.md](./docs/agent-patterns.md) documents memory, scheduler, and queue patterns.
- [docs/agent-implementation-guide.md](./docs/agent-implementation-guide.md) explains how to integrate `thingd` into projects.
- [docs/ai-primitives.md](./docs/ai-primitives.md) plans graph links, hybrid search, locks, workflow DAGs, semantic cache, tool ledger, and compaction.
- [docs/cli.md](./docs/cli.md) describes the current runtime CLIs and the planned admin/operator CLI phases.
- [docs/coding-standards.md](./docs/coding-standards.md) explains the coding standards.
- [docs/handoff.md](./docs/handoff.md) is the quick restart point for future work.
- [docs/persistence-and-native-bindings.md](./docs/persistence-and-native-bindings.md) explains the Rust persistence boundary and native binding direction.
- [docs/sidecar-cluster.md](./docs/sidecar-cluster.md) explains the planned sidecar, Kubernetes, and cluster bridge shape.
- [docs/benchmarks.md](./docs/benchmarks.md) explains local benchmark commands and how to interpret them.
- [docs/release.md](./docs/release.md) explains npm publishing and automatic versioning.

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

`thingd` uses semantic-release on `main` for automatic npm versioning and publishing.

Conventional commits map to SemVer like this:

- `fix:` creates a patch release
- `feat:` creates a minor release
- `BREAKING CHANGE:` or `!` creates a major release

The npm package is published from [packages/thingd](./packages/thingd). Publishing is skipped until the repository has an `NPM_TOKEN` secret configured.

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

Start with the local Node/package gate:

```bash
git clone https://github.com/sayanmohsin/thingd.git
cd thingd
pnpm install
pnpm test:local
```

If Rust is installed, also run:

```bash
pnpm rust:check
pnpm rust:fmt:check
pnpm rust:clippy
pnpm test:rust
```

## Roadmap

Canonical ordering, exit criteria, and per-phase doc checklists:
**[docs/roadmap.md](./docs/roadmap.md)**. Quick restart:
[docs/handoff.md](./docs/handoff.md).

**Next:** Phase 8 **Sidecar hardening** (leader failover, Docker release).

### v0.1 - local core

- [x] object put/get/delete in the Node SDK proof store
- [x] append-only event API in the Node SDK proof store
- [x] queue claim/ack/nack in the Node SDK proof store
- [x] delayed jobs and dead-letter queue in the Node SDK proof store
- [x] basic Node.js SDK
- [x] Rust storage boundary traits
- [x] SQLite object/event adapter in the Rust core
- [x] SQLite queue adapter in the Rust core
- [x] delayed jobs and configurable lease expiration in the Rust core
- [x] opt-in native Node adapter to the Rust store
- [x] native prebuild/release strategy

### v0.2 - search and searchable memory

- [x] full-text search
- [x] metadata filters
- [x] object-to-text indexing
- [x] stdio MCP server skeleton
- [x] remote HTTP MCP server skeleton
- [x] audit events for MCP writes

### v0.3 - production shape

- [x] SQLite schema migration guardrails
- [ ] worker heartbeats
- [x] idempotency keys in the Node SDK proof store
- [x] delayed jobs in the Node SDK proof store
- [x] persistent SDK store backed by native Rust for local repo testing
- [x] remote SDK driver for sidecar mode over Streamable HTTP MCP
- [x] Docker runtime scaffold
- [x] bridge-mode env vars and follower MCP forwarding
- [x] Docker/Kubernetes/proxy deployment examples
- [x] admin/operator CLI
- [x] inspector UI (Phase 5 complete — `thingd dashboard`)

### CLI phases

- [x] CLI-A: `packages/thingd-cli`, `thingd` binary, Interactive TUI Dashboard, Non-interactive JSON output, object/event/queue inspection. (See [CLI documentation](./packages/thingd-cli/README.md))
- [x] CLI-B: pretty tables, `doctor`, queue stats, benchmark wrappers, clearer runtime errors
- [x] CLI-C: export/import, snapshots, and redaction-friendly handoff flows

### later

- [ ] vector search
- [ ] graph links
- [ ] hybrid search
- [ ] locks, leases, and semaphores
- [ ] workflow DAGs
- [ ] semantic cache
- [ ] tool-call ledger
- [ ] compaction snapshots
- [x] local read replicas
- [ ] server binary
- [ ] published Docker sidecar image
- [x] Kubernetes sidecar mode example
- [x] cluster bridge with leader write forwarding
- [ ] tenant partitioning
- [x] follower replica catch-up
- [x] sync and compaction

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
