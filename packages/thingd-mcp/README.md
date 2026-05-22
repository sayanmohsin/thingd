# thingd-mcp

MCP server for `thingd`.

This package exposes the public `thingd` SDK as model-friendly MCP
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
pnpm --filter thingd-mcp build
pnpm --filter thingd-mcp test
```

## Run Locally

### Stdio

```bash
pnpm build
node packages/thingd-mcp/dist/cli.js --path :memory:
```

For the private native driver:

```bash
pnpm --filter thingd-native build
node packages/thingd-mcp/dist/cli.js --path ./thingd.db --driver native
```

Environment variables:

```txt
THINGD_PATH=:memory:
THINGD_DRIVER=memory
```

`THINGD_DRIVER` can be `memory` or `native`.

### Node SDK Sidecar Client

Node apps can use this HTTP runtime through the normal `thingd`
SDK. Set `THINGD_URL` and call `ThingD.open()`:

```bash
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=change-me
```

```ts
import { ThingD } from "thingd";

const db = await ThingD.open();
```

If `THINGD_URL` has no path, the SDK appends `/mcp` automatically and uses the
remote driver over Streamable HTTP MCP.

### Streamable HTTP

```bash
pnpm build
THINGD_AUTH_TOKEN=change-me pnpm serve:mcp
```

Or directly:

```bash
node packages/thingd-mcp/dist/http-cli.js \
  --path ./thingd.db \
  --driver native \
  --host 127.0.0.1 \
  --port 8757 \
  --auth-token change-me
```

HTTP runtime environment:

```txt
THINGD_PATH=/data/thingd.db
THINGD_DRIVER=native
THINGD_HOST=0.0.0.0
THINGD_PORT=8757
THINGD_AUTH_TOKEN=change-me
THINGD_ALLOW_UNAUTHENTICATED=false
THINGD_MCP_AUDIT=true
THINGD_MCP_ACTOR=mcp-client
THINGD_MCP_SOURCE=thingd-mcp
THINGD_MCP_AUDIT_STREAM=__thingd:mcp:audit
THINGD_CLUSTER_MODE=single
THINGD_CLUSTER_LEADER_URL=
THINGD_CLUSTER_FORWARD_AUTH_TOKEN=
THINGD_CLUSTER_PEERS=
```

Endpoints:

```txt
GET  /healthz
POST /mcp
GET  /cluster/status
GET  /cluster/peers
```

When binding to `0.0.0.0` or another non-loopback host, the HTTP runtime
requires `THINGD_AUTH_TOKEN` unless `THINGD_ALLOW_UNAUTHENTICATED=true` is set.

### Audit Events

Write tools append audit events to `__thingd:mcp:audit` by default. Tool calls
can pass optional `actor` and `source` inputs.

### Bridge Mode

```txt
THINGD_CLUSTER_MODE=single|leader|follower
THINGD_CLUSTER_LEADER_URL=http://thingd-leader:8757
THINGD_CLUSTER_FORWARD_AUTH_TOKEN=change-me
THINGD_CLUSTER_DISCOVERY=none|static|kubernetes
```

Followers forward MCP traffic to the leader. Local follower replica catch-up is
not implemented yet.

## Tools

```txt
thing.search
thing.get
thing.put
thing.delete
thing.events.append
thing.events.list
thing.queue.push
thing.queue.claim
thing.queue.ack
thing.queue.nack
thing.queue.list
thing.queue.dead
```

## Design Rules

- Keep app-facing APIs in `thingd`.
- Keep MCP tools object-shaped and model-readable.
- Prefer safe tool descriptions and explicit inputs.
- Keep MCP write audit events enabled unless a runtime explicitly disables them.
- Do not expose SQL as the MCP interface.
