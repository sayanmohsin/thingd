# Vision

`memoryd` is a local data runtime for software that needs to be readable by both application code and AI agents.

The first principle is simple: keep the deployment feel close to SQLite, but expose primitives closer to how modern AI-native apps work.

## Core Ideas

### Object-shaped storage

Apps should be able to store records as objects without designing a relational schema first. Under the hood, `memoryd` can still use mature storage engines, but the public API should feel like:

```ts
await db.put("customers", { id: "cus_123", name: "Acme", plan: "pro" });
```

### MCP-native access

The database should ship with safe MCP tools so agents can search, read, write, and append events without learning internal table layouts.

```txt
memory.search
memory.get
memory.put
memory.patch
memory.events.append
memory.queue.push
```

### AI-readable by default

Stored records should be easy to turn into text, summaries, embeddings, and links. Each memory can carry structured metadata, source information, confidence, timestamps, and permissions.

### Events first

Mutations should produce an append-only trail. This supports auditability, sync, rebuildable indexes, agent timelines, and future replication.

### Durable workflow primitives

AI apps need background processing: chunking, embedding, summarization, tool calls, retries, and compaction. Durable queues and leases should be first-class, not an afterthought.

## Non-goals

- replacing SQLite as a storage engine
- pretending local files magically support multi-primary writes across pods
- building a huge distributed database before the local developer experience is excellent

## First Useful Release

The first useful version should make a Node.js app able to:

- put and get object records
- append events
- enqueue and consume durable jobs
- search local memory
- expose MCP tools for agent access

Replication, vector indexing, and production multi-pod coordination should come after that foundation is pleasant and reliable.
