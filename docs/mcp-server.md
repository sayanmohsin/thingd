# MCP Server

`memoryd` includes an MCP server package at `packages/memoryd-mcp`.

The MCP server wraps the public `@sayanmohsin/memoryd` SDK and exposes
model-friendly tools for objects, events, search, and queues. This lets MCP
clients and agents use `memoryd` as an AI-readable memory database without
knowing SQL or internal storage details.

## Current Status

Phase 9 implements the local stdio skeleton plus a remote-capable Streamable
HTTP runtime.

Implemented:

- stdio MCP server entrypoint
- Streamable HTTP MCP endpoint at `/mcp`
- bearer token auth for `/mcp`
- health endpoint at `/healthz`
- `memory.search`
- object read/write/delete tools
- event append/list tools
- queue push/claim/ack/nack/list/dead tools
- in-process MCP client tests
- Streamable HTTP MCP client tests

Not implemented yet:

- audit events for MCP writes
- hosted/cloud gateway
- TLS termination

## Tool Surface

```txt
memory.search
memory.objects.get
memory.objects.put
memory.objects.delete
memory.events.append
memory.events.list
memory.queue.push
memory.queue.claim
memory.queue.ack
memory.queue.nack
memory.queue.list
memory.queue.dead
```

## Local Usage

Build the repo:

```bash
npm run build
```

Run with the in-memory SDK store:

```bash
node packages/memoryd-mcp/dist/cli.js --path :memory:
```

Run with the private native Rust-backed SQLite driver:

```bash
npm run build --workspace @sayanmohsin/memoryd-native
node packages/memoryd-mcp/dist/cli.js --path ./memoryd.db --driver native
```

The CLI also reads:

```txt
MEMORYD_PATH
MEMORYD_DRIVER
```

`MEMORYD_DRIVER` can be `memory` or `native`.

## Streamable HTTP Usage

Run the HTTP MCP server:

```bash
npm run build
MEMORYD_AUTH_TOKEN=change-me npm run serve:mcp
```

Default local URL:

```txt
http://127.0.0.1:8757/mcp
```

Direct command:

```bash
node packages/memoryd-mcp/dist/http-cli.js \
  --path ./memoryd.db \
  --driver native \
  --host 127.0.0.1 \
  --port 8757 \
  --auth-token change-me
```

Environment variables:

```txt
MEMORYD_PATH=/data/memoryd.db
MEMORYD_DRIVER=native
MEMORYD_HOST=0.0.0.0
MEMORYD_PORT=8757
MEMORYD_AUTH_TOKEN=change-me
```

Health check:

```bash
curl http://127.0.0.1:8757/healthz
```

## Docker Usage

See [docker-runtime.md](./docker-runtime.md).

## ChatGPT And Remote MCP Direction

The Streamable HTTP server is remote-capable, but ChatGPT-style cloud usage
needs a public HTTPS URL. `localhost` inside your laptop or Docker host is not
reachable by ChatGPT.

Expected deployment shape:

```txt
ChatGPT / agent
  -> HTTPS MCP endpoint
  -> memoryd server runtime
  -> memoryd database
```

The next phase should harden this runtime with migrations, audit events, TLS
deployment guidance, and production packaging.
