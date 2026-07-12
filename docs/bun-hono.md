# thingd + Bun + Hono

thingd's HTTP REST client works in any runtime with `fetch()` — Bun, Deno, Cloudflare Workers, browsers, and Node.js.

Bun users can access the full engine (objects, events, queues, search, links) via HTTP without needing the native Rust addon.

## Import Path

| Import | Bun | What it gives you |
|--------|-----|-------------------|
| `@thingd/sdk` | ❌ | Bundles Express MCP server + Node `http` types |
| `@thingd/sdk/client` | ✅ | `HttpThingStore`, `InMemoryThingStore`, `openThingD` |
| `@thingd/client` | ✅ | Standalone zero-dep REST client |
| `@thingd/sdk/types` | ✅ | TypeScript types only |

## Architecture

thingd runs as a **sidecar** — a separate Rust binary that you start once. Your Bun app connects to it over HTTP REST.

```
┌──────────────┐         HTTP REST      ┌──────────────────┐
│  Bun + Hono  │  ──────────────────→  │  thingd sidecar  │
│  (your app)  │  ←──────────────────  │  (Rust binary)   │
└──────────────┘                       └──────────────────┘
```

## Setup

### 1. Start the sidecar

```bash
# Install the CLI
npm install -g @thingd/cli

# Start on port 8757
thingd serve --http :8757
```

Or via Docker:

```bash
docker run -p 8757:8757 thingd/thingd
```

Or as a Kubernetes sidecar (same pod, container port 8757).

### 2. Connect from Bun

```ts
import { HttpThingStore } from "@thingd/sdk/client";

const thingd = await HttpThingStore.open({
  url: process.env.THINGD_URL ?? "http://localhost:8757",
  authToken: process.env.THINGD_AUTH_TOKEN,
});
```

## Full Hono Example

```ts
import { ThingD } from "@thingd/sdk/client";
import { Hono } from "hono";

const thingd = await ThingD.open({
  driver: "cloud",
  databaseUrl: "http://localhost:8757",
});

const app = new Hono();

// Objects
app.post("/notes", async (c) => {
  const { id, ...rest } = await c.req.json();
  const stored = await thingd.put("notes", { id, ...rest });
  return c.json(stored, 201);
});

app.get("/notes/:id", async (c) => {
  const note = await thingd.get("notes", c.req.param("id"));
  return note ? c.json(note) : c.json({ error: "not found" }, 404);
});

app.get("/search", async (c) => {
  const q = c.req.query("q") ?? "";
  const results = await thingd.search(q);
  return c.json({ results });
});

// Events
app.post("/events/:stream", async (c) => {
  const event = await c.req.json();
  const stored = await thingd.appendEvent(c.req.param("stream"), event);
  return c.json(stored, 201);
});

// Queues
app.post("/jobs", async (c) => {
  const payload = await c.req.json();
  const job = await thingd.pushJob("default", payload);
  return c.json(job, 201);
});

export default { fetch: app.fetch, port: 3000 };
```

## Running in Production

### Option A: Process manager

```bash
# Start sidecar
thingd serve --http :8757 --data-dir ./data &

# Start Bun app
bun run index.ts
```

### Option B: Docker Compose

```yaml
services:
  thingd:
    image: thingd/thingd
    ports: ["8757:8757"]
    volumes: ["./data:/data"]

  app:
    build: .
    ports: ["3000:3000"]
    environment:
      THINGD_URL: http://thingd:8757
```

### Option C: Kubernetes sidecar

```yaml
spec:
  containers:
    - name: thingd
      image: thingd/thingd
    - name: app
      image: my-app
      env:
        - name: THINGD_URL
          value: http://localhost:8757
```

## Limitations vs Node.js Native

| Feature | Bun + HTTP | Node.js Native |
|---------|-----------|----------------|
| SQLite persistence | ✅ via sidecar | ✅ in-process |
| Full-text search (FTS5) | ✅ via sidecar | ✅ in-process |
| Performance | Network hop (<1ms local) | In-process (zero-copy) |
| `backupTo()` | ❌ sidecar API call | ✅ direct |
| `walCheckpoint()` | ❌ sidecar API call | ✅ direct |
| Startup | Sidecar + app (2 processes) | Single process |

For local development the network hop is negligible (< 0.5ms on loopback). For production, the sidecar pattern is actually cleaner — your app restarts don't affect the engine's state.
