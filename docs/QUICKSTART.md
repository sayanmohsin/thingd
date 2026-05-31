# Agent Quickstart — thingd in 5 Minutes

This guide takes you from zero to a working persistent agent memory store, ready to use with Cursor or Claude Desktop, in under five minutes.

## 1. Install

```bash
npm install -g thingd-cli
```

Or run directly with `npx`:

```bash
npx thingd install
```

> **Note:** `thingd install` auto-detects your Node runtime path, configures Claude Desktop automatically (on macOS), and prints a copy-pasteable block for Cursor's MCP settings.

## 2. Register with your editor

### Cursor

Run:

```bash
thingd install
```

Copy the printed Cursor MCP JSON block into **Settings → Features → MCP**.

Or add it manually: open Cursor **Settings** (`Cmd+,`), go to **Features → MCP → + Add New MCP Tool**:

| Field | Value |
|---|---|
| Name | `thingd` |
| Type | `command` |
| Command | `thingd mcp --driver native` |

### Claude Desktop

`thingd install` writes to `~/Library/Application Support/Claude/claude_desktop_config.json` automatically on macOS.

Manual entry:

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

## 3. Verify the connection

```bash
thingd doctor
```

Expected output:

```txt
✔ Node runtime: /usr/local/bin/node
✔ Native driver: available
✔ Database: ~/.thingd/data.db
✔ MCP server: reachable
```

## 4. Inspect live with the dashboard

```bash
thingd dashboard
```

Opens a dark-mode browser dashboard at `http://localhost:8758`. Browse collections, event streams, search queries, and queue jobs interactively.

## 5. Put your first memory object (via Node SDK)

```bash
npm install thingd
```

```ts
import { ThingD } from "thingd";

const db = await ThingD.open({ driver: "native" });

// Store a decision
await db.put("decisions", {
  id: "use-rust-core",
  text: "Use Rust for the storage engine and TypeScript for the developer SDK.",
  project: "thingd",
  confidence: 0.95,
});

// Append an event to the project timeline
await db.events.append("project:thingd", {
  type: "decision.made",
  text: "Rust core selected for performance and memory safety.",
  actor: "sayan",
});

// Search memory — stemming means "choosing" matches "choose"
const hits = await db.search("why choosing rust?", {
  collections: ["decisions"],
});
console.log(hits);

// Enqueue a background job
await db.queue("embed").push(
  { object: "decisions/use-rust-core" },
  { idempotencyKey: "embed:decisions:use-rust-core:v1", maxAttempts: 3 }
);

await db.close();
```

## 6. Use memory tools from your agent (MCP)

Once the MCP server is registered, Cursor and Claude can call these tools directly:

```txt
thing_search      — full-text + metadata filter search
thing_get         — fetch a specific object by id
thing_put         — create or update an object
thing_delete      — remove an object
thing_events_append — write to an event stream
thing_events_list   — read an event stream
thing_queue_push  — enqueue a background job
thing_queue_claim — lease and start a job
thing_queue_ack   — complete a job
thing_queue_nack  — retry or dead-letter a job
thing_queue_list  — list active jobs
thing_queue_dead  — list dead-letter jobs
```

**Example: agent puts a task for the next session**

```json
// thing_put
{
  "collection": "tasks",
  "object": {
    "id": "refactor-auth",
    "title": "Refactor auth middleware to use thingd sessions",
    "status": "pending",
    "priority": 1
  },
  "actor": "cursor-agent",
  "source": "planning-session"
}
```

**Example: agent searches before writing (no duplicates)**

```json
// thing_search
{ "query": "auth middleware refactor", "collections": ["tasks"] }

// → found id: "refactor-auth"
// → update it instead of creating a duplicate
```

## 7. Add agent rules to your project

Copy the provided `.cursorrules` to your project root to teach agents the memory conventions automatically:

```bash
cp node_modules/thingd-cli/examples/cursor-agent-memory/.cursorrules .cursorrules
```

Or see the full example at [examples/cursor-agent-memory/](../examples/cursor-agent-memory/).

## What's next

- **Scheduler pattern** — background recurring jobs without a cron daemon: [agent-patterns.md](./agent-patterns.md#pattern-2--scheduler-no-built-in-cron)
- **Multi-agent blackboard** — coordinate agents via shared collections: [agent-patterns.md](./agent-patterns.md#pattern-6--multi-agent-blackboard-shared-state--facts)
- **Sidecar mode** — share one store between your app and agents: [sidecar-cluster.md](./sidecar-cluster.md)
- **MCP hardening** — collection allowlists, read-only mode, payload limits: [mcp-server.md](./mcp-server.md)
- **Why thingd?** — the full agent value proposition: [why-agents.md](./why-agents.md)
