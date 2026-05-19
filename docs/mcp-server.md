# MCP Server

`memoryd` includes an MCP server package at `packages/memoryd-mcp`.

The MCP server wraps the public `@sayanmohsin/memoryd` SDK and exposes
model-friendly tools for objects, events, search, and queues. This lets MCP
clients and agents use `memoryd` as an AI-readable memory database without
knowing SQL or internal storage details.

## Current Status

The package implements the local stdio server plus a remote-capable Streamable
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
- audit events for MCP write tools
- non-loopback HTTP auth guardrails
- bridge-mode cluster status endpoints
- follower MCP forwarding to a configured leader

Not implemented yet:

- hosted/cloud gateway
- TLS termination
- follower local replica catch-up

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

Health check:

```bash
curl http://127.0.0.1:8757/healthz
curl http://127.0.0.1:8757/cluster/status
curl http://127.0.0.1:8757/cluster/peers
```

When the HTTP runtime binds to a non-loopback host such as `0.0.0.0`, it
requires `MEMORYD_AUTH_TOKEN`. Set `MEMORYD_ALLOW_UNAUTHENTICATED=true` only for
local experiments.

## Audit Events

MCP write tools append audit events to `__memoryd:mcp:audit` by default.

Audited tools:

```txt
memory.objects.put
memory.objects.delete
memory.events.append
memory.queue.push
memory.queue.claim
memory.queue.ack
memory.queue.nack
```

Each write tool accepts optional `actor` and `source` inputs. If omitted, the
runtime uses `MEMORYD_MCP_ACTOR` and `MEMORYD_MCP_SOURCE`, falling back to
`mcp-client` and `memoryd-mcp`.

Disable audit events with:

```txt
MEMORYD_MCP_AUDIT=false
```

## Bridge Mode

The HTTP MCP runtime can run as `single`, `leader`, or `follower`:

```txt
MEMORYD_CLUSTER_MODE=single|leader|follower
MEMORYD_CLUSTER_LEADER_URL=http://memoryd-leader:8757
MEMORYD_CLUSTER_FORWARD_AUTH_TOKEN=change-me
MEMORYD_CLUSTER_DISCOVERY=none|static|kubernetes
MEMORYD_CLUSTER_PEERS=http://memoryd-0:8757,http://memoryd-1:8757
MEMORYD_ADVERTISE_URL=http://memoryd-0:8757
```

Followers forward MCP traffic to the leader. This gives Kubernetes pods one
local endpoint while avoiding multi-writer SQLite. Local follower replica
catch-up is still future work.

## Docker Usage

See [docker-runtime.md](./docker-runtime.md).

## Node SDK Remote Driver

Node apps can use the same SDK against the Streamable HTTP runtime:

```bash
MEMORYD_URL=http://127.0.0.1:8757
MEMORYD_AUTH_TOKEN=change-me
```

```ts
const db = await MemoryD.open();
```

`MemoryD.open()` appends `/mcp` automatically when `MEMORYD_URL` points at the
runtime root.

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

See [docker-runtime.md](./docker-runtime.md), [runtime-env.md](./runtime-env.md),
and the [deploy examples](../deploy).
