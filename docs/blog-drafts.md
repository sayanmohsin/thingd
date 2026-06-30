# Dev.to blog post drafts — thingd

---

## Post 1: "Why your AI agent needs a data engine, not just a vector store"

**Tags:** `ai`, `mcp`, `llms`, `rust`, `opensource`

**Cover image:** Suggest a 1200x630 OG image with thingd logo on dark background

---

Most AI agent frameworks treat data as an afterthought. You get a vector
store for embeddings and maybe a key-value cache. But real agents need
more than similarity search — they need durable work queues, event
schedules, full-text search, and structured object storage.

### The problem

I kept rebuilding the same backend for every agent project:

- A place to store structured data (not just embeddings)
- A queue system with retries, leases, and dead-letter handling
- Event streams to track what happened and when
- Search that actually works (BM25 ranking, metadata filters)
- An MCP interface so the agent can use all of it directly

### The solution: thingd

thingd bundles all of these into one engine:

```
npm install @thingd/sdk
```

```typescript
import { ThingD } from "@thingd/sdk";

const db = await ThingD.open("agent-memory.db");

// Store structured data
await db.put("decisions", {
  id: "arch-1",
  text: "Use PostgreSQL for user data",
  status: "approved",
});

// Search with BM25 + metadata filters
const results = await db.search("postgres", {
  collections: ["decisions"],
  filter: { status: "approved" },
});

// Durable queues with leases and retries
await db.queue("embed-queue").push({
  payload: { docId: "arch-1", text: "Use PostgreSQL..." },
});
const job = await db.queue("embed-queue").claim();
// ... process ...
await db.queue("embed-queue").ack(job.id);

// Event streams
await db.events.append("agent:session-1", { type: "decision.made",
  data: { decision: "arch-1", reason: "..." },
});
```

### Why MCP matters

thingd speaks MCP (Model Context Protocol) natively. That means any
MCP-compatible agent — Claude Desktop, Cursor, Cline, or custom — gets
27 tools automatically:

| Category | Tools |
|----------|-------|
| Objects | `thing_put`, `thing_get`, `thing_delete`, `thing_objects_list`, `thing_objects_put_batch`, `thing_objects_delete_batch` |
| Search | `thing_search` |
| Queues | `thing_queue_push`, `thing_queue_claim`, `thing_queue_ack`, `thing_queue_nack`, `thing_queue_list`, `thing_queue_dead` |
| Events | `thing_events_append`, `thing_events_list` |
| Links | `thing_link_create`, `thing_link_delete`, `thing_link_get`, `thing_link_neighbors`, `thing_link_count` |
| Count | `thing_count_objects`, `thing_count_events`, `thing_count_active_jobs`, `thing_count_dead_jobs` |
| Discovery | `thing_list_collections`, `thing_list_streams`, `thing_list_queues` |

No schema definitions, no API generation, no manual tool registration.
The agent discovers everything at runtime.

### How it works under the hood

thingd is written in Rust (via NAPI-RS) with a Node.js SDK. Data
persists to SQLite with WAL mode, foreign keys, and FTS5 full-text
search.

Run it three ways:

1. **Embedded** — `ThingD.open("path.db")` in your Node.js app
2. **Sidecar** — `thingd mcp` starts an MCP server (stdio or HTTP)
3. **Docker** — `docker run -p 8757:8757 sayanmohsin/thingd`

### Try it

```bash
npx thingd install           # Set up MCP for your agent (local)
npx thingd mcp connect  # Set up MCP for your agent (cloud)
thingd dashboard      # Open the TUI inspector
```

GitHub: https://github.com/sayanmohsin/thingd

I'm looking for feedback on the API design, missing features, and
real-world use cases. Open an issue or drop a comment.

---

## Post 2: "Build an AI agent with persistent memory in 5 minutes"

**Tags:** `ai`, `tutorial`, `mcp`, `llms`, `javascript`

**Cover image:** Suggest a 1200x630 OG image with terminal/code aesthetic

---

This tutorial shows how to give an AI agent persistent memory using
thingd and Claude Desktop. Total setup time: under 5 minutes.

### What you'll build

An agent that:
- Remembers decisions across sessions
- Stores and searches project context
- Manages a work queue for background tasks
- Tracks events in an append-only log

### Prerequisites

- Node.js >= 24.0.0
- Claude Desktop (or any MCP client)

### Step 1: Install thingd

```bash
npm install -g @thingd/cli
```

### Step 2: Set up MCP

```bash
npx thingd install
```

This adds thingd to your Claude Desktop config. Restart Claude Desktop.

### Step 3: Verify it works

Open Claude Desktop and ask:

> What tools do you have for managing data?

Claude should list 27 `thing_*` tools covering objects, queues, events,
and search.

### Step 4: Use it in code

```typescript
import { ThingD } from "@thingd/sdk";

const db = await ThingD.open("agent-memory.db");

// Store a decision
await db.put("decisions", {
  id: "project-setup",
  text: "Use thingd for agent memory",
  timestamp: Date.now(),
});

// Search for it
const results = await db.search("agent memory");
console.log(results);
// [{ id: "project-setup", score: 0.95, ... }]
```

### Step 5: Queue background work

```typescript
// Push a job
await db.queue("embed-queue").push({
  payload: { docId: "project-setup", text: "..." },
});

// Claim and process
const job = await db.queue("embed-queue").claim();
// ... generate embeddings ...
await db.queue("embed-queue").ack(job.id);
```

### What's next?

- Read the [agent patterns guide](https://github.com/sayanmohsin/thingd/blob/main/docs/agent-patterns.md)
- Check the [MCP server reference](https://github.com/sayanmohsin/thingd/blob/main/docs/mcp-server.md)
- Join the discussion on GitHub

GitHub: https://github.com/sayanmohsin/thingd

---

## Post 3: "Why we built thingd: an object-first data engine for AI agents"

**Tags:** `ai`, `architecture`, `rust`, `databases`, `opensource`

**Cover image:** Suggest a 1200x630 OG image with architecture diagram

---

Most AI agent frameworks solve the model layer well. But the data layer
is fragmented — agents end up stitching together a vector store, a
key-value cache, a queue system, and an event log. thingd takes a
different approach: one engine, four primitives, one interface.

### The four primitives

**1. Objects**

Versioned JSON documents with no schema. Put, get, delete, list.

```typescript
await db.put("notes", { id: "n1", text: "hello", tags: ["greeting"] });
```

**2. Queues**

Durable job queues with leases, retries, delay, and dead-letter.

```typescript
await db.queue("work").push({ payload: { task: "process" } });
const job = await db.queue("work").claim();
await db.queue("work").ack(job.id);
```

**3. Events**

Append-only timelines. Track what happened and when.

```typescript
await db.events.append("session:1", { type: "action", data: { type: "click" } });
```

**4. Search**

FTS5 full-text search with BM25 ranking and metadata filters.

```typescript
const hits = await db.search("greeting", { collections: ["notes"] });
```

### Why object-first?

Most databases force you to define a schema before you can write data.
thingd lets you store any JSON object in any collection. This matters
for agents because:

1. Agent data is heterogeneous — memories, decisions, task results
2. Schemas evolve faster than migrations
3. The agent itself decides structure at runtime

### Why queues?

Agents need to do background work: generate embeddings, call APIs,
process results. A queue gives you:

- **Leases** — work is reserved, not duplicated
- **Retries** — failed jobs get a second chance
- **Dead-letter** — permanently failed jobs are isolated
- **Delay** — retry after N milliseconds

### Why events?

Agents need history. What did the agent do? When? Why? Events give you:

- Append-only log (no mutation)
- Stream-based organization
- Sequence numbers for ordered replay

### Why search?

Agents need to find things. "What did we decide about the database?"
BM25 ranking handles relevance. Metadata filters handle scope.

### The MCP layer

All four primitives are exposed via MCP (Model Context Protocol). Any
MCP-compatible agent gets 27 tools automatically:

```bash
npx thingd install           # Configure MCP (local)
npx thingd mcp connect       # Configure MCP (cloud)
thingd mcp                   # Start MCP server
```

### Try it

```bash
npm install @thingd/sdk
```

```typescript
import { ThingD } from "@thingd/sdk";
const db = await ThingD.open(":memory:");
await db.put("test", { id: "1", text: "hello" });
console.log(await db.get("test", "1"));
```

GitHub: https://github.com/sayanmohsin/thingd
