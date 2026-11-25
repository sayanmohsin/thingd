# Quickstart — thingd in 5 Minutes

This guide takes you from zero to storing and searching data with thingd, then optionally connecting your AI editor — all in under five minutes.

## 1. Install

Requires Node.js >= 24.0.0.

```bash
npm install -g @thingd/cli
```

Or run directly with `npx`:

```bash
npx --package @thingd/cli thingd install
```

> **Note:** `thingd install` also sets up Claude Desktop (on macOS) and prints Cursor MCP settings.

## 2. Put your first object

```bash
npm install @thingd/sdk
```

Save the following as `quickstart.mjs` in the same directory:

```js
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

Run it with `node quickstart.mjs`. You should see the saved note and search results containing it (the exact output includes additional fields).

That's object storage, search, events, and queues in one script.
The explicit `memory` driver is the portable TypeScript/reference store. In
Node.js, `ThingD.open(":memory:")` uses the ThingDB RAM engine when the native
addon is available and never creates durable files; data is lost on process
exit.
For direct Rust usage, see the [standalone ThingDB API contract](./api-spec/thingdb.md).

### Optional encrypted native database

For a file-backed native store, inject a 64-character hexadecimal key before
opening the database:

```bash
export THINGD_ENCRYPTION_KEY=<64-hex-characters>
```

```ts
const db = await ThingD.open({
  driver: "native",
  path: "./thingd-data",
  encryption: { key: process.env.THINGD_ENCRYPTION_KEY },
});
```

The key is a process startup secret, not an MCP or REST credential. Encrypted
databases require it on every reopen; changing it does not rotate the database.

## 3. Try persistent storage and the CLI

The example above uses process-local memory, so the CLI cannot see its data.
To share a local database, change the `ThingD.open` line in `quickstart.mjs` to:

```js
const db = await ThingD.open({ driver: "native", path: "./thingd-data" });
```

Run `node quickstart.mjs` again and wait for it to close the database. Then run
the CLI from the same directory, pointing it at the same path:

```bash
export THINGD_PATH="$(pwd)/thingd-data"
thingd objects list notes
```

The output should include `hello-world`. Close one process before opening the
same local database in another. The CLI defaults to `~/.thingd/data.db` when
no path is specified; see [storage backends](./storage-backends.md) for driver
and durability details.

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

### Optional schema files

For an explicit, version-controlled data contract, add a `schema.thingd` file
and validate it before committing:

```bash
thingd schema check schema.thingd
```

Schema files are optional; the normal object API remains schemaless. See the
[schema files guide](/schema) for syntax and the current validation boundary.

## 4. Connect your editor (MCP)

thingd ships with 51 Node MCP tools. The Rust sidecar exposes 41 engine tools; connect your editor to search, read, write, and queue data directly from your agent.

If the native database is encrypted, configure `THINGD_ENCRYPTION_KEY` in the
host process before starting MCP. The editor's MCP configuration and tool calls
remain unchanged, and the editor never receives the database key.

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

The diagnostic reports the Node version, the selected driver and local path, or remote connectivity when configured. Check your editor’s MCP status separately to confirm that it connected.

Once connected to the Node MCP server, your agent can call all 51 tools — search, objects, events, queues, links, aggregate, schema, NLQ, vector, indexing, scheduler, and discovery.

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

For a React Native or Expo application that does not need a custom API server,
follow the [Nice Rep CLI-first mobile setup](./nice-rep.md). It covers the
declarative app definition, `schema.thingd`, Publish lifecycle, publishable-key
configuration, and the `createThingdAppClient` flow.

## Troubleshooting

- **Node version or command not found:** run `node --version` and check that the global npm bin directory is on your PATH. The SDK requires Node.js 24 or newer.
- **Native addon unavailable:** run `thingd doctor --driver native` and check its addon output. See [storage backends](./storage-backends.md) for native and portable options.
- **Objects missing in the CLI:** the memory example disappears when its process exits. For a persistent database, check that the SDK and CLI use the same absolute path and that the SDK process has closed it.
- **Editor cannot connect:** run `thingd doctor`, check the command and path in your editor’s MCP configuration, then restart the editor. See [MCP setup](./mcp-server.md) for more connection options.

## 5. Secure your deployment (optional)

```bash
# Set an auth token
export THINGD_AUTH_TOKEN="$(openssl rand -hex 32)"

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
- **Mobile app backend** — CLI-first Expo setup: [nice-rep.md](./nice-rep.md)
- **Agent patterns** — scheduler, multi-agent blackboard, search-before-put: [agent-patterns.md](./agent-patterns.md)
- **MCP server** — hardening, bridge mode, env reference: [mcp-server.md](./mcp-server.md)
- **API spec** — REST and MCP reference: [api-spec/](./api-spec/)
- **thingd Cloud** — managed hosting: [thingd.cloud](https://thingd.cloud)
