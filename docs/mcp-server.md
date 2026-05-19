# MCP Server

`memoryd` includes an MCP server package at `packages/memoryd-mcp`.

The MCP server wraps the public `@sayanmohsin/memoryd` SDK and exposes
model-friendly tools for objects, events, search, and queues. This lets MCP
clients and agents use `memoryd` as an AI-readable memory database without
knowing SQL or internal storage details.

## Current Status

Phase 8 implements the local stdio skeleton.

Implemented:

- stdio MCP server entrypoint
- `memory.search`
- object read/write/delete tools
- event append/list tools
- queue push/claim/ack/nack/list/dead tools
- in-process MCP client tests

Not implemented yet:

- remote Streamable HTTP MCP endpoint
- Docker image
- bearer token auth
- audit events for MCP writes
- hosted/cloud gateway

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

## ChatGPT And Remote MCP Direction

The current stdio server is for local MCP clients and tests. ChatGPT-style
remote usage needs an HTTPS MCP endpoint, auth, and a Docker/server runtime.

Planned shape:

```txt
ChatGPT / agent
  -> HTTPS MCP endpoint
  -> memoryd server runtime
  -> memoryd database
```

That remote runtime belongs in the next phase.
