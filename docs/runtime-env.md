# Runtime Environment

This is the current environment variable reference for the `thingd` HTTP MCP
runtime and Docker sidecar.

## App SDK

```txt
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=change-me
```

When `THINGD_URL` is set, `ThingD.open()` uses the SDK cloud driver and
talks to the sidecar over HTTP REST. If the URL has no path, `/v1` is
used automatically.

## Storage

```txt
THINGD_PATH=/data/thingd.db
THINGD_DRIVER=native
```

`THINGD_DRIVER` can be `memory` or `native`. Use `native` for the Rust-backed
SQLite store after the native package has been built into the runtime image.

## HTTP

```txt
THINGD_HOST=0.0.0.0
THINGD_PORT=8757
THINGD_AUTH_TOKEN=change-me
THINGD_ALLOW_UNAUTHENTICATED=false
THINGD_CONFIG=/etc/thingd/config.yaml
THINGD_PRODUCTION=false
```

When `THINGD_HOST` is a non-loopback host, `THINGD_AUTH_TOKEN` is required.
`THINGD_ALLOW_UNAUTHENTICATED=true` is only for local experiments.
`THINGD_PRODUCTION=true` enables error sanitization (internal details stripped from 500 responses).

## Hardening

```txt
THINGD_RATE_LIMIT_ENABLED=true
THINGD_RATE_LIMIT_RPM=300
THINGD_CORS_ORIGINS=http://localhost:8757
THINGD_MAX_PAYLOAD_BYTES=524288
```

| Var | Default | Description |
| --- | --- | --- |
| `THINGD_RATE_LIMIT_ENABLED` | `true` | Enable per-IP rate limiting |
| `THINGD_RATE_LIMIT_RPM` | `300` | Requests per minute per IP |
| `THINGD_CORS_ORIGINS` | `http://localhost:8757` | Comma-separated allowed origins. Empty = permissive (`*`) |
| `THINGD_MAX_PAYLOAD_BYTES` | `524288` (512 KB) | Request body size limit |

## Server Config

The sidecar reads a YAML config file from `THINGD_CONFIG` (default: `/etc/thingd/config.yaml`).
All config values can be overridden by environment variables (prefixed with `THINGD_`).

**Example config.yaml:**

```yaml
server:
  host: "0.0.0.0"
  port: 8757
  database: "/data/thingd.db"
  production_mode: false
auth:
  token: "change-me-with-a-random-token"
  allow_unauthenticated: false
hardening:
  cors_allowed_origins:
    - "http://localhost:8757"
  rate_limit_enabled: true
  rate_limit_requests_per_minute: 300
  max_payload_bytes: 524288
  max_connector_file_bytes: 67108864
  connector_file_root: "/srv/thingd/imports"
  connector_allowed_hosts: ["db.internal.example"]
  connector_require_tls: true
```

## MCP Audit

```txt
THINGD_MCP_AUDIT=true
THINGD_MCP_ACTOR=mcp-client
THINGD_MCP_SOURCE=thingd-mcp
THINGD_MCP_AUDIT_STREAM=__thingd:mcp:audit
```

Write tools append audit events by default. Set `THINGD_MCP_AUDIT=false` only
when you explicitly do not want MCP write events recorded.

## MCP Hardening

```txt
THINGD_MCP_COLLECTIONS=memories,decisions,tasks
THINGD_MCP_READ_ONLY=false
THINGD_MCP_MAX_PAYLOAD_BYTES=524288
```

| Var | Default | Description |
| --- | --- | --- |
| `THINGD_MCP_COLLECTIONS` | unset (all allowed) | Comma-separated allowlist. Tool calls for unlisted collections are rejected. |
| `THINGD_MCP_READ_ONLY` | `false` | When `true`, all write tools (`thing_put`, `thing_delete`, queue mutations) return an error. |
| `THINGD_MCP_MAX_PAYLOAD_BYTES` | `524288` (512 KB) | HTTP MCP request body size limit. Requests over this limit receive HTTP 413. |


## Bridge And Cluster

```txt
THINGD_CLUSTER_MODE=single
THINGD_CLUSTER_DISCOVERY=none
THINGD_CLUSTER_LEADER_URL=
THINGD_CLUSTER_LEADER_FALLBACK_URL=
THINGD_CLUSTER_FORWARD_AUTH_TOKEN=
THINGD_CLUSTER_PEERS=
THINGD_CLUSTER_LEADER_ELECTION=false
THINGD_CLUSTER_LEADER_ELECTION_MAX_FAILURES=3
THINGD_ADVERTISE_URL=
THINGD_CLUSTER_SERVICE=
THINGD_CLUSTER_NAMESPACE=default
THINGD_CLUSTER_PORT=8757
```

Modes:

```txt
single    standalone runtime, default
leader    owns writes locally
follower  forwards MCP traffic to THINGD_CLUSTER_LEADER_URL
```

Discovery modes:

```txt
none        no peer list
static      read THINGD_CLUSTER_PEERS
kubernetes  derive a service URL from THINGD_CLUSTER_SERVICE and namespace
```

Current bridge behavior: followers forward MCP traffic to the leader. Follower
local replica catch-up polls the leader every 500ms and applies replicated
events to the local SQLite database. If `THINGD_CLUSTER_LEADER_FALLBACK_URL` is
set, the follower falls back to that URL when the primary leader is unreachable.

### Leader Election (static config)

When `THINGD_CLUSTER_LEADER_ELECTION=true`, followers with a configured
`THINGD_CLUSTER_PEERS` list can automatically promote the next peer in the list
to leader when the current leader becomes unreachable.

- The peer list order defines succession priority (first entry = primary leader).
- When the leader is unreachable for `THINGD_CLUSTER_LEADER_ELECTION_MAX_FAILURES`
  consecutive replication cycles (each cycle is 500ms), the next peer in the list
  auto-promotes to leader.
- The promoted peer starts serving MCP writes directly and replication events
  for other followers.
- Other peers automatically redirect their `leaderUrl` to the new leader.
- **No split-brain prevention** — do not use with concurrent writes from
  multiple nodes attempting to become leader simultaneously. This is a
  single-leader failover for static deployments (Kubernetes StatefulSet with
  ordered pod names, Docker Compose with fixed service order).

## Runtime Endpoints

```txt
GET  /healthz
POST /mcp
GET  /cluster/status
GET  /cluster/peers
```

## MCP Resources

The MCP server exposes a `thingd://collections` resource via the MCP `resources/list` capability.
Agents can call `resources/list` to enumerate available collection names without using a tool.
If `THINGD_MCP_COLLECTIONS` is set, only allowed collections appear in the resource list.
