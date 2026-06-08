# Roadmap

Single source of truth for build order, doc updates, and exit criteria. When the
recommended next phase changes, update this file first, then
[handoff.md](./handoff.md) and the short summary in [README.md](../README.md).

Last updated: 2026-05-31.

## North star (next ~8 weeks)

> Install `thingd`, get durable local memory in under a minute, search it
> usefully, run background jobs with queues, and let Cursor/agents use MCP without
> guessing drivers or schemas.

Cluster replication, workflow DAGs, and vector search stay important but come
**after** native default, real search, and operator CLI polish.

## Current behavior (honest baseline)

| Area | Today | Target |
| --- | --- | --- |
| SDK default | Native SQLite on path, Memory fallback | Native SQLite when prebuild available |
| `thingd mcp` default | Native → `~/.thingd/data.db` | Same; document clearly |
| Search | SQLite FTS + metadata filters | SQLite FTS + metadata filters |
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
| 5 | Inspector Dashboard UI | 3–5 days | `thingd-cli`, static |
| 6 | MCP hardening | ~1 week | `thingd-cli`, MCP |
| 7 | CLI-C data movement | 1–2 days | `thingd-cli` |
| 8 | Sidecar hardening | 1–2 weeks | `thingd-cli`, `deploy/` |
| 9–14 | AI-native primitives | ongoing | `thingd-core`, SDK, MCP |

Phases 8–13 match the priority order in [ai-primitives.md](./ai-primitives.md).

---

## Phase 1 — CLI-B (operator polish)

**Status:** completed  
**Detail:** [cli.md](./cli.md#phase-cli-b---operator-polish)

### Deliverables

- [x] `thingd doctor` (Node, native binding, remote reachability, auth)
- [x] Pretty table output (`--pretty` without raw JSON only)
- [x] `thingd queues stats <queue>`
- [x] `thingd collections list`, `thingd objects list <collection>`
- [x] `thingd events streams`
- [x] `thingd bench rust --smoke` / `--count <n>` wrappers
- [x] Clear errors: missing native, connection refused, 401

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

**Status:** completed  
**Detail:** [persistence-and-native-bindings.md](./persistence-and-native-bindings.md)

### Deliverables

- [x] Prebuild strategy (darwin/linux, arm64 + x64)
- [x] SDK loads native when available; documented fallback to memory + warning
- [x] `thingd install` reports active driver and binding status
- [x] `thingd doctor` checks prebuild vs local build
- [x] Release docs: npm publish path without `pnpm --filter thingd-native build`

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

**Status:** completed  
**Unlocks:** credible “search local memory” in vision and README

### Deliverables

- [x] `search_documents` (or equivalent) table in SQLite adapter
- [x] Index object text on put/delete/update
- [x] SQLite FTS5 (or approved FTS approach) in `crates/thingd-core`
- [x] Metadata filters in SDK + `thing_search` MCP
- [x] Simple recency scoring
- [x] Rust + Node tests; benchmark note in `docs/benchmarks.md`

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

**Status:** completed  
**Detail:** [agent-patterns.md](./agent-patterns.md), [why-agents.md](./why-agents.md), [QUICKSTART.md](./QUICKSTART.md)

### Deliverables

- [x] Agent quickstart (install → MCP → put/search/queue in 5 minutes) — `docs/QUICKSTART.md`
- [x] Scheduler pattern: `schedules` collection + `scheduler` queue + heartbeat
- [x] Example Cursor rule snippet (`examples/cursor-agent-memory/.cursorrules`)
- [x] "Search before put" convention for agents
- [x] Multi-agent coordination patterns (blackboard, task handoff, event pub/sub)
- [x] Session context reload pattern
- [x] Runnable example scripts (`quickstart.ts`, `scheduler-heartbeat.ts`)

### Exit criteria

- New contributor can answer "why thingd for agents?" from docs alone ✅
- No new Rust required for doc-only slice ✅

### Doc / CLI checklist

| Touch | Update |
| --- | --- |
| New examples | `examples/cursor-agent-memory/README.md` updated |
| Agent quickstart | `docs/QUICKSTART.md` created |
| Agent patterns | `docs/agent-patterns.md` Pattern 9 added, all links updated |
| Why agents | `docs/why-agents.md` quickstart link, dashboard section |
| handoff + README | `docs/handoff.md` updated, root README updated |

---

## Phase 5 — Inspector Dashboard UI

**Status:** completed  
**Unlocks:** visual local developer inspection and "phpMyAdmin-style" queue supervision

### Deliverables

- [x] Svelte 5 + Vite frontend inside `packages/thingd-cli/src/dashboard/frontend/`
- [x] REST API server routing in `packages/thingd-cli/src/dashboard/server.ts`
- [x] `dashboard` command in `thingd-cli` that auto-opens browser
- [x] Dashboard integration tests (auth gate, connection swap, static assets)
- [x] Glassmorphic dark-mode UI: metrics, collections, events, FTS5 search, queue supervision
- [x] Structured JSON editor (raw + visual builder) for payload and metadata fields
- [x] Dashboard auth gate via `THINGD_DASHBOARD_TOKEN` env
- [x] Connection settings modal (swap driver/path live)

### Exit criteria

- `thingd dashboard` opens a responsive dark-mode glassmorphic dashboard ✅
- Dashboard covers all collections, event logs, FTS5 stemming queries, and queue jobs ✅

---

## Phase 6 — MCP hardening

**Status:** completed

### Deliverables

- [x] Collection allowlist / read-only MCP mode — `THINGD_MCP_COLLECTIONS`, `THINGD_MCP_READ_ONLY` (stdio + HTTP)
- [x] Payload size limits for HTTP MCP — `THINGD_MCP_MAX_PAYLOAD_BYTES` (default 512 KB, Content-Length fast-path)
- [x] MCP resources — `thingd://collections` via `resources/list` (allowlist-filtered)
- [x] Documented security defaults — `docs/mcp-server.md` MCP Hardening section, `docs/runtime-env.md`

### Exit criteria

- `docs/mcp-server.md` Not-implemented list updated ✅
- Smoke tests for auth + allowlist ✅ (31/31 pass)

### Doc / CLI checklist

| Touch | Update |
| --- | --- |
| Env vars | `docs/runtime-env.md` — MCP Hardening section added |
| MCP server | `docs/mcp-server.md` — hardening + resources sections added |

---

## Phase 7 — CLI-C (data movement)

**Status:** completed  
**Detail:** [cli.md](./cli.md#phase-cli-c---data-movement)

### Deliverables

- [x] export/import JSONL (objects, events)
- [x] snapshot create/restore for local dev
- [x] Redaction hooks documented for agent exports

### Doc / CLI checklist

| Touch | Update |
| --- | --- |
| All CLI-C commands | `docs/cli.md`, `packages/thingd-cli/README.md`, tests |

---

## Phase 8 — Sidecar hardening

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

## Phases 9–14 — AI-native primitives

Aligned with [ai-primitives.md](./ai-primitives.md). Do not start Phase 10 vectors until Phase 3 Search-A ships.

| Phase | Primitive | Focus |
| --- | --- | --- |
| 9 | Graph links | Rust trait, SQLite `links`, SDK `db.links`, MCP read tools |
| 10 | Hybrid search | Vectors + graph expansion on top of FTS |
| 11 | Locks & semaphores | Coordination, queue heartbeats |
| 12 | Workflow DAGs | Multi-step pipelines on queue substrate |
| 13 | Semantic cache + tool ledger | Cost/latency, audit |
| 14 | Compaction & snapshots | Long-running memory |

Each primitive phase must update: `ai-primitives.md`, `architecture.md`, README “later” checklist, MCP tool list, `packages/thingd/src/types.ts`, tests.

---

## Doc maintenance (every phase)

Read [doc-maintenance.md](./doc-maintenance.md) before merging. When positioning, tradeoffs, or hard questions change, update [faq.md](./faq.md). Minimum on any user-facing change:

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
