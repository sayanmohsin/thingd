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

## Pattern 6 — Multi-agent blackboard (Shared state & facts)

Use a shared object collection as a **Blackboard** to coordinate agent state, capabilities, and gathered facts without direct messaging.

### Flow:
1. **Agent Registration**: Each active agent registers its state and capabilities into an `"agents"` collection:
   ```json
   // thing_put into "agents"
   {
     "id": "researcher-agent",
     "status": "idle",
     "specialty": "web-search-and-summarization"
   }
   ```
2. **Fact Compilation**: An agent processes raw information and writes structured findings into a `"shared_facts"` collection.
3. **Retrieval**: Other agents query the blackboard via `thing_search` or `thing_objects_list` to fetch the updated context:
   ```json
   // thing_search
   { "query": "latest research results", "collections": ["shared_facts"] }
   ```

## Pattern 7 — Multi-agent task handoff (Safe queues)

Use queues to delegate work dynamically among multiple specialized worker agents. `thingd`'s built-in lease management guarantees **at-most-once processing concurrency** (no two agents will process the same task simultaneously).

### Flow:
1. **Coordinator Agent** delegates work:
   - Pushes a job to `"code_review"` using `thing_queue_push`.
2. **Worker Agents** continuously poll or claim ready tasks:
   - Claims task using `thing_queue_claim` with `leaseMs: 30000`.
   - The task transitions to `"leased"`, ensuring other worker agents skip it.
3. **Worker Agent** completes the task, writes the results to the `"reviews"` collection, and runs `thing_queue_ack` to clear it.

## Pattern 8 — Event-driven pub/sub (Signaling & coordination)

Use event streams as a lightweight publish/subscribe bus to coordinate agent actions asynchronously.

### Flow:
1. **Publisher Agent**: Emits status or lifecycle signals to a shared event stream (e.g., `activity:session_123`):
   ```json
   // thing_events_append
   {
     "stream": "activity:session_123",
     "event": {
       "type": "draft_completed",
       "author": "writer-agent",
       "file": "draft.md"
     }
   }
   ```
2. **Subscriber Agent**: Regularly lists the stream using `thing_events_list` to discover new events. When it detects a `"draft_completed"` event type, it fires its own follow-up workflow (e.g. proofreading).

## Anti-patterns

- Storing large blobs without chunking — use object refs + queue jobs to process
- Relying on search for exact id lookup — use `thing_get`
- Expecting exactly-once queue delivery — use ack/nack and idempotency
- Assuming `ThingD.open("./file.db")` persists without `driver: "native"`

## Examples & Quickstarts

We have created fully functional, runnable examples demonstrating these agent patterns inside the [examples/cursor-agent-memory/](file:///Users/sayanmohsin/Space/Programming/personal/thingd/examples/cursor-agent-memory/) directory:

1. **[.cursorrules](file:///Users/sayanmohsin/Space/Programming/personal/thingd/examples/cursor-agent-memory/.cursorrules)**: Drop-in system configuration rules instructing AI subagents on search-before-put conventions, transaction auditing, and blackboard task queues.
2. **[quickstart.ts](file:///Users/sayanmohsin/Space/Programming/personal/thingd/examples/cursor-agent-memory/quickstart.ts)**: A runnable script demonstrating automatic native SQLite driver promotion, FTS5 stemming queries (e.g. searching `"learning"` matches `"learn"`), and custom JSON metadata filters.
3. **[scheduler-heartbeat.ts](file:///Users/sayanmohsin/Space/Programming/personal/thingd/examples/cursor-agent-memory/scheduler-heartbeat.ts)**: A runnable task coordinator executing the **Schedules collection + Scheduler queue** heartbeat cron-like pattern.
