---
layout: home
title: thingd — The Open-Source Engine for Managing Your Things
description: Thingd is the open-source engine for managing your things — an object-first Rust data engine for applications and AI agents, with storage, queues, events, search, links, and MCP access in one binary.
# MCP tool count: update docs/.vitepress/config.ts (mcpToolCount) and packages/thingd/src/constants.ts

hero:
  name: thingd
  text: The open-source engine for managing your things
  tagline: Object-shaped storage, durable queues, event streams, full-text search, vector search, graph links, and MCP-native tools — all in one static binary for applications and AI agents.
  # MCP tool count: update docs/.vitepress/config.ts (mcpToolCount) and packages/thingd/src/constants.ts
  actions:
    - theme: brand
      text: Get started →
      link: /quickstart
    - theme: alt
      text: GitHub
      link: https://github.com/sayanmohsin/thingd
    - theme: alt
      text: Docker
      link: https://hub.docker.com/r/sayanmohsin/thingd

features:
  - icon: 🗄️
    title: Object Store
    details: Versioned JSON records in named collections. No schema. Put, get, delete, list. Auto-versioning.
  - icon: 📋
    title: Durable Queues
    details: At-least-once with leases, retries, delays, idempotency keys, and dead-letter queue.
  - icon: 📡
    title: Event Streams
    details: Append-only timelines with auto-incrementing sequences. Built for audit and replication.
  - icon: 🔍
    title: Full-text Search
    details: Tantivy (pure Rust) full-text indexing with metadata filters and stemming; ranking hardening is ongoing.
  - icon: 🧠
    title: Vector Search
    details: Semantic search via persisted vectors and cosine similarity. HNSW/ANN and hybrid ranking are future scale work.
  - icon: 🤖
    title: MCP-native
    details: 49 Node MCP tools. The Rust sidecar exposes 39 engine tools; scheduler tools are SDK-level.
    # MCP tool count: update docs/.vitepress/config.ts (mcpToolCount) and packages/thingd/src/constants.ts
  - icon: ⚡
    title: Multi-mode
    details: Embedded, sidecar, or cluster. In-memory or durable local engine, with RocksDB by default and experimental ThingDB opt-in. Same API everywhere.
  - icon: 🔁
    title: Provider-neutral replication
    details: Synchronize an explicit Thingd source to a protected replica with cursors, tombstones, snapshots, provenance, and conflict quarantine.
  - icon: 🌐
    title: WASM-ready
    details: Compiles to wasm32 for browser agents and edge workers. Same API, in-memory backend.

---

## Storage and benchmark status

Thingd uses ThingDB RAM for disposable native/server in-memory operation and
RocksDB as the default durable backend. Durable ThingDB is an experimental,
opt-in Rust-native format and does not open RocksDB files directly.

The [storage backend guide](/storage-backends) explains the runtime modes and
safe logical repack path. The [benchmark guide](/benchmarks) documents the
single reproducible harness used to compare ThingDB RAM, the `MemoryEngine`
reference, RocksDB, and durable ThingDB. Results are local development signals,
not production performance claims.

### ☁️ Need managed hosting?

[**thingd Cloud**](https://thingd.cloud) runs thingd for you — hosted MCP endpoints, API key management, team dashboard, tenant isolation, and backups.

---
