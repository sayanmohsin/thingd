# @sayanmohsin/memoryd-mcp

MCP server for `memoryd`.

This package exposes the public `@sayanmohsin/memoryd` SDK as model-friendly MCP
tools. It does not reach into internal store implementations.

## Status

Phase 9 runtime skeleton:

- stdio MCP transport
- Streamable HTTP MCP transport
- bearer token auth for HTTP MCP
- `/healthz`
- object tools
- event tools
- search tool
- queue tools
- tests using the official MCP SDK in-memory and Streamable HTTP transports

Docker runtime and production hardening are still early.

## Build And Test

```bash
npm run build --workspace @sayanmohsin/memoryd-mcp
npm run test --workspace @sayanmohsin/memoryd-mcp
```

## Run Locally

### Stdio

```bash
npm run build
node packages/memoryd-mcp/dist/cli.js --path :memory:
```

For the private native driver:

```bash
npm run build --workspace @sayanmohsin/memoryd-native
node packages/memoryd-mcp/dist/cli.js --path ./memoryd.db --driver native
```

Environment variables:

```txt
MEMORYD_PATH=:memory:
MEMORYD_DRIVER=memory
```

`MEMORYD_DRIVER` can be `memory` or `native`.

### Streamable HTTP

```bash
npm run build
MEMORYD_AUTH_TOKEN=change-me npm run serve:mcp
```

Or directly:

```bash
node packages/memoryd-mcp/dist/http-cli.js \
  --path ./memoryd.db \
  --driver native \
  --host 127.0.0.1 \
  --port 8757 \
  --auth-token change-me
```

HTTP runtime environment:

```txt
MEMORYD_PATH=/data/memoryd.db
MEMORYD_DRIVER=native
MEMORYD_HOST=0.0.0.0
MEMORYD_PORT=8757
MEMORYD_AUTH_TOKEN=change-me
```

Endpoints:

```txt
GET  /healthz
POST /mcp
```

## Tools

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

## Design Rules

- Keep app-facing APIs in `@sayanmohsin/memoryd`.
- Keep MCP tools object-shaped and model-readable.
- Prefer safe tool descriptions and explicit inputs.
- Add audit metadata later when the server/runtime layer exists.
- Do not expose SQL as the MCP interface.
