# MCP Server

`thingd` includes MCP server capability directly within the unified CLI package at `packages/thingd-cli`.

The MCP server wraps the public `thingd` SDK and exposes
model-friendly tools for objects, events, search, and queues. This lets MCP
clients and agents use `thingd` as an AI-readable data engine without
knowing SQL or internal storage details.

## Current Status

**46 SDK MCP tools** are exposed via stdio and Streamable HTTP. The Rust sidecar exposes 36 core tools.

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

`thing_search` uses Tantivy — a pure Rust full-text search engine with BM25 ranking, custom metadata key-value filters, and dynamic recency-weighted ranking.

## Tool Surface

All SDK tools are documented in the [API spec — MCP tools reference](api-spec/mcp-tools.md)
with JSON schemas, input/output types, and return formats.

### Categories

- **Search & Objects** (8): `thing_search`, `thing_get`, `thing_put`, `thing_delete`, `thing_objects_list`, `thing_objects_put_batch`, `thing_objects_delete_batch`, `thing_objects_get_batch`
- **Events** (2): `thing_events_append`, `thing_events_list`
- **Queues** (6): `thing_queue_push`, `thing_queue_claim`, `thing_queue_ack`, `thing_queue_nack`, `thing_queue_list`, `thing_queue_dead`
- **Links** (5): `thing_link_create`, `thing_link_delete`, `thing_link_get`, `thing_link_neighbors`, `thing_link_count`
- **Vector Search** (1): `thing_vector_search`
- **Count & Discovery** (8): `thing_count_objects`, `thing_count_objects_in_collection`, `thing_count_events`, `thing_count_active_jobs`, `thing_count_dead_jobs`, `thing_list_collections`, `thing_list_streams`, `thing_list_queues`
- **Aggregate** (2): `thing_aggregate`, `thing_timeseries`
- **Scheduler** (10): `thing_scheduler_schedule`, `thing_scheduler_schedule_interval`, `thing_scheduler_schedule_once`, `thing_scheduler_list`, `thing_scheduler_get`, `thing_scheduler_stats`, `thing_scheduler_pause`, `thing_scheduler_resume`, `thing_scheduler_run`, `thing_scheduler_remove`
- **Schema** (1): `thing_schema`
- **NLQ** (1): `thing_nlq`

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

See the **[5-minute quickstart](./quickstart.md)** for a full walkthrough including Cursor, Claude Desktop, Node SDK, and MCP tool usage.

### Cloud MCP Connect

If you use thingd Cloud, generate agent config pointing at your hosted MCP endpoint:

```bash
thingd cloud login          # authenticate
thingd mcp connect          # pick project/instance → writes config
```

This command:
1. Fetches your projects and instances from thingd Cloud
2. Pre-fills the MCP URL from your instance's endpoint
3. Pre-fills the auth token from your login session
4. Lets you edit URL and token before writing
5. Writes to Claude Desktop, Antigravity IDE, or prints Cursor-compatible JSON

Requires `thingd cloud login` first.

### VS Code (GitHub Copilot / Cline)

VS Code supports MCP servers via the `github.copilot.chat.mcpServers` setting in your
VS Code `settings.json` (per-user) or `mcp.json` (per-project).

**Option A — Local stdio (recommended for development):**

Run thingd as a local stdio MCP server. This never breaks on cloud deploys.

In `~/.config/Code/User/mcp.json` (macOS/Linux) or `%APPDATA%\Code\User\mcp.json` (Windows):

```json
{
  "servers": {
    "thingd": {
      "type": "stdio",
      "command": "thingd",
      "args": ["mcp", "--driver", "native"]
    }
  }
}
```

**Option B — Cloud HTTP endpoint (use a project API key):**

Connect to your thingd Cloud instance. Create a project API key from
the web dashboard (Project Settings → API Keys), or use the CLI:

```bash
thingd cloud login
thingd cloud api-key create <project-id> my-api-key
```

Then add to `mcp.json`:

```json
{
  "servers": {
    "thingd-cloud": {
      "type": "http",
      "url": "https://api.thingd.cloud/mcp/<project-id>/<instance-name>",
      "headers": {
        "Authorization": "Bearer <api-key>"
      }
    }
  }
}
```

> **Note:** Project API keys (`md_pk_...`) persist across cloud deployments.
> Use `thingd cloud token create` for CLI/TUI access. Project API keys are
> managed in the web dashboard under Project Settings → API Keys.

**Option C — Both (redundant fallback):**

Add both servers. When the cloud endpoint is unreachable (e.g., during deploy),
VS Code falls back to the local stdio server automatically for tools that
support it.

```json
{
  "servers": {
    "thingd-local": {
      "type": "stdio",
      "command": "thingd",
      "args": ["mcp", "--driver", "native"]
    },
    "thingd-cloud": {
      "type": "http",
      "url": "https://api.thingd.cloud/mcp/<project-id>/<instance-name>",
      "headers": {
        "Authorization": "Bearer <api-key>"
      }
    }
  }
}
```

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
thing_objects_put_batch
thing_objects_delete_batch
thing_events_append
thing_queue_push
thing_queue_claim
thing_queue_ack
thing_queue_nack
thing_link_create
thing_link_delete
```

Each write tool accepts optional `actor` and `source` inputs. If omitted, the
runtime uses `THINGD_MCP_ACTOR` and `THINGD_MCP_SOURCE`, falling back to
`mcp-client` and `thingd-mcp`.

The `__thingd:mcp:audit` stream is protected at the engine level — events
cannot be deleted or modified. Direct writes via `thing_events_append` or the
REST event endpoint are rejected.

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

Node apps can use the same SDK against the sidecar's HTTP REST API:

```bash
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=change-me
```

```ts
const db = await ThingD.open();
```

`ThingD.open()` detects `http://` URLs and uses the REST cloud driver
automatically.

## Security

The MCP server supports the same security features as the REST API:

- **Authentication:** Bearer token via `THINGD_AUTH_TOKEN`. Required for non-loopback hosts.
- **Hardening:** Collection allowlists (`THINGD_MCP_COLLECTIONS`), read-only mode (`THINGD_MCP_READ_ONLY`), payload limits (`THINGD_MCP_MAX_PAYLOAD_BYTES`)
- **Audit:** All write operations are logged to the `__thingd:mcp:audit` event stream by default
- **TLS:** Not built-in. Deploy behind nginx or Caddy for HTTPS termination
- **Rate limiting:** Via `hardening.rate_limit_enabled` (REST sidecar only)

See [Security](./security.md) for full documentation.

### Audit environment variables

```txt
THINGD_MCP_AUDIT=true
THINGD_MCP_ACTOR=mcp-client
THINGD_MCP_SOURCE=thingd-mcp
THINGD_MCP_AUDIT_STREAM=__thingd:mcp:audit
```

### Cluster bridge mode

Bridge mode is env-driven:

```txt
THINGD_CLUSTER_MODE=single|leader|follower
THINGD_CLUSTER_LEADER_URL=http://thingd-leader:8757
THINGD_CLUSTER_FORWARD_AUTH_TOKEN=change-me
THINGD_CLUSTER_DISCOVERY=none|static|kubernetes
THINGD_CLUSTER_PEERS=http://thingd-0:8757,http://thingd-1:8757
THINGD_CLUSTER_LEADER_ELECTION=false
THINGD_CLUSTER_LEADER_ELECTION_MAX_FAILURES=3
```

Followers automatically forward MCP write traffic to the configured leader and
run a background pull catch-up replication thread to keep their local read
replicas in sync. With `THINGD_CLUSTER_LEADER_ELECTION=true`, followers
auto-promote the next peer in the ordered peer list when the current leader
becomes unreachable.

### MCP enforcement

The MCP layer enforces:

- allowed collections
- read/write permissions
- tool-level validation
- safe mutation boundaries
- source and actor attribution

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
