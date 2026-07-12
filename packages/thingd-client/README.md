# @thingd/client

Zero-dependency HTTP client for thingd — works in **browsers**, Cloudflare Workers, AWS Lambda, Bun, Deno, and Node.js 18+.

```ts
import { ThingdClient } from "@thingd/client";

const client = new ThingdClient({
  url: "https://api.thingd.cloud",
  authToken: "sk-...",
});

await client.put("notes", { id: "1", text: "Hello world" });
const note = await client.get("notes", "1");
```

## Why this package?

`@thingd/client` is a **pure fetch-based** client with zero runtime dependencies (~5KB). Use it when you need to call the thingd REST API from environments where Node.js packages aren't available or desirable.

## Install

```bash
npm install @thingd/client
```

## Usage

Connect to any thingd instance — local sidecar, Docker, or thingd.cloud:

```ts
const client = new ThingdClient({
  url: "http://localhost:8757",             // local sidecar
  // url: "https://api.thingd.cloud",       // thingd.cloud
  authToken: "your-api-key",
});
```

### Objects

```ts
await client.put("users", { id: "user-1", name: "Alice" });
const user = await client.get("users", "user-1");
const deleted = await client.delete("users", "user-1");
const users = await client.listObjects("users", {
  filter: { role: "admin" },
  sortBy: { field: "created_at", direction: "desc" },
  limit: 10,
});
```

### Search

```ts
const results = await client.search("alice", {
  collections: ["users", "notes"],
  limit: 5,
});
```

### Events

```ts
await client.events.append("audit", {
  type: "user.login",
  text: "Alice logged in",
});
const events = await client.events.list("audit", { limit: 10 });
```

### Queues

```ts
await client.queue("email").push(
  { to: "alice@example.com" },
  { idempotencyKey: "email:user-1", maxAttempts: 3 }
);

const job = await client.queue("email").claim({ leaseMs: 30_000 });
if (job) {
  await client.queue("email").ack(job.id);
}
```

### Links

```ts
await client.links.create("users/alice", "authored", "notes/1");
const neighbors = await client.links.neighbors("users/alice", "Outgoing");
```

### Aggregation

```ts
const count = await client.aggregate.count("sales", {
  groupBy: "region",
});
const revenue = await client.aggregate.sum("sales", "amount");
```

### NLQ (Natural Language Query)

```ts
const result = await client.nlq.query("What were total sales by region?");
```

## API

| Method | Description |
|--------|-------------|
| `put(collection, object)` | Create or replace an object |
| `get(collection, id)` | Get an object by ID |
| `delete(collection, id)` | Delete an object |
| `listObjects(collection, opts)` | List/filter/sort objects |
| `search(query, opts)` | Full-text search |
| `events.append(stream, event)` | Append an event |
| `events.list(stream?, opts)` | List events |
| `queue(name).push(payload, opts)` | Push a queue job |
| `queue(name).claim(opts)` | Claim a job |
| `queue(name).ack(jobId)` | Acknowledge a job |
| `queue(name).nack(jobId, opts)` | Nack a job |
| `links.create(from, type, to)` | Create a graph link |
| `links.neighbors(ref, dir)` | Get linked references |
| `aggregate.count/sum/avg/min/max` | Aggregation queries |
| `timeseries(collection, opts)` | Time-bucketed aggregation |
| `schema(collection?)` | Reflect collection schema |
| `nlq.query(question)` | Natural language query |
| `listCollections()` | List all collections |
| `listStreams()` | List all event streams |
| `listQueues()` | List all queues |
| `close()` | Clean up |

## thingd.cloud

Connect to thingd Cloud using a project-scoped API key:

```ts
const client = new ThingdClient({
  url: "https://api.thingd.cloud",
  authToken: "md_live_xxx",
});
```

## License

Apache-2.0
