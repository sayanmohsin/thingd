# Roadmap

Single source of truth for build order, doc updates, and exit criteria. When the
recommended next phase changes, update this file first, then
[handoff.md](./handoff.md) and the short summary in [README.md](../README.md).

Last updated: 2026-05-30.

## North star (next ~8 weeks)

> Install `thingd`, get durable local memory in under a minute, search it
> usefully, run background jobs with queues, and let Cursor/agents use MCP without
> guessing drivers or schemas.

Cluster replication, workflow DAGs, and vector search stay important but come
**after** native default, real search, and operator CLI polish.

## Current behavior (honest baseline)

| Area | Today | Target |
| --- | --- | --- |
| SDK default | TypeScript in-memory proof store | Native SQLite when prebuild available |
| `thingd mcp` default | Native → `~/.thingd/data.db` | Same; document clearly |
| Search | Substring over serialized JSON (objects + events) | SQLite FTS + metadata filters |
| MCP tools | `thing_*` namespace | Stable; no `memory.*` aliases until designed |
| Scheduler | Pattern on queues + external heartbeat | Documented in [agent-patterns.md](./agent-patterns.md) |
| Cluster | Leader follower MCP forward | Follower replica catch-up later |

## Phase map

| Phase | Name | Est. effort | Primary packages |
| --- | --- | --- | --- |
| 1 | CLI-B operator polish | 1–2 days | `thingd-cli` |
| 2 | Native release path | ~1 week | `thingd-native`, `thingd`, release |
| 3 | Search-A (FTS + metadata) | 1–2 weeks | `thingd-core`, `thingd`, MCP |
| 4 | Agent patterns (docs + examples) | 3–5 days | `docs/`, `examples/` |
| 5 | MCP hardening | ~1 week | `thingd-cli`, MCP |
| 6 | CLI-C data movement | 1–2 days | `thingd-cli` |
| 7 | Sidecar hardening | 1–2 weeks | `thingd-cli`, `deploy/` |
| 8–13 | AI-native primitives | ongoing | `thingd-core`, SDK, MCP |

Phases 8–13 match the priority order in [ai-primitives.md](./ai-primitives.md).

---

## Phase 1 — CLI-B (operator polish)

**Status:** next up  
**Detail:** [cli.md](./cli.md#phase-cli-b---operator-polish)

### Deliverables

- [ ] `thingd doctor` (Node, native binding, remote reachability, auth)
- [ ] Pretty table output (`--pretty` without raw JSON only)
- [ ] `thingd queues stats <queue>`
- [ ] `thingd collections list`, `thingd objects list <collection>`
- [ ] `thingd events streams`
- [ ] `thingd bench rust --smoke` / `--count <n>` wrappers
- [ ] Clear errors: missing native, connection refused, 401

### Exit criteria

- `pnpm test:cli` passes
- New commands documented in `docs/cli.md`, `packages/thingd-cli/README.md`, README tool list if surfaced

### Doc / CLI checklist

| Touch | Update |
| --- | --- |
| New CLI command | `docs/cli.md`, `packages/thingd-cli/README.md`, `packages/thingd-cli/src/index.ts` help text |
| Connection defaults | `docs/runtime-env.md`, `docs/mcp-server.md` if env-related |
| Tests | `packages/thingd-cli/test/` |

---

## Phase 2 — Native release path

**Status:** after CLI-B  
**Detail:** [persistence-and-native-bindings.md](./persistence-and-native-bindings.md)

### Deliverables

- [ ] Prebuild strategy (darwin/linux, arm64 + x64)
- [ ] SDK loads native when available; documented fallback to memory + warning
- [ ] `thingd install` reports active driver and binding status
- [ ] `thingd doctor` checks prebuild vs local build
- [ ] Release docs: npm publish path without `pnpm --filter thingd-native build`

### Exit criteria

- Clean machine: `npm install thingd` + `thingd mcp` persists without manual native build
- README v0.1 checkbox: native prebuild/release strategy

### Doc / CLI checklist

| Touch | Update |
| --- | --- |
| Native loading | `docs/persistence-and-native-bindings.md`, README Installation + Status |
| Default driver table | README, [handoff.md](./handoff.md), `docs/agent-implementation-guide.md` |
| Release | `docs/release.md` |
| CLI install/doctor | `docs/cli.md`, `packages/thingd-cli/README.md` |

---

## Phase 3 — Search-A (agent memory)

**Status:** after Phase 2  
**Unlocks:** credible “search local memory” in vision and README

### Deliverables

- [ ] `search_documents` (or equivalent) table in SQLite adapter
- [ ] Index object text on put/delete/update
- [ ] SQLite FTS5 (or approved FTS approach) in `crates/thingd-core`
- [ ] Metadata filters in SDK + `thing_search` MCP
- [ ] Simple recency scoring
- [ ] Rust + Node tests; benchmark note in `docs/benchmarks.md`

### Exit criteria

- Keyword search finds records by field content, not accidental JSON key matches
- README v0.2 checkboxes: full-text, metadata filters, object-to-text indexing

### Doc / CLI checklist

| Touch | Update |
| --- | --- |
| Search API | `packages/thingd/src/types.ts`, README Search section, **remove** “substring only” caveat |
| MCP | `docs/mcp-server.md`, tool descriptions if behavior changes |
| CLI | `thingd search` help + filters flags |
| Agent guide | `docs/agent-implementation-guide.md` |

---

## Phase 4 — Agent patterns (docs + examples)

**Status:** can start in parallel with Phase 3 (docs only)  
**Detail:** [agent-patterns.md](./agent-patterns.md), [why-agents.md](./why-agents.md)

### Deliverables

- [ ] Agent quickstart (install → MCP → put/search/queue in 5 minutes)
- [ ] Scheduler pattern: `schedules` collection + `scheduler` queue + heartbeat
- [ ] Example Cursor rule snippet (`examples/cursor-agent-memory/`)
- [ ] “Search before put” convention for agents
- [ ] Optional: sample seed script matching MCP tutorial

### Exit criteria

- New contributor can answer “why thingd for agents?” from docs alone
- No new Rust required for doc-only slice

### Doc / CLI checklist

| Touch | Update |
| --- | --- |
| New examples | `examples/*/README.md`, root README Examples |
| MCP install | `docs/mcp-server.md` links quickstart |
| handoff + README | Link `why-agents.md`, `agent-patterns.md` |

---

## Phase 5 — MCP hardening

**Status:** after Search-A

### Deliverables

- [ ] Collection allowlist / read-only MCP mode (env-driven)
- [ ] Payload size limits for HTTP MCP
- [ ] MCP resources (e.g. list collections) — design in `docs/mcp-server.md` first
- [ ] Documented security defaults for non-loopback bind

### Exit criteria

- `docs/mcp-server.md` “Not implemented” list updated
- Smoke tests for auth + allowlist

### Doc / CLI checklist

| Touch | Update |
| --- | --- |
| Env vars | `docs/runtime-env.md`, `docs/mcp-server.md`, deploy READMEs |
| Docker/K8s | `docs/docker-runtime.md`, `deploy/` |

---

## Phase 6 — CLI-C (data movement)

**Status:** after Search-A + stable list/pagination semantics  
**Detail:** [cli.md](./cli.md#phase-cli-c---data-movement)

### Deliverables

- [ ] export/import JSONL (objects, events)
- [ ] snapshot create/restore for local dev
- [ ] Redaction hooks documented for agent exports

### Doc / CLI checklist

| Touch | Update |
| --- | --- |
| All CLI-C commands | `docs/cli.md`, `packages/thingd-cli/README.md`, tests |

---

## Phase 7 — Sidecar hardening

**Status:** when Kubernetes/multi-pod is a real need  
**Detail:** [sidecar-cluster.md](./sidecar-cluster.md)

### Deliverables

- [ ] Follower local replica catch-up (or explicit “won’t do yet”)
- [ ] Failover / leader election story (static config minimum)
- [ ] Published Docker image + versioned tags
- [ ] Integration tests for bridge forward + auth

### Doc / CLI checklist

| Touch | Update |
| --- | --- |
| Cluster | `docs/sidecar-cluster.md`, `deploy/`, README multi-pod section |
| `thingd status` | cluster fields documented in `docs/cli.md` |

---

## Phases 8–13 — AI-native primitives

Aligned with [ai-primitives.md](./ai-primitives.md). Do not start Phase 9 vectors until Phase 3 Search-A ships.

| Phase | Primitive | Focus |
| --- | --- | --- |
| 8 | Graph links | Rust trait, SQLite `links`, SDK `db.links`, MCP read tools |
| 9 | Hybrid search | Vectors + graph expansion on top of FTS |
| 10 | Locks & semaphores | Coordination, queue heartbeats |
| 11 | Workflow DAGs | Multi-step pipelines on queue substrate |
| 12 | Semantic cache + tool ledger | Cost/latency, audit |
| 13 | Compaction & snapshots | Long-running memory |

Each primitive phase must update: `ai-primitives.md`, `architecture.md`, README “later” checklist, MCP tool list, `packages/thingd/src/types.ts`, tests.

---

## Doc maintenance (every phase)

Read [doc-maintenance.md](./doc-maintenance.md) before merging. Minimum on any user-facing change:

```bash
pnpm check
pnpm build
pnpm test:local
# plus test:cli if CLI touched, test:rust if Rust touched
```

## Version labels (informal)

- **v0.1** — local core + native path (Phase 2 completes prebuild story)
- **v0.2** — agent memory (Phase 3 search + Phase 4 docs)
- **v0.3** — production shape (Phase 5–7, inspector UI optional later)

README checkboxes stay as high-level markers; this file owns ordering.
