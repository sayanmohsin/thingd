# Why thingd?

## The gap

Modern apps and AI agents need more than a database. They need to store
object-shaped records, search them, queue background work, keep audit trails,
and expose tools to agents — without stitching together 3-5 separate services.

Most teams end up running:

```txt
Postgres (data) + Redis (queues) + Elasticsearch (search) + Kafka (events)
```

thingd replaces this stack with a single binary.

---

## What you get

### Objects — versioned JSON, no schema

Store any JSON record in a named collection. No migrations, no ORM, no schema
design. Every object gets an auto-incrementing version.

```ts
await db.put("customers", { id: "cus_1", name: "Acme", plan: "pro" });
```

### Queues — durable background jobs

Push work, claim it with a lease, ack on success, nack on failure. Built-in
retries, delays, idempotency keys, and dead-letter handling — no Redis, no
SQS, no sidekiq.

```ts
await db.queue("embed").push(
  { docId: "doc-1" },
  { idempotencyKey: "embed:doc-1", maxAttempts: 3 }
);
```

### Events — append-only timelines

Every mutation can produce an audit trail. Timelines are per-stream with
auto-incrementing sequence numbers.

```ts
await db.events.append("project:thingd", {
  type: "decision.made",
  text: "Rust core selected.",
  actor: "sayan",
});
```

### Search — FTS5 with metadata filters

SQLite FTS5 with Porter stemming, metadata key-value filters, and recency-decay
scoring. No external search index to deploy.

```ts
const hits = await db.search("rust", { collections: ["decisions"] });
```

### Links — directed graph relationships

Link objects, events, and references. Query incoming and outgoing neighbors.

```ts
await db.links.create("doc-1", "references", "doc-2");
const neighbors = await db.links.neighbors("doc-1", "outgoing");
```

### MCP — tool access for AI agents

All primitives are exposed as MCP tools ({{ $themeConfig.mcpToolCount }} total)
so AI agents can search, read, write, and manage data without custom
integrations. MCP is one access layer — not the product.

---

## Local-first by design

thingd runs everywhere with the same API:

| Mode | What you get |
|---|---|
| **In-memory** | Zero setup, fast tests, REPL-style exploration |
| **SQLite** | Durable persistence, FTS5, WAL mode — one file |
| **Docker** | Sidecar server, ~15MB image, instant startup |
| **Embedded** | In-process with your Node.js app, zero network calls |

No cluster to configure. No cloud to sign up for. Just `npm install` and go.

---

## When to use thingd

- **Small to medium apps** — one runtime instead of 3-5 infrastructure dependencies
- **Devtools and internal tools** — ship with embedded data, no Postgres required
- **Edge and single-node deployments** — SQLite is the right choice at this scale
- **AI agent memory** — persistent, structured, tool-addressable state
- **Prototypes and MVPs** — start with memory, graduate to SQLite, later to Docker

## When to skip thingd

- **Large-scale analytics** — you want columnar storage or a data warehouse
- **Multi-primary writes** — thingd is single-writer (leader/follower for reads)
- **Full SQL compatibility** — thingd is object-shaped, not relational
- **You already run Postgres + Redis + Elasticsearch** and it works for you

---

## Next steps

- [Quickstart](./quickstart.md) — 5 minutes to your first object
- [Architecture](./architecture.md) — how the engine is structured
- [Why agents use thingd](./why-agents.md) — the AI agent value proposition
- [thingd Cloud](https://thingd.cloud) — managed hosting, no ops needed
