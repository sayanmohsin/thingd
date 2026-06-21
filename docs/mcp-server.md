# MCP Server

`thingd` includes MCP server capability directly within the unified CLI package at `packages/thingd-cli`.

The MCP server wraps the public `thingd` SDK and exposes
model-friendly tools for objects, events, search, and queues. This lets MCP
clients and agents use `thingd` as an AI-readable memory database without
knowing SQL or internal storage details.

## Current Status

**27 MCP tools** exposed via stdio and Streamable HTTP.

The package implements the local stdio server plus a remote-capable Streamable
HTTP runtime.

Implemented:

- stdio MCP server entrypoint
- Streamable HTTP MCP endpoint at `/mcp`
- bearer token auth for `/mcp`
- health endpoint at `/healthz`
- `thing_search`
- object read/write/delete tools
- event append/list tools
- queue push/claim/ack/nack/list/dead tools
- graph link create/delete/get/neighbors/count tools
- object batch put/delete tools
- in-process MCP client tests
- Streamable HTTP MCP client tests
- audit events for MCP write tools
- non-loopback HTTP auth guardrails
- bridge-mode cluster status endpoints
- follower MCP forwarding to a configured leader

### Search behavior

`thing_search` uses a high-performance database-native SQLite **FTS5** virtual table with Porter word stemming, custom metadata key-value filters, and dynamic recency-weighted ranking.

## Tool Surface

### Object tools

| Tool | Description |
|------|-------------|
| `thing_search` | Full-text search across objects and events with metadata filters |
| `thing_get` | Read one object by collection and id |
| `thing_put` | Create or replace an object in a collection |
| `thing_delete` | Delete an object by collection and id |
| `thing_objects_list` | List objects with optional filter, sortBy, limit, offset |
| `thing_objects_put_batch` | Create or replace multiple objects in a single operation |
| `thing_objects_delete_batch` | Delete multiple objects by ID in a single operation |

### Event tools

| Tool | Description |
|------|-------------|
| `thing_events_append` | Append an event to a named stream |
| `thing_events_list` | List events with optional stream filter and pagination |

### Queue tools

| Tool | Description |
|------|-------------|
| `thing_queue_push` | Push a job onto a queue with optional delay and idempotency |
| `thing_queue_claim` | Claim the next ready job from a queue |
| `thing_queue_ack` | Mark a leased job as completed |
| `thing_queue_nack` | Reject a job for retry or dead-letter routing |
| `thing_queue_list` | List all jobs in a queue |
| `thing_queue_dead` | List dead-letter jobs in a queue |

### Link tools

| Tool | Description |
|------|-------------|
| `thing_link_create` | Create a directed graph link between two references |
| `thing_link_delete` | Delete a graph link by id |
| `thing_link_get` | Get a graph link by id |
| `thing_link_neighbors` | Get links connected to a reference (outgoing/incoming/both) |
| `thing_link_count` | Count all graph links |

### Count and discovery tools

| Tool | Description |
|------|-------------|
| `thing_count_objects` | Count all objects across all collections |
| `thing_count_events` | Count all events across all streams |
| `thing_count_active_jobs` | Count all active (non-dead) queue jobs |
| `thing_count_dead_jobs` | Count all dead-letter queue jobs |
| `thing_list_collections` | List all collection names |
| `thing_list_streams` | List all event stream names |
| `thing_list_queues` | List all queue names |

## Zero-Config Setup

The recommended way to set up and integrate the stdio MCP server with your AI editor (Cursor or Claude Desktop) is using the `install` command:

```bash
thingd install
```

This command will:
1. Auto-detect your Node runtime path and global CLI script path.
2. Auto-detect native driver availability.
3. Automatically configure Claude Desktop (on macOS) by writing to `claude_desktop_config.json`.
4. Print a copy-pasteable JSON block for Cursor's MCP server configuration.
5. Auto-create the default database directory.

See the **[5-minute quickstart](./QUICKSTART.md)** for a full walkthrough including Cursor, Claude Desktop, Node SDK, and MCP tool usage.

---

## Local Usage

Build the repo:

```bash
pnpm build
```

Run with the in-memory SDK store:

```bash
thingd mcp --path :memory:
```

Run with the private native Rust-backed SQLite driver (which automatically persists to `~/.thingd/data.db` by default if no path is given):

```bash
thingd mcp --driver native
```

You can customize the path by passing `--path <file>` or setting environment variables.

The CLI also reads:

```txt
THINGD_PATH
THINGD_DRIVER
```

`THINGD_DRIVER` can be `memory` or `native`. Defaults to `~/.thingd/data.db` and `native` for persistent local storage.

## Streamable HTTP Usage

Run the HTTP MCP server:

```bash
pnpm build
THINGD_AUTH_TOKEN=change-me pnpm serve:mcp
```

Default local URL:

```txt
http://127.0.0.1:8757/mcp
```

Direct command:

```bash
node packages/thingd-cli/dist/index.js mcp-http \
  --path ./thingd.db \
  --driver native \
  --host 127.0.0.1 \
  --port 8757 \
  --auth-token change-me
```

Environment variables:

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
THINGD_MCP_COLLECTIONS=memories,decisions,tasks
THINGD_MCP_READ_ONLY=false
THINGD_MCP_MAX_PAYLOAD_BYTES=524288
THINGD_CLUSTER_MODE=single
THINGD_CLUSTER_LEADER_URL=
THINGD_CLUSTER_FORWARD_AUTH_TOKEN=
THINGD_CLUSTER_PEERS=
```

Health check:

```bash
curl http://127.0.0.1:8757/healthz
curl http://127.0.0.1:8757/cluster/status
curl http://127.0.0.1:8757/cluster/peers
```

When the HTTP runtime binds to a non-loopback host such as `0.0.0.0`, it
requires `THINGD_AUTH_TOKEN`. Set `THINGD_ALLOW_UNAUTHENTICATED=true` only for
local experiments.

## MCP Hardening

Three env vars control access enforcement on both the stdio and HTTP runtimes:

| Var | Default | Behaviour |
| --- | --- | --- |
| `THINGD_MCP_COLLECTIONS` | unset | Comma-separated allowlist. Tool calls for unlisted collections are rejected immediately. |
| `THINGD_MCP_READ_ONLY` | `false` | When `true`, all write tools return an error. Read tools are unaffected. |
| `THINGD_MCP_MAX_PAYLOAD_BYTES` | `524288` | HTTP only. Request bodies over this limit receive HTTP 413 before the MCP layer processes them. |

**Examples:**

```bash
# Allow agents to read/write only two collections
THINGD_MCP_COLLECTIONS=memories,decisions thingd mcp --driver native

# Public inspector — agents can search and read but not write
THINGD_MCP_READ_ONLY=true thingd mcp-http --driver native

# Tighter payload limit for untrusted networks
THINGD_MCP_MAX_PAYLOAD_BYTES=65536 THINGD_AUTH_TOKEN=secret thingd mcp-http
```

## MCP Resources

The server implements the MCP `resources/list` capability. Agents can call
`resources/list` to enumerate known collections without a tool call:

```txt
thingd://collections  — list of known object collection names
```

If `THINGD_MCP_COLLECTIONS` is set, only allowed collections appear. Returns an
empty list when using the in-memory driver (no collections have been created yet).

## Audit Events

MCP write tools append audit events to `__thingd:mcp:audit` by default.

Audited tools:

```txt
thing_put
thing_delete
thing_events_append
thing_queue_push
thing_queue_claim
thing_queue_ack
thing_queue_nack
```

Each write tool accepts optional `actor` and `source` inputs. If omitted, the
runtime uses `THINGD_MCP_ACTOR` and `THINGD_MCP_SOURCE`, falling back to
`mcp-client` and `thingd-mcp`.

Disable audit events with:

```txt
THINGD_MCP_AUDIT=false
```

## Bridge Mode

The HTTP MCP runtime can run as `single`, `leader`, or `follower`:

```txt
THINGD_CLUSTER_MODE=single|leader|follower
THINGD_CLUSTER_LEADER_URL=http://thingd-leader:8757
THINGD_CLUSTER_FORWARD_AUTH_TOKEN=change-me
THINGD_CLUSTER_DISCOVERY=none|static|kubernetes
THINGD_CLUSTER_PEERS=http://thingd-0:8757,http://thingd-1:8757
THINGD_ADVERTISE_URL=http://thingd-0:8757
```

Followers forward MCP traffic to the leader. This gives Kubernetes pods one
local endpoint while avoiding multi-writer SQLite. Local follower replica
catch-up runs as an async background process that polls the leader every 500ms
for new change events.

## Docker Usage

See [docker-runtime.md](./docker-runtime.md).

## Node SDK Remote Driver

Node apps can use the same SDK against the Streamable HTTP runtime:

```bash
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=change-me
```

```ts
const db = await ThingD.open();
```

`ThingD.open()` appends `/mcp` automatically when `THINGD_URL` points at the
runtime root.

## ChatGPT And Remote MCP Access

The Streamable HTTP server is remote-capable, but ChatGPT-style cloud usage
needs a public HTTPS URL. `localhost` inside your laptop or Docker host is not
reachable by ChatGPT.

Expected deployment shape:

```txt
ChatGPT / agent
  -> HTTPS MCP endpoint
  -> thingd server runtime
  -> thingd database
```

See [docker-runtime.md](./docker-runtime.md), [runtime-env.md](./runtime-env.md),
[api-spec/mcp-tools.md](./api-spec/mcp-tools.md), and the [deploy examples](../deploy).
