# Agent Quickstart — thingd in 5 Minutes

This guide takes you from zero to a working persistent agent memory store, ready to use with Cursor or Claude Desktop, in under five minutes.

## 1. Install

Requires Node.js >= 24.0.0.

```bash
npm install -g @thingd/cli
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

### Cloud setup (thingd Cloud)

```bash
npx thingd cloud login
npx thingd mcp connect
```

Follow the prompts to select your project and instance. The config is written to Claude Desktop (on macOS) or printed for Cursor.

### VS Code (GitHub Copilot / Cline)

In `~/.config/Code/User/mcp.json` (macOS/Linux) or `%APPDATA%\Code\User\mcp.json` (Windows), or in
`.vscode/mcp.json` at your project root:

```json
{
  "servers": {
    "thingd": {
      "type": "stdio",
      "command": "thingd",
      "args": ["mcp", "--driver", "native"]
    }
  }
}
```

See the **[MCP server docs](mcp-server.md#vs-code-github-copilot--cline)** for VS Code cloud setup
with persistent API keys.

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
npm install @thingd/sdk
```

```ts
import { ThingD } from "@thingd/sdk";

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

Once the MCP server is registered, Cursor and Claude can call these {{ $themeConfig.mcpToolCount }} tools directly:

```txt
thing_search          — full-text + metadata filter search
thing_get             — fetch a specific object by id
thing_put             — create or update an object
thing_delete          — remove an object
thing_objects_list    — list objects with filter/sort/offset
thing_objects_put_batch — batch create/update objects
thing_objects_delete_batch — batch delete objects
thing_events_append   — write to an event stream
thing_events_list     — read an event stream
thing_queue_push      — enqueue a background job
thing_queue_claim     — lease and start a job
thing_queue_ack       — complete a job
thing_queue_nack      — retry or dead-letter a job
thing_queue_list      — list active jobs
thing_queue_dead      — list dead-letter jobs
thing_link_create     — create a graph link
thing_link_delete     — delete a graph link
thing_link_get        — get a graph link by id
thing_link_neighbors  — get links connected to a reference
thing_link_count      — count all graph links
thing_count_objects   — count all objects
thing_count_events    — count all events
thing_count_active_jobs — count active queue jobs
thing_count_dead_jobs — count dead-letter jobs
thing_list_collections — list all collection names
thing_list_streams    — list all event stream names
thing_list_queues     — list all queue names
```

Full tool reference: [api-spec/mcp-tools.md](./api-spec/mcp-tools.md)

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

## 7. Secure your deployment (optional)

```bash
# Set an auth token
export THINGD_AUTH_TOKEN="your-secure-token-here-min-16-chars"

# Create a backup
thingd backup --out ./backups/thingd-$(date +%Y-%m-%d).db

# Check database integrity
thingd db integrity

# Enable production mode (sanitizes error messages)
# Set server.production_mode: true in config.yaml
```

> **Note for cloud users:** Test tokens (`md_test_...`) are ephemeral and expire on
> every cloud deployment. Create a persistent API key with `thingd cloud api-key create` —
> it survives redeploys and won't break your editor MCP connection.

See [Security](./security.md) and [Operations](./operations.md) for full documentation.

> **☁️ Prefer hosted?** [thingd Cloud](https://thingd.cloud) runs thingd for you — get a managed MCP endpoint, API key management, team dashboard, and backups without the ops overhead. [Sign up →](https://thingd.cloud)

## What's next

- **Scheduler pattern** — background recurring jobs without a cron daemon: [agent-patterns.md](./agent-patterns.md#pattern-2--scheduler-no-built-in-cron)
- **Multi-agent blackboard** — coordinate agents via shared collections: [agent-patterns.md](./agent-patterns.md#pattern-6--multi-agent-blackboard-shared-state--facts)
- **Sidecar mode** — share one store between your app and agents: [mcp-server.md](./mcp-server.md#bridge-mode)
- **MCP hardening** — collection allowlists, read-only mode, payload limits: [mcp-server.md](./mcp-server.md)
- **Security** — TLS, CORS, rate limiting, auth, production hardening: [security.md](./security.md)
- **Operations** — backup, restore, integrity checks, WAL management: [operations.md](./operations.md)
- **Why thingd?** — the full agent value proposition: [why-agents.md](./why-agents.md)
- **API spec** — REST and MCP reference: [api-spec/](./api-spec/)
- **thingd Cloud** — managed hosting for thingd: [thingd.cloud](https://thingd.cloud)
