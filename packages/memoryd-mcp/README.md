# @sayanmohsin/memoryd-mcp

MCP server for `memoryd`.

This package exposes the public `@sayanmohsin/memoryd` SDK as model-friendly MCP
tools. It does not reach into internal store implementations.

## Status

Runtime skeleton:

- stdio MCP transport
- Streamable HTTP MCP transport
- bearer token auth for HTTP MCP
- `/healthz`
- non-loopback HTTP auth guardrails
- bridge-mode cluster status endpoints
- follower MCP forwarding to a configured leader
- object tools
- event tools
- search tool
- queue tools
- audit events for write tools
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
MEMORYD_ALLOW_UNAUTHENTICATED=false
MEMORYD_MCP_AUDIT=true
MEMORYD_MCP_ACTOR=mcp-client
MEMORYD_MCP_SOURCE=memoryd-mcp
MEMORYD_MCP_AUDIT_STREAM=__memoryd:mcp:audit
MEMORYD_CLUSTER_MODE=single
MEMORYD_CLUSTER_LEADER_URL=
MEMORYD_CLUSTER_FORWARD_AUTH_TOKEN=
MEMORYD_CLUSTER_PEERS=
```

Endpoints:

```txt
GET  /healthz
POST /mcp
GET  /cluster/status
GET  /cluster/peers
```

When binding to `0.0.0.0` or another non-loopback host, the HTTP runtime
requires `MEMORYD_AUTH_TOKEN` unless `MEMORYD_ALLOW_UNAUTHENTICATED=true` is set.

### Audit Events

Write tools append audit events to `__memoryd:mcp:audit` by default. Tool calls
can pass optional `actor` and `source` inputs.

### Bridge Mode

```txt
MEMORYD_CLUSTER_MODE=single|leader|follower
MEMORYD_CLUSTER_LEADER_URL=http://memoryd-leader:8757
MEMORYD_CLUSTER_FORWARD_AUTH_TOKEN=change-me
MEMORYD_CLUSTER_DISCOVERY=none|static|kubernetes
```

Followers forward MCP traffic to the leader. Local follower replica catch-up is
not implemented yet.

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
- Keep MCP write audit events enabled unless a runtime explicitly disables them.
- Do not expose SQL as the MCP interface.
