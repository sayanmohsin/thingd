# Agent Implementation Guide

This guide is for AI coding agents and future contributors integrating `thingd` into apps.

Read this file before making integration changes. It explains the current project state, the intended API, local testing, and the boundaries that should not be crossed accidentally.

## Current State

`thingd` is an early open source project. The public Node.js API is real enough to test locally, and the default path still uses the TypeScript in-memory proof store. The Rust core has durable storage with a feature-gated SQLite adapter for objects, events, and queues.

Current implementation:

- `packages/thingd` exposes the Node.js SDK.
- `packages/thingd/src/stores/in-memory-thing-store.ts` is the current proof store.
- `crates/thingd-core` contains the Rust storage boundary, in-memory Rust engine, and `SqliteThingStore` behind the `sqlite` feature.
- `packages/thingd-native` is a private N-API binding for local native driver testing.
- `packages/thingd/src/stores/remote-thing-store.ts` lets the SDK talk to a sidecar over Streamable HTTP MCP.
- `packages/thingd-cli` exposes the visual TUI dashboard, non-interactive CLI commands, and integrated stdio and Streamable HTTP MCP servers.
- the HTTP MCP runtime supports `single`, `leader`, and `follower` bridge modes.
- `examples/nestjs-basic` demonstrates app integration shape.
- `docs/cli.md` is the handoff plan for remaining admin/operator CLI phases.

Do not present the public Node package as production-ready persistent storage yet.

## Mental Model

`thingd` is meant to feel like:

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
  Node.js app -> native Rust binding -> local thingd file

server/sidecar mode:
  Node.js app -> HTTP/gRPC/Unix socket -> thingd server -> local thingd file

cluster sidecar mode:
  Node.js app -> localhost thingd sidecar -> leader/follower thingd cluster
```

Current Node.js code uses the TypeScript in-memory proof layer by default.
The Rust crate includes `SqliteThingStore` for object, event, and queue persistence, including delayed jobs, configurable lease expiration, retry delay, dead-letter state, and schema migration guardrails. The SDK can opt into the private native bridge with `driver: "native"` after `thingd-native` is built locally.
The SDK can opt into sidecar mode with `driver: "remote"` or automatically when
`THINGD_URL` is set.
The HTTP MCP runtime can run as a bridge follower and forward MCP traffic to a
configured leader. It does not yet replicate local follower stores.

## Integration Checklist

When integrating `thingd` into a Node.js app:

1. Install or link the local package.
2. Create one `ThingD` instance during app startup.
3. Wrap it in your framework's dependency injection layer if there is one.
4. Use collections for object-shaped app memory.
5. Use events for meaningful state changes and agent-readable timelines.
6. Use queues for background work such as embeddings, summarization, retries, and indexing.
7. Run local checks before claiming the integration works.

## Local Package Use

Inside this repository:

```bash
pnpm install
pnpm build
pnpm test:local
```

In another local app before npm publish:

```bash
pnpm add /Users/sayan/Documents/Experimental/thingd/packages/thingd
```

Or use a `file:` dependency:

```json
{
  "dependencies": {
    "thingd": "file:/Users/sayan/Documents/Experimental/thingd/packages/thingd"
  }
}
```

Use `pnpm test:package` to verify the packed package works without publishing to npm.

For sidecar mode:

```bash
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=change-me
```

```ts
const db = await ThingD.open();
```

Use `pnpm bench:rust` when storage performance changes. Read
[benchmarks.md](./benchmarks.md) before treating local numbers as product
claims. Benchmark runs do not update docs automatically; baseline updates are
intentional documentation edits.

## Basic Node.js Pattern

```ts
import { ThingD } from "thingd";

const db = await ThingD.open();

await db.put("decisions", {
  id: "rust-core",
  text: "Use Rust for the core engine and TypeScript for the app-facing SDK.",
  project: "thingd",
});

await db.events.append("project:thingd", {
  type: "decision.made",
  text: "thingd should stay object-shaped and MCP-native.",
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
import { ThingD } from "thingd";

export const THINGD = Symbol("THINGD");

@Global()
@Module({
  providers: [
    {
      provide: THINGD,
      useFactory: () => new ThingD({ /* ... */ }),
    },
  ],
  exports: [THINGD],
})
export class ThingDModule {}

// Usage in other modules:
import { Injectable, Inject } from "@nestjs/common";
import { ThingD } from "thingd";
import { THINGD } from "./thingd.module";

@Injectable()
export class WorkflowService {
  constructor(@Inject(THINGD) private readonly thingd: ThingD) {}

  async recordDecision(id: string, text: string) {
    const decision = await this.thingd.put("decisions", {
      id,
      text,
    });

    await this.thingd.events.append("project:thingd", {
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
thing.search
thing.get
thing.put
thing.delete
thing.events.append
thing.events.list
thing.queue.push
thing.queue.claim
thing.queue.ack
thing.queue.nack
thing.queue.list
thing.queue.dead
```

The MCP package has stdio and Streamable HTTP entrypoints. MCP write tools append
audit events to `__thingd:mcp:audit` by default. Tool callers can pass optional
`actor` and `source` fields, and runtimes can set defaults with
`THINGD_MCP_ACTOR` and `THINGD_MCP_SOURCE`.

## Rust And Native Binding Direction

The public API should stay in `thingd`. Native support should be an implementation detail underneath it.

```txt
thingd
  ThingD public API
  ThingStore interface
  in-memory proof store
  NativeThingStore adapter

thingd-native
  private N-API binding
  wraps crates/thingd-core

crates/thingd-core
  ObjectStore
  EventLog
  QueueStore
  ThingStore
  SqliteThingStore behind the sqlite feature
```

Do not introduce a second app-facing API from the native package. The native path should pass the same SDK tests that the in-memory store passes.

For storage decisions, read [persistence-and-native-bindings.md](./persistence-and-native-bindings.md).
For future AI-native data structures, read [ai-primitives.md](./ai-primitives.md).
For sidecar and cluster planning, read [sidecar-cluster.md](./sidecar-cluster.md).
For CLI work, read [cli.md](./cli.md) before creating commands or package structure.
For a project restart summary, read [handoff.md](./handoff.md).

## Implementation Rules For Agents

- Keep public API changes reflected in `packages/thingd/src/types.ts`.
- Keep Rust storage boundary changes reflected in `crates/thingd-core`.
- Add or update tests in `packages/thingd/test/thingd.test.mjs` for behavior changes.
- Update README/docs when changing integration behavior.
- Do not use internal store classes from app examples unless the example is explicitly about custom stores.
- Do not present native persistence as the default SDK path until prebuilds and package loading are production-ready.
- Do not add a separate app-facing API to `thingd-native`; keep the public API in `thingd`.
- Do not claim exactly-once queue delivery. The queue is at-least-once.
- Do not hide distributed-system tradeoffs. Multi-pod writes need server/sidecar or primary-writer mode.
- Do not add multi-primary cluster behavior. Planned cluster mode is leader-writer with forwarding and event replication.
- Do not add generic textbook structures as public features unless they map to an AI-native workflow primitive in `docs/ai-primitives.md`.
- Keep sidecar environment variables and Kubernetes examples aligned with `docs/sidecar-cluster.md`.
- Keep package publish behavior in `release.config.mjs` and `docs/release.md` aligned.
- For CLI work, create a dedicated package and use the public SDK instead of reaching into internal stores.
- Keep CLI command behavior documented in `docs/cli.md`.

## Recommended Next Phase

Start with **Phase CLI-B** from [cli.md](./cli.md).

The first-pass `thingd` binary can inspect and mutate local or remote stores
with JSON output. The next goal is operator polish: pretty tables, `doctor`,
queue stats, benchmark wrappers, and better runtime errors. This should land
before an inspector UI because it helps local development, Docker sidecar
debugging, Kubernetes handoff, and AI-agent integration immediately.

## Required Checks

Before handing work back:

```bash
pnpm test:local
pnpm test:cli
```

If Rust is installed:

```bash
pnpm rust:check
pnpm rust:fmt:check
pnpm rust:clippy
pnpm test:rust
```

Rust checks run with all features enabled so the SQLite adapter is covered in CI.

`pnpm test:local` does not run Rust checks because some local environments may not have `cargo` installed.

For storage benchmark work:

```bash
pnpm bench:rust
pnpm bench:rust:smoke
```

## Common Mistakes

- Importing from `src` or `dist` directly instead of `thingd`.
- Forgetting `pnpm build` before testing packed package behavior.
- Mutating returned queue jobs and assuming that changes the store.
- Treating delayed jobs as claimable immediately.
- Treating `nack` as failure instead of retry/dead-letter routing.
- Adding npm publish assumptions before `NPM_TOKEN` is configured.
- Using queue consumers without idempotency keys for repeatable work.
- Assuming `ThingD.open("./thingd.db")` persists. Use `driver: "native"` for
  embedded SQLite or `THINGD_URL` for sidecar mode.
