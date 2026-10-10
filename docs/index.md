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
    details: 51 Node MCP tools. The Rust sidecar exposes 41 engine tools; scheduler tools are SDK-level.
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

## ThingDB: embedded Rust key-value database

ThingDB is an experimental embedded key-value database written in Rust. The
standalone crate offers ordered byte-key keyspaces, atomic batches, snapshots,
and prefix/range scans. It has process-local RAM storage and a separate
experimental WAL-backed durable format. It is a database engine, not a SQL
server or a semantic object/event/queue API.

<section class="thingdb-lab" aria-labelledby="thingdb-lab-title">
  <div class="thingdb-lab__eyebrow"><span class="thingdb-lab__pulse"></span> EMBEDDED RUST DATABASE · EXPERIMENTAL</div>
  <h2 id="thingdb-lab-title">Meet ThingDB</h2>
  <p class="thingdb-lab__intro">An ordered key-value database for Rust applications: organize byte keys into keyspaces, commit related changes atomically, and read them through deterministic scans or consistent snapshots. Use RAM for disposable state; evaluate the separate durable format with care.</p>
  <div class="thingdb-lab__modes">
    <article class="thingdb-lab__mode thingdb-lab__mode--ram">
      <div class="thingdb-lab__mode-label">01 / VOLATILE</div>
      <h3>RAM database</h3>
      <p>In-process ordered storage with keyspaces, batches, scans, and snapshots. No database files; all data is lost when the process exits.</p>
      <span class="thingdb-lab__status">PROCESS-LOCAL · NON-DURABLE</span>
    </article>
    <article class="thingdb-lab__mode thingdb-lab__mode--durable">
      <div class="thingdb-lab__mode-label">02 / PERSISTENT</div>
      <h3>Durable database</h3>
      <p>Experimental WAL-backed storage with sync-before-ack writes, recovery, and ordered tables. Its format is independent of RocksDB.</p>
      <span class="thingdb-lab__status">EXPERIMENTAL · NOT A ROCKSDB FORMAT</span>
    </article>
  </div>
  <div class="thingdb-lab__foot">
    <span><strong>Standalone:</strong> use the <code>thingdb</code> crate without Thingd.</span>
    <span><strong>Stability:</strong> no durable format stability promise before 1.0.</span>
  </div>
  <div class="thingdb-lab__links">
    <a href="https://crates.io/crates/thingdb">Get the crate →</a>
    <a href="/api-spec/thingdb">Read the database API →</a>
    <a href="/benchmarks">Explore benchmark methodology →</a>
  </div>
</section>

### ThingDB in Thingd

Thingd uses ThingDB RAM for disposable native/server `:memory:`
operation. For durable Thingd storage, RocksDB remains the default; durable
ThingDB is opt-in and experimental. Thingd layers its object, event, queue,
vector, and search semantics above storage engines. The standalone crate's
`MemoryCache` is a separate bounded TTL/LRU cache, not Thingd's full
RAM database.

The [storage backend guide](/storage-backends) explains Thingd runtime modes
and logical repack. The [benchmark guide](/benchmarks) separates raw
key/value comparisons from Thingd semantic workloads. Comparable cross-database
results are pending; current benchmark data is development evidence, not a
portable performance claim.

### ☁️ Need managed hosting?

[**thingd Cloud**](https://thingd.cloud) runs thingd for you — hosted MCP endpoints, API key management, team dashboard, tenant isolation, and backups.

---
