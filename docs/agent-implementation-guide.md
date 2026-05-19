# Agent Implementation Guide

This guide is for AI coding agents and future contributors integrating `memoryd` into apps.

Read this file before making integration changes. It explains the current project state, the intended API, local testing, and the boundaries that should not be crossed accidentally.

## Current State

`memoryd` is an early open source project. The public Node.js API is real enough to test locally, and the default path still uses the TypeScript in-memory proof store. The Rust core has durable storage with a feature-gated SQLite adapter for objects, events, and queues.

Current implementation:

- `packages/memoryd` exposes the Node.js SDK.
- `packages/memoryd/src/stores/in-memory-memory-store.ts` is the current proof store.
- `crates/memoryd-core` contains the Rust storage boundary, in-memory Rust engine, and `SqliteMemoryStore` behind the `sqlite` feature.
- `packages/memoryd-native` is a private N-API binding for local native driver testing.
- `packages/memoryd-mcp` exposes the SDK through stdio and Streamable HTTP MCP servers.
- `examples/nestjs-basic` demonstrates app integration shape.

Do not present the public Node package as production-ready persistent storage yet.

## Mental Model

`memoryd` is meant to feel like:

```txt
SQLite-simple local deployment
+ object-shaped app memory
+ events and timelines
+ durable queues
+ hybrid search
+ MCP-native agent access
```

There are two planned runtime modes:

```txt
embedded mode:
  Node.js app -> native Rust binding -> local memoryd file

server/sidecar mode:
  Node.js app -> HTTP/gRPC/Unix socket -> memoryd server -> local memoryd file

cluster sidecar mode:
  Node.js app -> localhost memoryd sidecar -> leader/follower memoryd cluster
```

Current Node.js code uses the TypeScript in-memory proof layer by default.
The Rust crate includes `SqliteMemoryStore` for object, event, and queue persistence, including delayed jobs, configurable lease expiration, retry delay, and dead-letter state. The SDK can opt into the private native bridge with `driver: "native"` after `@sayanmohsin/memoryd-native` is built locally.

## Integration Checklist

When integrating `memoryd` into a Node.js app:

1. Install or link the local package.
2. Create one `MemoryD` instance during app startup.
3. Wrap it in your framework's dependency injection layer if there is one.
4. Use collections for object-shaped app memory.
5. Use events for meaningful state changes and agent-readable timelines.
6. Use queues for background work such as embeddings, summarization, retries, and indexing.
7. Run local checks before claiming the integration works.

## Local Package Use

Inside this repository:

```bash
npm install
npm run build
npm run test:local
```

In another local app before npm publish:

```bash
npm install /Users/sayan/Documents/Experimental/memoryd/packages/memoryd
```

Or use a `file:` dependency:

```json
{
  "dependencies": {
    "@sayanmohsin/memoryd": "file:/Users/sayan/Documents/Experimental/memoryd/packages/memoryd"
  }
}
```

Use `npm run test:package` to verify the packed package works without publishing to npm.

Use `npm run bench:rust` when storage performance changes. Read
[benchmarks.md](./benchmarks.md) before treating local numbers as product
claims. Benchmark runs do not update docs automatically; baseline updates are
intentional documentation edits.

## Basic Node.js Pattern

```ts
import { MemoryD } from "@sayanmohsin/memoryd";

const db = await MemoryD.open("./memoryd.db");

await db.put("decisions", {
  id: "rust-core",
  text: "Use Rust for the core engine and TypeScript for the app-facing SDK.",
  project: "memoryd",
});

await db.events.append("project:memoryd", {
  type: "decision.made",
  text: "memoryd should stay object-shaped and MCP-native.",
  object: "decisions/rust-core",
});

const hits = await db.search("why rust?", {
  collections: ["decisions"],
});
```

## Queue Pattern

Queues are at-least-once. Consumers must be idempotent.

```ts
const queue = db.queue("embed");

await queue.push(
  {
    object: "docs/doc_123",
  },
  {
    idempotencyKey: "embed:docs/doc_123:v1",
    maxAttempts: 5,
  },
);

const job = await queue.claim({
  leaseMs: 30_000,
});

if (job) {
  try {
    await embedDocument(job.payload.object);
    await queue.ack(job.id);
  } catch (error) {
    await queue.nack(job.id, {
      delayMs: 5_000,
      error: error instanceof Error ? error.message : String(error),
    });
  }
}
```

Use `queue.dead()` to inspect jobs that exceeded `maxAttempts`.

## NestJS Pattern

Create a module-level provider and inject it into services/controllers.

```ts
import { Global, Module } from "@nestjs/common";
import { MemoryD } from "@sayanmohsin/memoryd";

export const MEMORYD = Symbol("MEMORYD");

@Global()
@Module({
  providers: [
    {
      provide: MEMORYD,
      useFactory: () => MemoryD.open("./memoryd.db"),
    },
  ],
  exports: [MEMORYD],
})
export class MemorydModule {}
```

Then inject it:

```ts
import { Inject, Injectable } from "@nestjs/common";
import type { MemoryD } from "@sayanmohsin/memoryd";
import { MEMORYD } from "./memoryd.module";

@Injectable()
export class DecisionsService {
  constructor(@Inject(MEMORYD) private readonly memoryd: MemoryD) {}

  async recordDecision(id: string, text: string) {
    const decision = await this.memoryd.put("decisions", {
      id,
      text,
    });

    await this.memoryd.events.append("project:memoryd", {
      type: "decision.made",
      text,
      object: `decisions/${id}`,
    });

    return decision;
  }
}
```

The current `examples/nestjs-basic` app uses a local adapter shape. Future work should move it onto the exported SDK once the example is ready to demonstrate the package directly.

## MCP Integration Shape

The MCP package wraps the same SDK surface. It should not bypass validation or use internal store implementation details.

Current tools:

```txt
memory.search
memory.objects.get
memory.objects.put
memory.objects.delete
memory.events.append
memory.events.list
memory.queue.push
memory.queue.claim
memory.queue.ack
memory.queue.nack
memory.queue.list
memory.queue.dead
```

The MCP package has stdio and Streamable HTTP entrypoints. Each future remote
MCP write should include actor/source metadata and should append an audit event
when practical. The current skeleton does not yet implement audit writes.

## Rust And Native Binding Direction

The public API should stay in `@sayanmohsin/memoryd`. Native support should be an implementation detail underneath it.

```txt
@sayanmohsin/memoryd
  MemoryD public API
  MemoryStore interface
  in-memory proof store
  NativeMemoryStore adapter

@sayanmohsin/memoryd-native
  private N-API binding
  wraps crates/memoryd-core

crates/memoryd-core
  ObjectStore
  EventLog
  QueueStore
  MemoryStore
  SqliteMemoryStore behind the sqlite feature
```

Do not introduce a second app-facing API from the native package. The native path should pass the same SDK tests that the in-memory store passes.

For storage decisions, read [persistence-and-native-bindings.md](./persistence-and-native-bindings.md).
For future AI-native data structures, read [ai-primitives.md](./ai-primitives.md).
For sidecar and cluster planning, read [sidecar-cluster.md](./sidecar-cluster.md).

## Implementation Rules For Agents

- Keep public API changes reflected in `packages/memoryd/src/types.ts`.
- Keep Rust storage boundary changes reflected in `crates/memoryd-core`.
- Add or update tests in `packages/memoryd/test/memoryd.test.mjs` for behavior changes.
- Update README/docs when changing integration behavior.
- Do not use internal store classes from app examples unless the example is explicitly about custom stores.
- Do not present native persistence as the default SDK path until prebuilds, migrations, and package loading are production-ready.
- Do not add a separate app-facing API to `@sayanmohsin/memoryd-native`; keep the public API in `@sayanmohsin/memoryd`.
- Do not claim exactly-once queue delivery. The queue is at-least-once.
- Do not hide distributed-system tradeoffs. Multi-pod writes need server/sidecar or primary-writer mode.
- Do not add multi-primary cluster behavior. Planned cluster mode is leader-writer with forwarding and event replication.
- Do not add generic textbook structures as public features unless they map to an AI-native workflow primitive in `docs/ai-primitives.md`.
- Keep sidecar environment variables and Kubernetes examples aligned with `docs/sidecar-cluster.md`.
- Keep package publish behavior in `release.config.mjs` and `docs/release.md` aligned.

## Required Checks

Before handing work back:

```bash
npm run test:local
```

If Rust is installed:

```bash
npm run rust:check
npm run rust:fmt:check
npm run rust:clippy
npm run test:rust
```

Rust checks run with all features enabled so the SQLite adapter is covered in CI.

`npm run test:local` does not run Rust checks because some local environments may not have `cargo` installed.

For storage benchmark work:

```bash
npm run bench:rust
npm run bench:rust:smoke
```

## Common Mistakes

- Importing from `src` or `dist` directly instead of `@sayanmohsin/memoryd`.
- Forgetting `npm run build` before testing packed package behavior.
- Mutating returned queue jobs and assuming that changes the store.
- Treating delayed jobs as claimable immediately.
- Treating `nack` as failure instead of retry/dead-letter routing.
- Adding npm publish assumptions before `NPM_TOKEN` is configured.
- Using queue consumers without idempotency keys for repeatable work.
- Assuming `MemoryD.open("./memoryd.db")` persists through the Node SDK before native bindings are wired.
