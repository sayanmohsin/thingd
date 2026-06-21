# Agent Patterns

Recipes for using `thingd` with Cursor, Claude, and other MCP clients.
Assumes MCP tools from [mcp-server.md](./mcp-server.md) are enabled.

**New here?** Start with the [5-minute quickstart](./QUICKSTART.md) first.

---

## Convention: search before put

Before `thing_put`, call `thing_search` with the entity name or id. Reduces
duplicate memories and conflicting records.

Suggested Cursor / Claude rule (included in `.cursorrules`):

```txt
When using thingd MCP, always call thing_search before thing_put for the same
entity. Prefer updating via put with the same id over creating duplicates.
```

---

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

---

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
  "recurringIntervalMs": null
}
```

### Queue: `scheduler`

When `runAt <= now` and `enabled === true`:

```txt
thing_queue_push → queue: "scheduler"
payload: { scheduleId, action, ...payload }
delayMs: 0   (or ms until runAt)
idempotencyKey: "schedule:<id>:<runAt>"
```

### Heartbeat (pick one)

- Cursor Automation on an interval
- CLI cron: list due schedules, push jobs
- Agent session on `/loop` claiming the `scheduler` queue

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

**Runnable example:** [`examples/cursor-agent-memory/scheduler-heartbeat.ts`](../examples/cursor-agent-memory/scheduler-heartbeat.ts)

---

## Pattern 3 — Idempotent background worker

Queues are at-least-once. Always set `idempotencyKey` on push. Consumers should
treat duplicate claims as safe no-ops when work is already completed.

```txt
idempotencyKey: "embed:docs/doc_123:v1"
```

---

## Pattern 4 — Audit-friendly writes

Pass `actor` and `source` on MCP writes when multiple agents or apps share a
store:

```json
{
  "actor": "cursor-agent",
  "source": "nightly-review"
}
```

Inspect `__thingd:mcp:audit` via `thing_events_list` or the **inspector dashboard**:

```bash
thingd dashboard
```

---

## Pattern 5 — Sidecar + local app

App container sets:

```txt
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=...
```

App uses `ThingD.open()`; agents use the same sidecar MCP endpoint. One SQLite
file per pod (leader writes in cluster mode).

---

## Pattern 6 — Multi-agent blackboard (shared state & facts)

Use a shared object collection as a **Blackboard** to coordinate agent state, capabilities, and gathered facts without direct messaging.

### Flow

1. **Agent Registration** — each active agent registers into the `agents` collection:
   ```json
   {
     "id": "researcher-agent",
     "status": "idle",
     "specialty": "web-search-and-summarization"
   }
   ```
2. **Fact Compilation** — agents write structured findings to `shared_facts`.
3. **Retrieval** — other agents query the blackboard:
   ```json
   { "query": "latest research results", "collections": ["shared_facts"] }
   ```

---

## Pattern 7 — Multi-agent task handoff (safe queues)

Queues provide **at-most-once processing concurrency**: no two agents process the same job simultaneously.

### Flow

1. **Coordinator** pushes work to a named queue with `thing_queue_push`.
2. **Workers** claim via `thing_queue_claim` with `leaseMs: 30000`. The leased job is invisible to other workers.
3. **Worker** writes results, then calls `thing_queue_ack` to clear the job.
4. On failure, `thing_queue_nack` with a `delayMs` schedules a retry.

---

## Pattern 8 — Event-driven pub/sub (signaling & coordination)

Use event streams as a lightweight publish/subscribe bus.

### Flow

1. **Publisher** emits status or lifecycle signals:
   ```json
   {
     "stream": "activity:session_123",
     "event": { "type": "draft_completed", "author": "writer-agent", "file": "draft.md" }
   }
   ```
2. **Subscriber** polls via `thing_events_list` for new events on the stream and fires follow-up workflows when specific types appear.

---

## Pattern 9 — Session context reload

At the start of every agent session, reload prior context before making decisions:

```txt
thing_search     { query: "<project>", collections: ["memories", "decisions", "tasks"] }
thing_events_list { stream: "project:<id>", limit: 20 }
thing_queue_list  { queue: "scheduler" }
```

This gives you current truth (objects), history (events), and pending work (queue) — without hallucinating stale state from prior sessions.

---

## Pattern 10 — Bulk operations

When processing multiple records, use batch operations to reduce round trips:

```txt
thing_objects_put_batch  { collection: "tasks", objects: [{ id: "t1", ... }, { id: "t2", ... }] }
thing_objects_delete_batch { collection: "tasks", ids: ["t1", "t2"] }
```

Batch operations are atomic within the collection — all items succeed or all fail. Use them for data migration, bulk imports, or cleaning up stale records.

---

## Pattern 11 — Paginated task triage

When reviewing large collections, use sort + filter + pagination to work through items systematically:

```txt
thing_objects_list { collection: "tasks", filter: { status: "active" }, sortBy: { field: "created_at", direction: "desc" }, limit: 10, offset: 0 }
```

Process the first page, then increment `offset` by `limit` for the next page. Combine with `thing_search` for keyword-based filtering.

---

## Anti-patterns

- Storing large blobs without chunking — use object refs + queue jobs to process
- Relying on search for exact id lookup — use `thing_get`
- Expecting exactly-once queue delivery — use ack/nack and idempotency keys
- Assuming `ThingD.open("./file.db")` persists without `driver: "native"`
- Creating a new collection per session — use stable shared collection names
- Skipping `thing_search` before `thing_put` — causes duplicate, conflicting records

---

## Examples & Quickstarts

Fully runnable examples in [`examples/cursor-agent-memory/`](../examples/cursor-agent-memory/):

1. **[.cursorrules](../examples/cursor-agent-memory/.cursorrules)** — drop-in system rules for Cursor/Claude agents enforcing all conventions above.
2. **[quickstart.ts](../examples/cursor-agent-memory/quickstart.ts)** — native SQLite driver, FTS5 stemming search, metadata filters.
3. **[scheduler-heartbeat.ts](../examples/cursor-agent-memory/scheduler-heartbeat.ts)** — full Schedules + Queue + Heartbeat scheduler pattern.

5-minute install guide: **[docs/QUICKSTART.md](./QUICKSTART.md)**

Full API reference: **[docs/api-spec/](./api-spec/)**
