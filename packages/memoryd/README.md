# @sayanmohsin/memoryd

# @sayanmohsin/memoryd

Object-shaped local memory for AI-native apps. SQLite-simple, MCP-native, with search, events, and durable queues.

This is the Node.js SDK for `memoryd`. It exposes the intended object/event/queue abstractions with an in-memory store by default. Inside the `memoryd` repository it can also use the private Rust-backed native driver for local SQLite persistence, and a remote driver for sidecar/server mode.

## Installation

You can install the published SDK from npm:

```bash
npm install @sayanmohsin/memoryd
```

## Example Usage

### 1. Connecting and Basic Operations

```ts
import { MemoryD } from "@sayanmohsin/memoryd";

// Open the default in-memory store (for testing)
const db = await MemoryD.open();

// Store an object
await db.put("decisions", {
  id: "rust-core",
  text: "Use Rust for the core engine and TypeScript for the developer API.",
  project: "memoryd",
  confidence: 0.9,
});

// Retrieve an object
const decision = await db.get("decisions", "rust-core");
```

### 2. Events and Timelines

Every meaningful mutation can produce an event. Events make memory easier for humans and agents to understand.

```ts
await db.events.append("project:memoryd", {
  type: "decision.made",
  text: "memoryd will be object-shaped and MCP-native.",
  actor: "sayan",
});

const events = await db.events.list("project:memoryd");
```

### 3. Durable Queues

`memoryd` includes queue primitives because AI apps constantly need background work (chunking, embeddings, summarization, etc).

```ts
// Enqueue a job
await db.queue("embed").push({
  object: "decisions/rust-core",
});

// Claim a job with a 30-second visibility lease
const job = await db.queue("embed").claim({
  leaseMs: 30_000,
});

if (job) {
  try {
    // Process the job
    await embedDocument(job.payload.object);
    // Mark as complete
    await db.queue("embed").ack(job.id);
  } catch (error) {
    // Return to the queue for a retry
    await db.queue("embed").nack(job.id, {
      delayMs: 5_000,
      error: String(error),
    });
  }
}
```

### 4. Search (Planned / Experimental)

```ts
const hits = await db.search("why did we choose rust?", {
  collections: ["decisions"],
  limit: 5,
});
```

## Running against a sidecar / Docker Runtime

To use a running `memoryd` sidecar or Docker runtime instead of the local memory, you can connect over HTTP MCP:

```bash
MEMORYD_URL=http://127.0.0.1:8757
MEMORYD_AUTH_TOKEN=change-me
```

```ts
import { MemoryD } from "@sayanmohsin/memoryd";

// Automatically uses remote driver if MEMORYD_URL is set
const db = await MemoryD.open();
```

Or configure the remote driver explicitly:

```ts
const db = await MemoryD.open({
  url: "http://127.0.0.1:8757/mcp",
  driver: "remote",
  authToken: "change-me",
});
```

Remote mode uses the exact same SDK methods, but talks to the sidecar over Streamable HTTP MCP.

---

For full architecture details, local SQLite native drivers, and MCP server documentation, visit the [memoryd GitHub repository](https://github.com/sayanmohsin/memoryd).

`memoryd` is open source under the Apache-2.0 license.
