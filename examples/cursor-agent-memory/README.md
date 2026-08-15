# cursor-agent-memory

Fully runnable examples demonstrating how to equip Cursor agents, Claude Desktop, and Node.js apps with durable long-term memory, background scheduling, and multi-agent coordination using `thingd`.

> **Full quickstart guide:** [docs/QUICKSTART.md](../../docs/QUICKSTART.md)

---

## Setup

### 1. Register the MCP server

**Cursor** — run this once and follow the printed instructions:

```bash
thingd install
```

Or add manually: **Settings → Features → MCP → + Add New MCP Tool**

| Field | Value |
|---|---|
| Name | `thingd` |
| Type | `command` |
| Command | `thingd mcp --driver native` |

**Claude Desktop** — `thingd install` writes the config automatically on macOS. Manual entry:

```json
{
  "mcpServers": {
    "thingd": {
      "command": "thingd",
      "args": ["mcp", "--driver", "native"]
    }
  }
}
```

### 2. Verify

```bash
thingd doctor
```

### 3. Inspect with the dashboard

```bash
thingd dashboard
```

Opens `http://localhost:8758` — browse collections, event streams, queue jobs, and run FTS5 search queries interactively.

---

## Runnable Examples

Install dependencies first:

```bash
pnpm install
```

### `quickstart.ts` — SDK Basics

Demonstrates opening a persistent native Thingd database, storing structured memory objects, full-text stemming search (e.g. `"learning"` matches `"learn"`), and metadata filters. RocksDB is the default native backend; experimental ThingDB can be selected with `THINGD_STORAGE_BACKEND=thingdb`.

```bash
pnpm tsx quickstart.ts
```

Expected output:

```
=== [Step 1: Database Open] ===
Opened persistent thingd instance using native driver.

=== [Step 3: Full-Text Stemming Search] ===
Searched for "learning". FTS5 Porter Stemmer matched both "learn-rust" and "learn-typescript".

=== [Step 4: Search with Metadata Filters] ===
Searched for "learning" with filter { status: "active" }. Only "learn-typescript" returned.

🎉 Quickstart completed successfully!
```

### `scheduler-heartbeat.ts` — Scheduler Pattern

Demonstrates **Pattern 2: Objects + Queue + External Heartbeat**. Creates a recurring schedule in the `schedules` collection, pushes due tasks into the `scheduler` queue with idempotency keys, claims them with a lease, runs the action, acknowledges completion, and advances the next `runAt`.

```bash
pnpm tsx scheduler-heartbeat.ts
```

Expected output:

```
=== [Step 1: Register Schedule] ===
Created recurring schedule "nightly-report", marked as immediately due.

=== [Step 2a: Push Task to Queue] ===
Enqueued scheduler job with idempotencyKey "schedule:nightly-report:<runAt>".

=== [Step 3a: Task Claimed] ===
Claimed job. Concurrency lock active for 15s.

=== [Step 3b: Task Acknowledged] ===
Job completed and acknowledged. Cleared from queue.
```

---

## Agent Rules (`.cursorrules`)

Copy `.cursorrules` to your project root to teach agents the memory conventions:

```bash
cp .cursorrules /path/to/your/project/.cursorrules
```

The rules enforce:

| Rule | What it prevents |
|---|---|
| **Search-Before-Put** | Duplicate memory objects with conflicting IDs |
| **Transactional Auditing** | Untraceable mutations; every write gets `actor` + `source` |
| **Queue Safety** | Missing acks, double-processing, jobs silently failing |
| **Scheduler Pattern** | Confusion about how to run recurring work without a cron daemon |
| **Blackboard Coordination** | Agents stepping on each other without shared state conventions |
| **Session Context Reload** | Agents hallucinating stale context at the start of a session |

---

## Key Patterns

See [docs/agent-patterns.md](../../docs/agent-patterns.md) for the full pattern library:

- **Pattern 1** — Project memory (decisions, tasks, events)
- **Pattern 2** — Scheduler (schedules collection + queue + heartbeat)
- **Pattern 3** — Idempotent background workers
- **Pattern 4** — Audit-friendly writes
- **Pattern 5** — Sidecar + local app sharing one store
- **Pattern 6** — Multi-agent blackboard (shared facts)
- **Pattern 7** — Multi-agent task handoff (queue leases)
- **Pattern 8** — Event-driven pub/sub (signaling)

---

## Mental Model

```txt
objects   → current truth        (put, get, search)
events    → immutable history    (append, list)
queues    → pending work         (push, claim, ack/nack)
MCP tools → agent syscalls
```

Chat is the CPU. `thingd` is persistence and coordination.
