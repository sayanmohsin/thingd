# memoryd

Object-shaped local memory for AI-native apps. SQLite-simple, MCP-native, with search, events, and durable queues.

`memoryd` is an experimental Rust-powered local data engine for applications and AI agents. It is designed for developers who like the simplicity of SQLite, but want a higher-level object API, agent-readable memory, built-in workflow primitives, and first-class MCP access.

The short version:

```txt
SQLite-like local deployment
+ Mongo-like object shape
+ AI-readable events and memory
+ full-text and vector-ready search
+ durable queues for workers and agents
+ MCP tools for safe AI access
+ optional sidecar cluster bridge later
```

## Status

`memoryd` is in early design and scaffolding.

The repository currently contains:

- a Rust workspace
- Rust storage boundary traits and an in-memory Rust engine
- a feature-gated SQLite adapter for durable Rust object, event, and queue storage
- a working TypeScript Node.js SDK with an in-memory store
- object, event, search, and queue APIs
- queue semantics for leases, `ack`, `nack`, delayed jobs, and dead-letter jobs
- npm package smoke testing without publishing
- a private native-binding scaffold for future N-API work
- MCP package scaffolding
- architecture, release, persistence, and agent integration docs

It is not production-ready yet. The current public Node.js SDK is useful for API exploration and local integration tests, but it does not persist data across process restarts. The Rust core now has SQLite-backed object, event, and queue persistence behind the `sqlite` feature; the Node native adapter is still next.

## Why memoryd?

SQLite is excellent. It is small, fast, local, durable, and easy to deploy. But AI-native apps often need a friendlier layer above raw SQL.

AI agents and modern app workflows commonly need to:

- store object-shaped records without designing relational tables first
- search memory semantically and by keyword
- keep an append-only history of decisions, events, and changes
- run background jobs for embeddings, summarization, indexing, and tool calls
- coordinate multiple workers or pods with leases and retries
- expose safe read/write tools through MCP
- keep local state portable and inspectable

`memoryd` aims to provide those primitives as a tiny local runtime.

## What memoryd is

`memoryd` is intended to be:

- an open source Apache-2.0 project
- an object-shaped local data layer for apps
- an AI-readable memory store
- a durable queue engine for background jobs
- an event log for timelines and audit trails
- a search layer across text, metadata, and vectors
- an MCP server for controlled agent access
- a Rust core with a friendly TypeScript/Node.js SDK
- a future sidecar/server mode for Kubernetes-style deployments

## What memoryd is not

`memoryd` is not trying to replace mature databases.

It is not:

- a full Postgres replacement
- a magic multi-primary SQLite clone
- a Redis replacement for massive distributed workloads
- a hosted vector database
- a finished production system today

The goal is a practical local-first engine for small and medium apps, agent workflows, devtools, internal tools, edge deployments, and AI memory systems.

## Core primitives

```txt
memoryd
  docs        object-shaped JSON records
  events      append-only timelines and audit trails
  search      full-text, metadata, and vector-ready retrieval
  queue       durable jobs, retries, leases, delays, and DLQ
  graph       links between objects, memories, sources, and decisions
  locks       leases for multi-worker and multi-pod coordination
  mcp         agent-facing tools and resources
```

## Example API

This is the target developer experience.

```ts
import { MemoryD } from "@sayanmohsin/memoryd";

const db = await MemoryD.open("./memoryd.db");

await db.put("decisions", {
  id: "rust-core",
  text: "Use Rust for the core engine and TypeScript for the developer API.",
  project: "memoryd",
  confidence: 0.9,
});

const decision = await db.get("decisions", "rust-core");

await db.events.append("project:memoryd", {
  type: "decision.made",
  text: "memoryd will be object-shaped and MCP-native.",
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
- easy for AI tools to read
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

Every meaningful mutation can produce an event. Events make memory easier for humans and agents to understand.

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
- agent timelines
- rebuilding indexes
- sync and replication
- answering questions like "what changed?" or "why did this happen?"

## Durable queues

`memoryd` includes queue primitives because AI apps constantly need background work:

- chunk a document
- create embeddings
- summarize a thread
- retry a failed tool call
- rebuild a search index
- compact old memory
- run an agent task

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

`memoryd` should make the safe path obvious: jobs may run more than once, so consumers should be idempotent.

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

Early versions can start with full-text and metadata search. Vector search can come after the object and event model is stable.

## MCP-native access

MCP is a core part of the design. The database should ship with an MCP server so agents can read and write through explicit tools instead of guessing internal schemas.

Planned tools:

```txt
memory.search
memory.get
memory.put
memory.patch
memory.delete
memory.link
memory.events.append
memory.events.list
memory.queue.push
memory.queue.claim
memory.queue.ack
memory.queue.nack
memory.queue.dead
```

The MCP layer should enforce:

- allowed collections
- read/write permissions
- tool-level validation
- safe mutation boundaries
- source and actor attribution
- audit events for agent writes

## Sidecar and cluster mode

The long-term deployment model has two simple modes:

```txt
embedded:
  Node app -> native Rust binding -> SQLite file

sidecar:
  Node app -> localhost memoryd sidecar -> SQLite file
```

Cluster mode should be owned by the sidecar, not by app code:

```txt
Pod A memoryd sidecar = leader
Pod B memoryd sidecar = follower, forwards writes
Pod C memoryd sidecar = follower, forwards writes
```

Apps keep using `MemoryD`; deployment decides whether `MemoryD.open()` uses an
embedded store or connects to `MEMORYD_URL`.

See [docs/sidecar-cluster.md](./docs/sidecar-cluster.md) for the planned Node
API, environment variables, Kubernetes shape, and bridge helpers.

## Multi-pod direction

The honest multi-pod stance:

`memoryd` should not pretend local files magically support many pods writing to the same database file.

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
  | @sayanmohsin/memoryd
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
  memoryd-core/       Rust engine primitives

packages/
  memoryd/            Node.js SDK
  memoryd-native/     Planned native Node.js binding package
  memoryd-mcp/        MCP server package

examples/
  node-basic/         Minimal Node.js example
  nestjs-basic/       NestJS API example

docs/
  vision.md
  architecture.md
  agent-implementation-guide.md
  coding-standards.md
  persistence-and-native-bindings.md
  sidecar-cluster.md
  benchmarks.md
  release.md
```

## Examples

- [Node basic](./examples/node-basic) shows the intended SDK shape.
- [NestJS basic](./examples/nestjs-basic) shows how `memoryd` can sit behind a normal NestJS module, service, and controller setup.

## Local testing without npm publish

You can test the Node.js package locally before publishing anything to npm.

From the repository root:

```bash
npm install
npm run build
npm run test:node
npm run test:package
```

`npm run test:package` builds `@sayanmohsin/memoryd`, creates a local npm tarball, installs that tarball into a temporary app, imports the package, and runs a smoke test. This is the closest local check to "will this work after npm publish?" without publishing anything.

The included examples can consume the local package through the workspace/file dependency. For the NestJS example:

```bash
cd examples/nestjs-basic
npm run start:dev
```

For a separate Node.js app outside this repository, install the local package by path:

```bash
npm install /Users/sayan/Documents/Experimental/memoryd/packages/memoryd
```

Or add it to that app's `package.json`:

```json
{
  "dependencies": {
    "@sayanmohsin/memoryd": "file:/Users/sayan/Documents/Experimental/memoryd/packages/memoryd"
  }
}
```

Publishing to npm is only needed once you want other machines or users to install the package normally.

## Tooling and standards

Project conventions live in checked-in files so this private repo stays easy to work on with normal dev tools and AI coding sessions:

- [biome.json](./biome.json) controls TypeScript, JavaScript, and JSON formatting/linting.
- [rustfmt.toml](./rustfmt.toml) controls Rust formatting.
- [Cargo.toml](./Cargo.toml) defines workspace Rust and Clippy lints.
- [docs/agent-implementation-guide.md](./docs/agent-implementation-guide.md) explains how AI agents and contributors should integrate `memoryd` into apps.
- [docs/coding-standards.md](./docs/coding-standards.md) explains the coding standards.
- [docs/persistence-and-native-bindings.md](./docs/persistence-and-native-bindings.md) explains the Rust persistence boundary and native binding direction.
- [docs/sidecar-cluster.md](./docs/sidecar-cluster.md) explains the planned sidecar, Kubernetes, and cluster bridge shape.
- [docs/benchmarks.md](./docs/benchmarks.md) explains local benchmark commands and how to interpret them.
- [docs/release.md](./docs/release.md) explains npm publishing and automatic versioning.

Useful commands:

```bash
npm run check
npm run check:write
npm run test:local
npm run rust:fmt:check
npm run rust:clippy
npm test
```

Rust checks run all crate features, including the SQLite adapter:

```bash
npm run rust:check
npm run bench:rust
npm run bench:rust:smoke
npm run test:rust
```

## Releases

`memoryd` uses semantic-release on `main` for automatic npm versioning and publishing.

Conventional commits map to SemVer like this:

- `fix:` creates a patch release
- `feat:` creates a minor release
- `BREAKING CHANGE:` or `!` creates a major release

The npm package is published from [packages/memoryd](./packages/memoryd). Publishing is skipped until the repository has an `NPM_TOKEN` secret configured.

Before enabling publish, run:

```bash
npm run test:local
npm run release:dry-run
```

## Comparison

| Tool | Great at | Why memoryd is different |
| --- | --- | --- |
| SQLite | local relational storage | object API, MCP tools, events, queues, AI search |
| MongoDB | flexible documents | local-first tiny runtime, Rust core, MCP-native |
| Redis / BullMQ | fast queues and workers | durable local engine without requiring Redis |
| Postgres job queues | reliable jobs on Postgres | lighter local deployment for Node apps |
| LanceDB / vector DBs | vector search | broader memory runtime with events and queues |
| MCP servers | exposing tools to agents | storage engine designed around MCP from the start |

## Development

Start with the local Node/package gate:

```bash
git clone https://github.com/sayanmohsin/memoryd.git
cd memoryd
npm install
npm run test:local
```

If Rust is installed, also run:

```bash
npm run rust:check
npm run rust:fmt:check
npm run rust:clippy
npm run test:rust
```

## Roadmap

### v0.1 - local core

- [x] object put/get/delete in the Node SDK proof store
- [x] append-only event API in the Node SDK proof store
- [x] queue claim/ack/nack in the Node SDK proof store
- [x] delayed jobs and dead-letter queue in the Node SDK proof store
- [x] basic Node.js SDK
- [x] Rust storage boundary traits
- [x] SQLite object/event adapter in the Rust core
- [x] SQLite queue adapter in the Rust core
- [ ] native Node adapter to the Rust store

### v0.2 - agent memory

- [ ] full-text search
- [ ] metadata filters
- [ ] object-to-text indexing
- [ ] MCP server
- [ ] audit events for MCP writes

### v0.3 - production shape

- [ ] migrations
- [ ] worker heartbeats
- [x] idempotency keys in the Node SDK proof store
- [x] delayed jobs in the Node SDK proof store
- [ ] persistent SDK store backed by native Rust
- [ ] inspector UI

### later

- [ ] vector search
- [ ] graph links
- [ ] local read replicas
- [ ] server binary
- [ ] Docker sidecar image
- [ ] Kubernetes sidecar mode
- [ ] cluster bridge with leader write forwarding
- [ ] tenant partitioning
- [ ] sync and compaction

## Design principles

- Keep the local developer experience simple.
- Prefer boring durable storage under the hood.
- Expose object-shaped APIs to apps and agents.
- Make every important mutation explainable through events.
- Treat vector search as one retrieval signal, not the whole memory system.
- Use at-least-once queues and make idempotency easy.
- Be honest about distributed systems tradeoffs.

## License

`memoryd` is open source under the Apache-2.0 license. See [LICENSE](./LICENSE).
