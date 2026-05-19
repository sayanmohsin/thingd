# @sayanmohsin/memoryd-mcp

MCP server for `memoryd`.

This package exposes the public `@sayanmohsin/memoryd` SDK as model-friendly MCP
tools. It does not reach into internal store implementations.

## Status

Phase 8 skeleton:

- stdio MCP transport
- object tools
- event tools
- search tool
- queue tools
- tests using the official MCP SDK in-memory transport

Remote HTTP MCP and Docker runtime are planned next.

## Build And Test

```bash
npm run build --workspace @sayanmohsin/memoryd-mcp
npm run test --workspace @sayanmohsin/memoryd-mcp
```

## Run Locally

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
