# Agent Implementation Guide

This guide is for AI coding agents and future contributors integrating `thingd` into apps.

Read this file before making integration changes. It explains the current project state, the intended API, local testing, and the boundaries that should not be crossed accidentally.

## Current State

`thingd` is an open source project. The public Node.js API is real enough to test locally, and the default path still uses the TypeScript in-memory store. The Rust core has durable persistent storage for objects, events, queues, search, and vectors.

Current implementation:

- `packages/thingd` exposes the Node.js SDK.
- `packages/thingd/src/stores/in-memory-thing-store.ts` is the current in-memory store.
- `crates/thingd` contains the Rust storage boundary, in-memory Rust engine, and `PersistentEngine` behind the `persistent` feature.
- `packages/thingd-native` is a private N-API binding for local native driver testing.
- `packages/thingd/src/client/http-thing-store.ts` lets the SDK talk to a sidecar over HTTP REST.
- `packages/thingd-cli` exposes the visual TUI dashboard, non-interactive CLI commands, and integrated stdio and Streamable HTTP MCP servers.
- the HTTP MCP runtime supports `single`, `leader`, and `follower` bridge modes.
- `examples/nestjs-basic` demonstrates app integration shape.

Do not present the public Node package as production-ready persistent storage yet.

## Mental Model

`thingd` is meant to feel like:

```txt
simple persistent local deployment
+ object-shaped app memory
+ events and timelines
+ durable queues
+ hybrid search
+ MCP-native agent access
```

There are two runtime modes:

```txt
embedded mode:
  Node.js app -> native Rust binding -> local persistent directory

server/sidecar mode:
  Node.js app -> HTTP/gRPC/Unix socket -> thingd server -> local thingd file

cluster sidecar mode:
  Node.js app -> localhost thingd sidecar -> leader/follower thingd cluster
```

Current Node.js code uses the TypeScript in-memory store by default. Durable
local persistence uses the native persistent adapter through `driver: "native"`
after `thingd-native` is built locally. The deprecated SQLite adapter remains
available only for historical compatibility and is not the current runtime
model.
The SDK can opt into sidecar mode with `driver: "cloud"` or automatically when
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
pnpm add /path/to/thingd/packages/thingd
```

Or use a `file:` dependency:

```json
{
  "dependencies": {
    "@thingd/sdk": "file:/path/to/thingd/packages/thingd"
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
import { ThingD } from "@thingd/sdk";

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
import { ThingD } from "@thingd/sdk";

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
import { ThingD } from "@thingd/sdk";
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

The current `examples/nestjs-basic` app uses a local adapter shape.

## MCP Integration Shape

The MCP package wraps the same SDK surface. It should not bypass validation or use internal store implementation details.

Current tools:

```txt
thing_search
thing_get
thing_put
thing_delete
thing_events_append
thing_events_list
thing_queue_push
thing_queue_claim
thing_queue_ack
thing_queue_nack
thing_queue_list
thing_queue_dead
```

The MCP package has stdio and Streamable HTTP entrypoints. MCP write tools append
audit events to `__thingd:mcp:audit` by default. Tool callers can pass optional
`actor` and `source` fields, and runtimes can set defaults with
`THINGD_MCP_ACTOR` and `THINGD_MCP_SOURCE`.

## Rust And Native Binding Architecture

The public API should stay in `thingd`. Native support should be an implementation detail underneath it.

```txt
thingd
  ThingD public API
  ThingStore interface
  in-memory store
  NativeThingStore adapter

thingd-native
  private N-API binding
  wraps crates/thingd

crates/thingd
  ObjectStore
  EventLog
  QueueStore
  ThingStore
  PersistentEngine behind the persistent feature
```

Do not introduce a second app-facing API from the native package. The native path should pass the same SDK tests that the in-memory store passes.

For storage decisions and the Rust/native binding direction, see the Rust crate docs and the native binding package.

For CLI work, read [cli-reference.md](./cli-reference.md) before creating commands or package structure.
For agent value and patterns, read [why-agents.md](./why-agents.md) and
[agent-patterns.md](./agent-patterns.md).

## Implementation Rules For Agents

- Keep public API changes reflected in `packages/thingd/src/types.ts`.
- Keep Rust storage boundary changes reflected in `crates/thingd`.
- Add or update tests in `packages/thingd/test/thingd.test.mjs` for behavior changes.
- Update README/docs when changing integration behavior.
- Do not use internal store classes from app examples unless the example is explicitly about custom stores.
- Do not present native persistence as the default SDK path until prebuilds and package loading are production-ready.
- Do not add a separate app-facing API to `thingd-native`; keep the public API in `thingd`.
- Do not claim exactly-once queue delivery. The queue is at-least-once.
- Do not hide distributed-system tradeoffs. Multi-pod writes need server/sidecar or primary-writer mode.
- Do not add multi-primary cluster behavior. Cluster mode uses leader-writer with forwarding and event replication.
- Do not add generic textbook structures as public features unless they map to an AI-native workflow primitive.
- Keep sidecar environment variables and Kubernetes examples aligned with the deployed runtime configuration.
- Keep package publish behavior in `release.config.mjs` and `docs/release.md` aligned.
- For CLI work, create a dedicated package and use the public SDK instead of reaching into internal stores.
- Keep CLI command behavior documented in [cli-reference.md](./cli-reference.md).

## For a 5-minute working example: **[QUICKSTART.md](./QUICKSTART.md)**

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

Rust checks run with all features enabled so the persistent adapter is covered in CI.

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
  embedded persistent or `THINGD_URL` for sidecar mode.
