# Project Handoff

Quick restart point for future work on `thingd`.

Last updated: 2026-05-30.

## Recommended next phase

**Phase 1 — CLI-B** from [cli.md](./cli.md). Full ordered plan:
[roadmap.md](./roadmap.md).

After CLI-B: **Phase 2** native prebuilds, **Phase 3** Search-A (FTS), **Phase 4**
agent pattern docs/examples.

## Current shape

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

## Important boundaries

- The default public SDK path is still the TypeScript in-memory proof store.
- Durable local persistence is available through `driver: "native"` after the
  private native package has been built locally.
- Sidecar mode is available through `THINGD_URL` and the remote SDK driver.
- Follower bridge mode forwards MCP traffic to the leader, but follower local
  replica catch-up is not implemented.
- Search is substring-over-serialized JSON until Phase 3 (Search-A); see
  [roadmap.md](./roadmap.md#current-behavior-honest-baseline).
- The project is not production-ready yet.
- Do not expose SQL as the public API or MCP interface.

## Key docs

- [README.md](../README.md) — public overview
- [roadmap.md](./roadmap.md) — **canonical build order and exit criteria**
- [doc-maintenance.md](./doc-maintenance.md) — what to update when you change code
- [why-agents.md](./why-agents.md) — agent value proposition
- [agent-patterns.md](./agent-patterns.md) — memory, scheduler, idempotency patterns
- [agent-implementation-guide.md](./agent-implementation-guide.md) — integration for agents/contributors
- [cli.md](./cli.md) — CLI phases (CLI-B next)
- [mcp-server.md](./mcp-server.md) — MCP tools and runtime
- [sidecar-cluster.md](./sidecar-cluster.md) — Kubernetes and bridge mode
- [runtime-env.md](./runtime-env.md) — environment variables
- [persistence-and-native-bindings.md](./persistence-and-native-bindings.md) — Rust persistence
- [ai-primitives.md](./ai-primitives.md) — graph, hybrid search, workflows (phases 8–13)
- [benchmarks.md](./benchmarks.md) — benchmark commands
- [release.md](./release.md) — semantic-release and npm

## Default drivers

| Entry point | Default driver | Default path |
| --- | --- | --- |
| `ThingD.open()` from npm (today) | memory | n/a |
| `thingd mcp` / `mcp-http` | native (when built) | `~/.thingd/data.db` |
| `THINGD_URL` set | remote | sidecar |

## Current runtime commands

```bash
pnpm build
pnpm test:cli
pnpm serve:mcp
pnpm smoke:mcp
pnpm smoke:docker
```

```bash
node packages/thingd-cli/dist/index.js install
node packages/thingd-cli/dist/index.js mcp --driver native
THINGD_AUTH_TOKEN=change-me \
  node packages/thingd-cli/dist/index.js mcp-http --driver native
```

## Required checks

```bash
pnpm check
pnpm build
pnpm test:local
```

Rust (if installed):

```bash
pnpm rust:fmt:check
pnpm rust:clippy
pnpm test:rust
```

Runtime smoke:

```bash
pnpm smoke:mcp
pnpm smoke:docker
```

## Doc hygiene

Before merging user-facing changes, read [doc-maintenance.md](./doc-maintenance.md).
Update [roadmap.md](./roadmap.md) when the recommended phase changes.
