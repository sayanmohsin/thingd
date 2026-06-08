# Documentation Maintenance

Use this checklist whenever you change behavior in `thingd`. The goal is one
story across README, CLI help, MCP tools, and handoff docs.

Master build order: [roadmap.md](./roadmap.md).

## Quick matrix

| You changed… | Update these |
| --- | --- |
| **Public SDK API** (`packages/thingd`) | `packages/thingd/src/types.ts`, `packages/thingd/README.md`, root `README.md` (Example API, Status), `docs/agent-implementation-guide.md`, tests in `packages/thingd/test/` |
| **Store / search behavior** | Above + `docs/mcp-server.md` (current behavior), `docs/architecture.md`, `docs/benchmarks.md` if perf-related |
| **Rust core / SQLite** | `crates/thingd-core/README.md`, `docs/persistence-and-native-bindings.md`, `docs/architecture.md`, `pnpm test:rust` |
| **Native binding** | `packages/thingd-native/README.md`, `docs/persistence-and-native-bindings.md`, README Installation, `thingd install` / `doctor` docs |
| **MCP tool name / params** | `docs/mcp-server.md`, MCP server source in `packages/thingd-cli`, Cursor/Claude install output if affected, **do not** rename without updating `docs/vision.md` and `docs/ai-primitives.md` |
| **CLI command / flag** | `packages/thingd-cli/src/index.ts` help, `docs/cli.md`, `packages/thingd-cli/README.md`, `pnpm test:cli` |
| **Env var** | `docs/runtime-env.md`, `docs/mcp-server.md`, `docs/docker-runtime.md`, `docs/sidecar-cluster.md`, deploy READMEs |
| **Cluster / Docker / K8s** | `docs/sidecar-cluster.md`, `docs/docker-runtime.md`, `deploy/`, root README sidecar section |
| **Release / publish** | `docs/release.md`, `release.config.mjs`, root README Releases |
| **Roadmap / phase** | `docs/roadmap.md` first, then `docs/handoff.md`, README Roadmap summary |
| **Positioning / FAQ** | `docs/faq.md`, root `README.md` tagline and description |
| **Agent integration** | `docs/agent-implementation-guide.md`, `docs/agent-patterns.md`, `docs/why-agents.md` |
| **Agent setup / MCP connection** | `docs/agent-setup.md`, landing page (`docs/index.html`), `docs/QUICKSTART.md` |

## MCP tool naming

Public tools use the `thing_*` prefix:

```txt
thing_search, thing_get, thing_put, thing_delete
thing_events_*, thing_queue_*
```

Do not document `memory.*` aliases in user-facing guides unless they are
implemented. Planned primitives in [ai-primitives.md](./ai-primitives.md) should
use `thing_*` or `thing_links_*` style names consistent with existing tools.

## “Current vs planned” callouts

If behavior is still a proof or stub, say so in:

1. Root `README.md` Status or feature section
2. `docs/mcp-server.md` (Current Status)
3. `docs/handoff.md` (Important Boundaries)

Remove the caveat when the phase exit criteria in [roadmap.md](./roadmap.md) is met.

## Default driver table (keep in sync)

| Entry point | Default driver | Default path |
| --- | --- | --- |
| `ThingD.open()` from npm (today) | `memory` | n/a |
| `thingd mcp` / `thingd mcp-http` | `native` (when built) | `~/.thingd/data.db` |
| `ThingD.open()` with `THINGD_URL` | `remote` | sidecar URL |

When Phase 2 (native release) changes SDK defaults, update this table everywhere it appears.

## Search behavior

Document as:

> Search is powered by a high-performance database-native SQLite FTS5 virtual table with Porter word stemming, custom metadata key-value filters, and dynamic recency-weighted ranking.

## Required checks before handoff

```bash
pnpm check
pnpm build
pnpm test:local
```

If applicable:

```bash
pnpm test:cli      # CLI changes
pnpm test:mcp      # MCP changes
pnpm test:rust     # Rust changes
pnpm smoke:mcp     # HTTP MCP runtime
pnpm smoke:docker  # Docker runtime
```

## Commit message hints

Use conventional commits so semantic-release works:

- `feat(cli): add thingd doctor`
- `feat(search): sqlite fts index on put`
- `docs: align roadmap and README search caveat`

## Files that must not drift

| Topic | Canonical doc |
| --- | --- |
| Build order | `docs/roadmap.md` |
| Restart for contributors | `docs/handoff.md` |
| CLI commands | `docs/cli.md` |
| MCP tools & runtime | `docs/mcp-server.md` |
| Env vars | `docs/runtime-env.md` |
| AI structures (future) | `docs/ai-primitives.md` |
| Why agents care | `docs/why-agents.md` |
| Patterns (scheduler, memory) | `docs/agent-patterns.md` |
| Hard questions / FAQ | `docs/faq.md` |
