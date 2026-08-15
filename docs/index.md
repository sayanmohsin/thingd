---
layout: home
title: thingd — Open-Source Rust Data Engine for AI Agents
description: thingd is an open-source Rust data engine for AI agents. Object-shaped storage, durable queues, event streams, full-text search, vector search, graph links, and 49 Node MCP tools — all in one binary. Built by Sayan Mohsin.
# MCP tool count: update docs/.vitepress/config.ts (mcpToolCount) and packages/thingd/src/constants.ts

hero:
  name: thingd
  text: Open-source Rust data engine for AI agents
  tagline: Object-shaped storage, durable queues, event streams, full-text search, vector search, graph links, and 49 Node MCP-native tools — all in one static binary. No stitching together separate infrastructure.
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

### ☁️ Need managed hosting?

[**thingd Cloud**](https://thingd.cloud) runs thingd for you — hosted MCP endpoints, API key management, team dashboard, tenant isolation, and backups.

---
