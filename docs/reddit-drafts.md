# Reddit post drafts — thingd

---

## r/javascript

**Title:** `thingd — data engine with queues, search, events, and MCP. Try it in 30 seconds.`

```
npm install @thingd/sdk && node --input-type=module -e "
import { ThingD } from '@thingd/sdk';
const db = await ThingD.open(':memory:');
await db.put('test', { id: 'hello', text: 'Hello world' });
console.log(await db.get('test', 'hello'));
"

That's it. One npm package. Object store, durable queues, full-text search,
event streams, and 27 MCP tools for AI agents — all in one engine.

Written in Rust (NAPI-RS), Node.js SDK, Apache 2.0.
Run embedded, sidecar (MCP stdio/HTTP), Docker, or cluster.

I'd love for people to actually try it and tell me what's missing, what's
confusing, or what you'd build with it. Drop a comment — good or bad.

https://github.com/sayanmohsin/thingd
```

---

## r/node

**Title:** `thingd — one npm install replaces your DB, queue, search, and event stream`

```
npm install @thingd/sdk

import { ThingD } from "@thingd/sdk";
const db = await ThingD.open("persistent.db");
await db.put("users", { id: "abc", name: "test" });

// Full-text search
await db.search("test");

// Durable queue
await db.queuePush("jobs", { payload: { task: "process" } });
const job = await db.queueClaim("jobs");
await db.queueAck("jobs", job.id);

// Event stream
await db.eventAppend("orders", "created", { data: { orderId: "123" } });

// 27 MCP tools — AI agents can use it out of the box

36 commands total in the CLI. TUI dashboard. Docker image.

Rust engine, SQLite persistence, Apache 2.0.

Try it. Break it. Tell me what sucks. I'm actually reading comments.

https://github.com/sayanmohsin/thingd
```

---

## r/programming

**Title:** `thingd — object store + durable queues + FTS + event streams, one engine (Apache 2.0)`

```
thingd is an open-source data engine that replaces the usual
3-5 backend stack with a single dependency.

What's inside:
├─ Object store — versioned JSON, no schema
├─ Durable queues — leases, retries, DLQ, idempotency keys
├─ Event streams — append-only timelines
├─ Full-text search — FTS5, BM25 ranking, metadata filters
├─ MCP native — 27 tools AI agents discover automatically
└─ Multi-mode — embedded, sidecar, Docker, cluster

Stack: Rust core (NAPI-RS), Node.js/TypeScript SDK, SQLite.
~3.3k lines Rust, ~3.8k lines TS. Apache 2.0.

I posted this to get real feedback — not just stars. Try it on a real
project and tell me where it falls apart. Issues, PRs, and "this is
dumb because X" comments all welcome.

https://github.com/sayanmohsin/thingd
```

---

## r/opensource

**Title:** `thingd 0.20 — local-first data engine for apps and AI agents. Try it, break it, tell me what to fix.`

```
thingd is an open-source data engine (Apache 2.0) that bundles storage,
queues, search, and event streams into one embeddable package.

Features:
- Object store (versioned JSON)
- Durable queues (leases, retries, DLQ)
- Full-text search (FTS5, BM25)
- Event streams (append-only)
- MCP-native (27 tools for AI agents)
- SQLite or in-memory
- CLI + TUI dashboard + Docker

I'm looking for:
- People to try it and open issues when something doesn't make sense
- Contributors for Python/Go SDKs, Postgres backend, docs
- Honest feedback on the API design

Getting started:
  npm install @thingd/sdk
  # or
  docker run -p 8757:8757 sayanmohsin/thingd

https://github.com/sayanmohsin/thingd

Don't just star it — try it and tell me what's broken.
```

---

## r/MCP

**Title:** `thingd — embeddable data engine with 27 built-in MCP tools. One-line agent setup.`

```
thingd ships with 27 MCP tools out of the box. Every MCP client (Claude,
Cursor, Cline, etc.) can connect and use it immediately.

Try it with Claude Desktop:
  npx thingd install           # Local setup
  npx thingd mcp connect       # Cloud setup (after thingd cloud login)

Or via Docker:
  docker run -p 8757:8757 sayanmohsin/thingd
  # Point your agent to http://localhost:8757/mcp

The 27 tools cover: search, put/get/delete objects, queue push/claim/ack/nack,
event append/list, graph links, metrics, collections/streams/queues listing.

Under the hood: Rust engine, SQLite, Node.js SDK, Apache 2.0.

I want to know:
- Does the MCP setup work smoothly for your agent?
- What tools are missing?
- What's the first thing you tried to do that didn't work?

https://github.com/sayanmohsin/thingd
```

---

## r/AI_Agents

**Title:** `thingd — give your AI agent persistent memory + 27 tools in one command`

```
Every AI agent I built ended up needing a way to store, search, and
manage data. thingd solves that — one data engine that speaks MCP.

Setup for Claude/Cursor/Cline:
  npx thingd install

Or Docker:
  docker run -p 8757:8757 sayanmohsin/thingd

What your agent gets:
→ Persistent object store (put/get/delete/search)
→ Durable queues (push jobs, claim, ack/nack)
→ Event streams (append, list, search)
→ 27 MCP tools total, auto-discovered

All data persists locally via SQLite. No cloud dependency.
Open source, Apache 2.0.

Try it with your agent and let me know how it goes.
What's the first real use case you'd throw at it?

https://github.com/sayanmohsin/thingd
```
