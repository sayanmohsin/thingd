# Quickstart — thingd in 5 Minutes

This guide takes you from zero to storing and searching data with thingd, then optionally connecting your AI editor — all in under five minutes.

## 1. Install

Requires Node.js >= 24.0.0.

```bash
npm install -g @thingd/cli
```

Or run directly with `npx`:

```bash
npx thingd install
```

> **Note:** `thingd install` also sets up Claude Desktop (on macOS) and prints Cursor MCP settings.

## 2. Put your first object

```bash
npm install @thingd/sdk
```

```ts
import { ThingD } from "@thingd/sdk";

const db = await ThingD.open({ driver: "memory" });

// Store a JSON object — no schema required
await db.put("notes", {
  id: "hello-world",
  text: "My first thingd object",
  tags: ["demo", "quickstart"],
});

// Read it back
const note = await db.get("notes", "hello-world");
console.log(note);

// Full-text search
const hits = await db.search("hello", { collections: ["notes"] });
console.log(hits);

// Append an event
await db.events.append("audit:notes", {
  type: "note.created",
  text: "hello-world was created",
});

// Enqueue a background job
await db.queue("index").push(
  { noteId: "hello-world" },
  { idempotencyKey: "index:hello-world", maxAttempts: 3 }
);

await db.close();
```

That's it — object storage, search, events, and queues in a dozen lines.

## 3. Try the CLI

The CLI gives you the same primitives from the terminal:

```bash
# Show engine status
thingd status

# Search across all collections
thingd search "hello"

# List objects in a collection
thingd objects list notes

# Show queue state
thingd queues
```

```bash
# Launch the interactive inspector dashboard
thingd dashboard
```

Opens a dark-mode browser dashboard at `http://localhost:8758`. Browse collections, event streams, search queries, and queue jobs interactively.

## 4. Connect your editor (MCP)

thingd ships with 34 MCP tools. Connect your editor to search, read, write, and queue data directly from your agent.

### Cursor

Run:

```bash
thingd install
```

Copy the printed Cursor MCP JSON block into **Settings → Features → MCP**.

Or add it manually:

| Field | Value |
|---|---|
| Name | `thingd` |
| Type | `command` |
| Command | `thingd mcp --driver native` |

### Claude Desktop

`thingd install` writes the config automatically on macOS. For manual entry:

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

### VS Code (GitHub Copilot / Cline)

In `.vscode/mcp.json` at your project root:

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

### Verify

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

Once connected, your agent can call all 32 tools — search, objects, events, queues, links, aggregate, schema, NLQ, and discovery.

**Example: agent searches before writing**

```json
// thing_search
{ "query": "auth middleware refactor", "collections": ["tasks"] }

// → found existing task
// → thing_put to update instead of duplicating
```

Full tool reference: [api-spec/mcp-tools.md](./api-spec/mcp-tools.md)

### thingd Cloud

Prefer hosted? [thingd Cloud](https://thingd.cloud) runs thingd for you — managed MCP endpoint, API keys, team dashboard, and backups.

```bash
npx thingd cloud login
npx thingd mcp connect
```

## 5. Secure your deployment (optional)

```bash
# Set an auth token
export THINGD_AUTH_TOKEN="your-secure-token-here-min-16-chars"

# Create a backup
thingd backup --out ./backups/thingd-$(date +%Y-%m-%d).db

# Check database integrity
thingd db integrity
```

See [Security](./security.md) and [Operations](./operations.md) for full documentation.

## What's next

- **Why thingd?** — the full rationale: [why-thingd.md](./why-thingd.md)
- **Queue deep dive** — durable background jobs: [guides/queues.md](./guides/queues.md)
- **Why agents use thingd** — agent value proposition: [why-agents.md](./why-agents.md)
- **Agent patterns** — scheduler, multi-agent blackboard, search-before-put: [agent-patterns.md](./agent-patterns.md)
- **MCP server** — hardening, bridge mode, env reference: [mcp-server.md](./mcp-server.md)
- **API spec** — REST and MCP reference: [api-spec/](./api-spec/)
- **thingd Cloud** — managed hosting: [thingd.cloud](https://thingd.cloud)
