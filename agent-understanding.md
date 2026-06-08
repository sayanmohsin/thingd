# Understanding of thingd

> **Tagline**: A fast object-first data engine for applications and AI agents.

## What is thingd?

**thingd** is a high-performance **object-first data engine** built for modern applications and AI agents. It combines persistent storage, in-memory performance, durable queues, event streams, full-text search, and MCP-native access into a single system that can run embedded, standalone, or in the cloud.

At its core, thingd stores versioned JSON objects organized into collections — no complex schemas or heavy abstractions. Around that foundation, it includes durable queues, append-only event streams, full-text search, and AI-native tooling as built-in capabilities rather than external services.

Applications can run thingd entirely in memory for maximum speed, persist data locally, or connect to managed cloud deployments — using the same APIs across all modes. Through built-in MCP support, AI agents can search, retrieve, create objects, and process jobs using standardized tools, making thingd a shared memory layer for both application code and AI systems.

**Positioning**: Object-first data engine → AI-native platform (not a SQLite wrapper or queue system).

### Tech Stack
- **Core**: Rust (`crates/thingd-core`) — traits for ObjectStore, EventLog, QueueStore, Searcher, with `MemoryEngine` and `SqliteThingStore` adapters
- **Storage**: SQLite via `rusqlite`, FTS5 for full-text search with Porter stemming
- **Node SDK**: TypeScript (`packages/thingd`) — public npm package
- **Native binding**: napi-rs (`packages/thingd-native`) — bridges Node.js to Rust SQLite
- **CLI/MCP Server**: TypeScript (`packages/thingd-cli`) — stdio + Streamable HTTP transports, Svelte 5 dashboard
- **Deployment**: Docker, Kubernetes, leader/follower cluster mode

### Core Primitives (implemented)
1. **Object store** — put/get/delete/list JSON records in named collections, auto-versioning
2. **Event log** — append-only, auto-incrementing sequences, searchable
3. **Durable queues** — push/claim/ack/nack with leases, visibility timeouts, retries, delays, dead-letter queues
4. **Full-text search** — FTS5+B,M25 + recency scoring + metadata filtering
5. **MCP tools** — 12 `thing_*` tools for agents, with allowlist/read-only/payload-limit hardening
6. **Cluster bridge** — leader/follower write forwarding via MCP

### Architecture
```
App/AI Agent → ThingD SDK → [InMemory | Native napi | Cloud HTTP MCP]
                                    ↓
                          crates/thingd-core (Rust)
                            ├── MemoryEngine
                            └── SqliteThingStore → SQLite file
```
In sidecar mode, `thingd-cli` runs as an MCP server (stdio or HTTP), and the SDK connects remotely.

### Deployment Modes
- **Embedded** — native napi-rs binding in-process with the Node app
- **Sidecar** — standalone MCP server process, multi-process or multi-language
- **Cluster** — leader/follower with write forwarding, for Kubernetes

---

## What is "vision"?

"Vision" (in `docs/vision.md`) is **not** about computer vision. It is the **product vision and design philosophy** document for thingd. It defines:

### First Principle
> Keep deployment feel close to SQLite, but expose primitives closer to how modern AI-native apps work.

### Core Pillars
1. **Object-shaped storage** — store records without relational schema
2. **MCP-native access** — ship with safe MCP tools so agents can search, read, write
3. **AI-readable by default** — records should be easy to turn into text, summaries, embeddings, links
4. **Events first** — every mutation produces an append-only trail
5. **Durable workflow primitives** — queues, leases, retries as first-class citizens

### Non-Goals
- Not replacing SQLite
- Not pretending local files support multi-primary writes
- Not building a huge distributed DB before local experience is excellent

Vision.md is treated as a **canonical north star** — referenced from README, roadmap, coding standards, and doc-maintenance, and must be updated whenever MCP tool names or major design decisions change.
