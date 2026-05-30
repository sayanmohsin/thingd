# Agent Patterns

Recipes for using `thingd` with Cursor, Claude, and other MCP clients. Assumes
MCP tools from [mcp-server.md](./mcp-server.md) are enabled.

## Convention: search before put

Before `thing_put`, call `thing_search` with the entity name or id. Reduces
duplicate memories and conflicting records.

Suggested Cursor user rule:

```txt
When using thingd MCP, always thing_search before thing_put for the same entity.
Prefer updating via put with the same id over creating duplicates.
```

## Pattern 1 — Project memory

**Collections:**

```txt
projects     — id, name, status, summary
decisions    — id, projectId, text, confidence
tasks        — id, projectId, title, status, dueAt
```

**Events:** stream `project:<id>` for milestones and decision events.

**Example flow:**

1. `thing_put` decision with `project: "thingd"`
2. `thing_events_append` on `project:thingd` with `type: "decision.made"`
3. `thing_search` before next session to reload context

## Pattern 2 — Scheduler (no built-in cron)

`thingd` does not run timers. Use **objects + queue + external heartbeat**.

### Schema: `schedules` collection

```json
{
  "id": "reminder-001",
  "runAt": "2026-05-30T10:00:00Z",
  "action": "send_summary",
  "payload": { "projectId": "thingd" },
  "enabled": true,
  "recurring": null
}
```

### Queue: `scheduler`

When `runAt <= now` and `enabled`:

```txt
thing_queue_push → queue: "scheduler"
payload: { scheduleId, action, ...payload }
delayMs: 0   (or ms until runAt)
idempotencyKey: "schedule:<id>:<runAt>"
```

### Heartbeat (pick one)

- Cursor Automation on interval
- `thingd` CLI in cron: list due schedules, push jobs
- Agent session on `/loop` claiming `scheduler` queue

### Worker

```txt
thing_queue_claim → queue: "scheduler"
→ run action
→ thing_queue_ack
→ if recurring: thing_put schedule with next runAt
```

### One-shot delay only

For simple delays, skip the `schedules` collection:

```txt
thing_queue_push with delayMs: 3600000
```

## Pattern 3 — Idempotent background worker

Queues are at-least-once. Always set `idempotencyKey` on push. Consumers should
treat duplicate claims as safe no-ops when work already completed.

```txt
idempotencyKey: "embed:docs/doc_123:v1"
```

## Pattern 4 — Audit-friendly writes

Pass `actor` and `source` on MCP writes when multiple agents or apps share a
store:

```json
{
  "actor": "cursor-agent",
  "source": "nightly-review"
}
```

Inspect `__thingd:mcp:audit` via `thing_events_list` or `thing_search`.

## Pattern 5 — Sidecar + local app

App container sets:

```txt
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=...
```

App uses `ThingD.open()`; agents use the same sidecar MCP endpoint. One SQLite
file per pod (leader writes in cluster mode).

## Anti-patterns

- Storing large blobs without chunking — use object refs + queue jobs to process
- Relying on search for exact id lookup — use `thing_get`
- Expecting exactly-once queue delivery — use ack/nack and idempotency
- Assuming `ThingD.open("./file.db")` persists without `driver: "native"`

## Examples (planned)

Phase 4 in [roadmap.md](./roadmap.md) adds `examples/cursor-agent-memory/` with
a seed script and sample collections matching these patterns.
