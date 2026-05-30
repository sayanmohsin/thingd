# AI-Native Primitives Plan

This document plans the higher-value data primitives that should come after the
core object/event/queue engine. These are not generic textbook structures. They
are workflow primitives for AI-native apps, agents, and tools.

The goal is to make `thingd` feel like a tiny local runtime for memory,
retrieval, background work, provenance, and coordination.

## Priority Order

1. graph links
2. hybrid search
3. locks, leases, and semaphores
4. workflow DAGs
5. semantic cache
6. tool-call ledger
7. compaction and snapshots

## Design Rules

- Keep objects, events, and queues as the foundation.
- Prefer primitives that agents can inspect and explain.
- Make every mutating primitive append events where practical.
- Keep public APIs object-shaped and TypeScript-friendly.
- Avoid exposing low-value textbook structures as product features.
- Back first implementations with SQLite tables and Rust traits.
- Surface useful operations through Node SDK, CLI, and MCP tools.

## Graph Links

Graph links connect stored memory into explainable context.

Useful relationships:

```txt
document -> chunk
chunk -> source
decision -> supporting document
agent run -> tool call
customer -> conversation
queue job -> object
summary -> raw events
```

Target API:

```ts
await db.links.create("docs/doc_1", "supports", "decisions/dec_1", {
  weight: 0.9,
  source: "agent",
});

const neighbors = await db.links.neighbors("decisions/dec_1", {
  direction: "both",
  type: "supports",
});
```

MCP tools:

```txt
memory.links.create
memory.links.neighbors
memory.links.delete
memory.links.path
```

SQLite shape:

```txt
links
  id
  from_ref
  type
  to_ref
  weight
  metadata_json
  created_at
```

Value:

- source tracing
- "why did this answer happen?"
- richer retrieval context
- agent-readable provenance

## Hybrid Search

Hybrid search should combine keyword, metadata, graph, recency, and vector
signals.

Target API:

```ts
const hits = await db.search("why did we choose sqlite?", {
  collections: ["decisions", "docs"],
  mode: "hybrid",
  filter: {
    project: "thingd",
  },
  include: {
    neighbors: true,
    events: true,
  },
});
```

Signals:

```txt
BM25 / full-text
metadata filters
recency
graph distance
vector similarity
permissions
```

SQLite shape:

```txt
search_documents
  ref
  collection
  text
  metadata_json
  updated_at

fts tables
vector tables or extension-backed indexes later
```

Value:

- much better retrieval than vectors alone
- explainable results
- local-first search without hosted infra

## Locks, Leases, And Semaphores

Locks coordinate workers, pods, and agents.

Target API:

```ts
const lock = await db.locks.acquire("embed:docs/doc_1", {
  ttlMs: 30_000,
  owner: "worker-1",
});

await lock.renew();
await lock.release();
```

Semaphore API:

```ts
const permit = await db.semaphores.acquire("openai:embeddings", {
  max: 5,
  ttlMs: 60_000,
});
```

SQLite shape:

```txt
leases
  key
  owner
  kind
  expires_at
  metadata_json
  created_at
  updated_at
```

Value:

- prevent duplicate embedding work
- bound LLM/tool concurrency
- coordinate sidecar workers
- power queue claim heartbeats

## Workflow DAGs

Queues handle individual jobs. Workflow DAGs handle dependent jobs.

Example:

```txt
ingest document
  -> chunk document
  -> embed chunks
  -> summarize chunks
  -> index document
  -> notify agent
```

Target API:

```ts
const workflow = await db.workflows.create("ingest-doc", {
  input: {
    document: "docs/doc_1",
  },
});

await workflow.step("chunk");
await workflow.step("embed", { after: ["chunk"] });
await workflow.step("summarize", { after: ["embed"] });
await workflow.start();
```

SQLite shape:

```txt
workflows
workflow_steps
workflow_edges
workflow_runs
workflow_step_runs
```

Value:

- AI ingestion pipelines
- repeatable agent workflows
- resumable multi-step background work
- inspectable progress and failure state

## Semantic Cache

Semantic cache stores expensive model/tool outputs keyed by input, model, and
options.

Target API:

```ts
const cached = await db.cache.get("embedding", {
  inputHash,
  model: "text-embedding-3-large",
});

await db.cache.set("embedding", {
  inputHash,
  model: "text-embedding-3-large",
  value: embedding,
  ttlMs: 7 * 24 * 60 * 60 * 1000,
});
```

SQLite shape:

```txt
cache_entries
  namespace
  key
  value_json
  expires_at
  created_at
  accessed_at
```

Value:

- lower AI cost
- faster local iteration
- reproducible outputs for tests
- avoids duplicate embedding and summarization work

## Tool-Call Ledger

The ledger stores agent/tool activity as structured, queryable history.

Target API:

```ts
await db.tools.record({
  runId: "run_123",
  tool: "thing_search",
  input,
  output,
  status: "ok",
  latencyMs: 42,
  costUsd: 0.0002,
});
```

SQLite shape:

```txt
tool_calls
  id
  run_id
  tool
  input_json
  output_json
  status
  error
  latency_ms
  cost_usd
  created_at
```

Value:

- auditability
- replay
- debugging agent behavior
- evaluation datasets
- cost and latency analysis

## Compaction And Snapshots

Compaction turns verbose event streams into durable summaries.

Target API:

```ts
await db.memory.compact("thread:123", {
  strategy: "summary",
  keepLast: 50,
});
```

SQLite shape:

```txt
snapshots
  ref
  source_stream
  sequence_from
  sequence_to
  summary_json
  created_at
```

Value:

- smaller agent contexts
- long-running memory
- faster retrieval
- explainable summaries linked to raw source events

## MCP Surface

Each primitive should have a narrow MCP surface:

```txt
memory.links.*
thing_search
memory.locks.*
memory.workflows.*
memory.cache.*
memory.tools.*
memory.compact
```

Rules:

- default read-only unless explicitly configured
- every write includes actor/source metadata
- writes append events where practical
- destructive tools require explicit permission

## Suggested Phase Plan

### Phase 11 - Graph Links

- Rust link model and trait
- SQLite `links` table
- Node SDK `db.links`
- MCP link read tools

### Phase 12 - Hybrid Search

- object-to-text index
- SQLite FTS
- metadata filters
- graph-aware result expansion

### Phase 13 - Locks And Semaphores

- durable leases table
- acquire/renew/release
- queue heartbeat foundation
- sidecar coordination foundation

### Phase 14 - Workflow DAG

- workflow schema
- queue-backed step scheduling
- inspectable run state
- retry and resume support

### Phase 15 - Semantic Cache And Tool Ledger

- cache namespaces and TTL
- tool-call records
- cost/latency metadata
- MCP audit integration

### Phase 16 - Compaction And Snapshots

- snapshots table
- compaction queue jobs
- source event links
- retrieval uses summaries plus raw evidence
