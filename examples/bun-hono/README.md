# 🐰 thingd + Bun + Hono Example

A runnable example demonstrating thingd as a **remote data engine** accessed from a Bun + Hono app over HTTP.

The key difference from the Node.js examples: this uses `@thingd/sdk/client` which has **zero Node.js dependencies** — pure `fetch()`, works in Bun, Deno, Edge, and browsers.

## Architecture

```
┌──────────────┐         HTTP          ┌──────────────────┐
│  Bun + Hono  │  ──────────────────→  │  thingd sidecar  │
│  (your app)  │  ←──────────────────  │  (Rust binary)   │
└──────────────┘         REST          └──────────────────┘
```

You run the sidecar separately. Your Bun app connects to it via `HttpThingStore`.

## Quick Start

### 1. Start the thingd sidecar

```bash
# Install the CLI (any runtime)
npm install -g @thingd/cli

# Start the sidecar on port 8757
thingd serve --http :8757
```

Or via Docker:

```bash
docker run -p 8757:8757 thingd/thingd
```

### 2. Run the Bun app

```bash
# Navigate to this example
cd examples/bun-hono

# Install dependencies
bun install

# Run in development mode
bun run dev

# Or run in production
bun run src/index.ts
```

### 3. Test it

```bash
# Put an object
curl -X POST http://localhost:3000/objects \
  -H "Content-Type: application/json" \
  -d '{"id":"hello","text":"Hello from Bun!"}'

# Get it back
curl http://localhost:3000/objects/items/hello

# Search
curl "http://localhost:3000/search?q=hello"

# Push a job
curl -X POST http://localhost:3000/queues/tasks/push \
  -H "Content-Type: application/json" \
  -d '{"task":"process-order","orderId":42}'

# Push an event
curl -X POST http://localhost:3000/events/audit \
  -H "Content-Type: application/json" \
  -d '{"type":"user.login","text":"user@example.com logged in"}'
```

## What This Example Shows

| Route | Method | thingd API |
|-------|--------|------------|
| `/objects` | POST | `thingd.put()` |
| `/objects/:collection/:id` | GET | `thingd.get()` |
| `/objects` | GET | `thingd.listObjects()` |
| `/objects/:collection/:id` | DELETE | `thingd.delete()` |
| `/events/:stream` | POST | `thingd.appendEvent()` |
| `/events` | GET | `thingd.listEvents()` |
| `/queues/:name/push` | POST | `thingd.pushJob()` |
| `/queues/:name/claim` | POST | `thingd.claimJob()` |
| `/queues/:name/ack/:jobId` | POST | `thingd.ackJob()` |
| `/search?q=` | GET | `thingd.search()` |

## Import Path

```ts
import { ThingD } from "@thingd/sdk/client";
```

This is the only import path that works in Bun. The main `@thingd/sdk` export bundles an Express-based MCP server and Node `http` types — those won't work in Bun.

## Sidecar Auth

If your sidecar has auth enabled, set the token:

```ts
const thingd = await ThingD.open({
  driver: "cloud",
  databaseUrl: "http://localhost:8757",
  authToken: "your-secret-token",
});
```

Or set `THINGD_URL` and `THINGD_AUTH_TOKEN` environment variables.
