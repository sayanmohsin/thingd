# thingd

[![npm version (SDK)](https://badge.fury.io/js/thingd.svg)](https://www.npmjs.com/package/thingd)

A fast object-first data engine for applications and AI agents.

This is the **thingd** Node.js SDK — a high-performance object-first data engine that combines persistent storage, durable queues, event streams, full-text search, and MCP-native access. This package exposes the object/event/queue/search abstractions with an in-memory store by default, with optional native SQLite persistence and remote sidecar connectivity.

> Looking for the admin dashboard and CLI? Check out the [thingd-cli](https://www.npmjs.com/package/thingd-cli) package!

## Installation

You can install the published SDK from npm:

```bash
npm install thingd
```

## Example Usage

### 1. Connecting and Basic Operations

```ts
import { ThingD } from "thingd";

// Open the default in-memory store (for testing)
const db = await ThingD.open();

// Store an object
await db.put("decisions", {
  id: "rust-core",
  text: "Use Rust for the core engine and TypeScript for the developer API.",
  project: "thingd",
  confidence: 0.9,
});

// Retrieve an object
const decision = await db.get("decisions", "rust-core");
```

### 2. Events and Timelines

Every meaningful mutation can produce an event. Events make memory easier to understand and audit.

```ts
await db.events.append("project:thingd", {
  type: "decision.made",
  text: "thingd will be object-shaped and MCP-native.",
  actor: "sayan",
});

const events = await db.events.list("project:thingd");
```

### 3. Durable Queues

`thingd` includes queue primitives because apps constantly need background work (chunking, embeddings, summarization, etc).

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

## Using the Native Driver Directly

For persistent file-based storage, use the native SQLite driver instead of the in-memory store:

```ts
import { ThingD } from "thingd";

const db = await ThingD.open({
  path: "./my-app.db",
  driver: "native",
});

await db.put("collections", { id: "notes", text: "My first note" });
const result = await db.get("collections", "notes");
console.log(result);
await db.close();
```

You can also import the store class directly for more control:

```ts
import { NativeThingStore } from "thingd";

const store = new NativeThingStore({ path: "./data.db" });
await store.init();

await store.put("users", { id: "alice", name: "Alice" });
const user = await store.get("users", "alice");
await store.close();
```

## MCP Client

Connect to a remote thingd MCP server programmatically:

```ts
import { CloudThingStore } from "thingd";

const store = new CloudThingStore({
  url: "http://127.0.0.1:8757/mcp",
  authToken: "change-me",
});

await store.init();

await store.put("tasks", { id: "demo", text: "MCP-powered task" });
const tasks = await store.listObjects("tasks");
console.log(tasks);
await store.close();
```

## Running against a sidecar / Docker Runtime

To use a running `thingd` sidecar or Docker runtime instead of the local memory, you can connect over HTTP MCP:

```bash
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=change-me
```

```ts
import { ThingD } from "thingd";

// Automatically uses remote driver if THINGD_URL is set
const db = await ThingD.open();
```

Or configure the remote driver explicitly:

```ts
const db = await ThingD.open({
  url: "http://127.0.0.1:8757/mcp",
  driver: "remote",
  authToken: "change-me",
});
```

Remote mode uses the exact same SDK methods, but talks to the sidecar over Streamable HTTP MCP.

---

For full architecture details, local SQLite native drivers, and MCP server documentation, visit the [thingd GitHub repository](https://github.com/sayanmohsin/thingd).

`thingd` is open source under the Apache-2.0 license.
