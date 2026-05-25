# Project Handoff

This file is the quick restart point for future work on `thingd`.

Last updated: 2026-05-19.

## Current Shape

`thingd` is an early open source, Rust-powered, object-shaped memory database
for AI-native apps. It combines:

- Node.js SDK
- Rust storage traits and in-memory engine
- `rusqlite` durable object, event, and queue store behind the `sqlite` feature
- private N-API native driver for local testing
- Streamable HTTP MCP sidecar runtime
- stdio MCP runtime
- queue primitives with leases, retries, delayed jobs, and dead-letter jobs
- MCP audit events
- Docker runtime scaffold
- Kubernetes sidecar and leader/follower examples
- remote SDK driver through `THINGD_URL`
- first-pass `thingd` admin/operator CLI with local and remote JSON output

## Important Boundaries

- The default public SDK path is still the TypeScript in-memory proof store.
- Durable local persistence is available through `driver: "native"` after the
  private native package has been built locally.
- Sidecar mode is available through `THINGD_URL` and the remote SDK driver.
- Follower bridge mode forwards MCP traffic to the leader, but follower local
  replica catch-up is not implemented.
- The project is not production-ready yet.
- Do not expose SQL as the public API or MCP interface.

## Key Docs

- [README.md](../README.md) is the public project overview.
- [agent-implementation-guide.md](./agent-implementation-guide.md) is the main
  guide for AI coding agents and future contributors.
- [cli.md](./cli.md) describes current runtime CLIs and the next admin CLI
  phases.
- [sidecar-cluster.md](./sidecar-cluster.md) explains Kubernetes and bridge
  mode.
- [runtime-env.md](./runtime-env.md) lists runtime environment variables.
- [mcp-server.md](./mcp-server.md) explains MCP tools and runtime behavior.
- [persistence-and-native-bindings.md](./persistence-and-native-bindings.md)
  explains Rust persistence and native binding direction.
- [ai-primitives.md](./ai-primitives.md) plans graph, hybrid search, locks,
  workflows, semantic cache, tool ledger, and compaction.
- [benchmarks.md](./benchmarks.md) documents benchmark commands and baselines.
- [release.md](./release.md) explains semantic-release and npm publishing.

## Current Runtime Commands

```bash
pnpm build
pnpm test:cli
pnpm serve:mcp
pnpm smoke:mcp
pnpm smoke:docker
```

Direct MCP runtime commands:

```bash
node packages/thingd-cli/dist/index.js mcp --path :memory:

THINGD_AUTH_TOKEN=change-me \
node packages/thingd-cli/dist/index.js mcp-http --path ./thingd.db --driver native
```

App sidecar usage:

```bash
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=change-me
```

```ts
import { ThingD } from "thingd";

const db = await ThingD.open();
```

## Required Checks

Run the local Node/package gate:

```bash
pnpm check
pnpm build
pnpm test:local
```

If Rust is available:

```bash
pnpm rust:fmt:check
pnpm rust:clippy
pnpm test:rust
```

For runtime smoke:

```bash
pnpm smoke:mcp
pnpm smoke:docker
```

## Recommended Next Phase

Start **Phase CLI-B** from [cli.md](./cli.md).

Goal:

- add pretty table output
- add `thingd doctor`
- add queue stats
- add object and event summary commands
- add benchmark wrapper commands
- improve auth, connection refused, and missing native binding errors
- update docs and tests

The first-pass CLI is already in place. CLI-B should make it nicer to use
before building an inspector UI.

## Later Phases

After CLI-B:

1. CLI-C: export/import, snapshots, and redaction-friendly handoff flows.
2. Native release: prebuild strategy and package loading hardening.
3. Search: SQLite FTS, metadata filters, object-to-text indexing.
4. Sidecar hardening: Kubernetes discovery, follower catch-up, failover tests.
5. AI primitives: graph links, locks, workflow DAGs, semantic cache, tool ledger.

Keep phases small and update this file whenever the recommended next phase
changes.
