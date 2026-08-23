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
THINGD_STORAGE_BACKEND=rocksdb|thingdb
THINGD_ENCRYPTION_KEY=<64 hexadecimal characters>
THINGD_SEARCH_MODE=persistent
THINGD_SEARCH_COMMIT_INTERVAL_MS=250
THINGD_SEARCH_COMMIT_BATCH_SIZE=32
THINGD_SEARCH_QUEUE_MAX_KEYS=10000
THINGD_JOURNAL_MAX_BYTES=33554432
THINGD_RECOVERY_BATCH_SIZE=32
THINGD_RECOVERY_PAUSE_MS=50
THINGD_RECOVERY_MAX_RETRIES=3
THINGD_RECOVERY_MEMORY_LIMIT_BYTES=<optional bytes>
THINGD_NATIVE_MAX_PAYLOAD_BYTES=<optional bytes>
THINGD_NATIVE_MAX_BATCH_ITEMS=<optional count>
```

`THINGD_DRIVER` can be `memory` or `native`. Use `native` for the Rust-backed
persistent store after the native package has been built into the runtime image.
For native Node or server `:memory:` operation, ThingDB RAM is selected by
default and creates no durable files. The explicit `memory` driver remains the
portable TypeScript/reference implementation.

Use `ThingD.open(":memory:")` when the application needs the full Thingd
in-memory database contract. For a bounded transient key/value cache with TTL
and LRU behavior, use the ThingDB cache API instead; neither mode survives
process exit or replaces durable storage.

`THINGD_STORAGE_BACKEND` defaults to `rocksdb`. Set it to `thingdb` only for
the experimental Rust-native backend. ThingDB uses a separate on-disk format;
switching an existing directory requires logical repack rather than changing
the environment variable in place.

`THINGD_ENCRYPTION_KEY` is optional and applies only to native persistent
storage. It must represent exactly 32 bytes as 64 hexadecimal characters. Do
not put it in source control, container image layers, MCP configuration, URLs,
or request payloads. Disposable ThingDB RAM, explicit memory, and cloud drivers
reject this local option rather than ignoring it. Changing the value does not
rotate an existing database.

`THINGD_SEARCH_MODE=persistent` uses staged startup: it opens primary storage
without Tantivy, compacts primary storage, then rebuilds missing or incompatible
Tantivy state with bounded batches and pauses. During storage recovery, search
uses the bounded fallback scan, `/ready` returns `503`, and writes return `503`
with `Retry-After: 1`. After startup, persistent indexing is asynchronous and
eventually consistent: a durable write is acknowledged after primary storage
commits, while one background worker coalesces Tantivy updates and commits them
at the configured interval or batch size. `persistent-async` selects the same
write behavior explicitly. `persistent-no-rebuild` never mutates Tantivy and
always uses fallback scanning for search. `disabled` avoids opening Tantivy
entirely. For hosts with less than 2 GB RAM, prefer a separate standalone
thingd-server over HTTP.

The search queue is bounded by `THINGD_SEARCH_QUEUE_MAX_KEYS`. Overflow or a
Tantivy failure preserves the durable write, marks search stale, serves fallback
search, and schedules a bounded rebuild. `THINGD_SEARCH_COMMIT_INTERVAL_MS`
and `THINGD_SEARCH_COMMIT_BATCH_SIZE` control the debounce and commit batch.
Normal search lag does not make `/ready` fail while primary storage is healthy.

On small instances, start the sidecar before write-heavy clients and wait for
`/ready`. Catalog seeders and other mutation clients must retry `503` responses
according to `Retry-After: 1`; restarting thingd in response to those responses
can restart recovery and increase resource pressure.
The defaults are conservative for approximately 1 GB hosts. A configured
memory ceiling fails recovery closed instead of allowing the process to thrash.
After failed recovery, stop writers and use `thingd db compact --path <path>`
or `thingd db repack --path <source> --destination <destination>`.

`THINGD_NATIVE_MAX_PAYLOAD_BYTES` optionally bounds JSON batch payloads passed
through the embedded native binding. It is unset by default for compatibility;
low-memory deployments should set an explicit limit appropriate for their
workload. `THINGD_NATIVE_MAX_BATCH_ITEMS` applies the same opt-in protection to
the number of objects, events, or queue jobs in a native batch.

The native durable backend is RocksDB compiled into the server/native artifact
by default; set `THINGD_STORAGE_BACKEND=thingdb` to opt into the experimental
Rust-native backend. These settings do not require a RocksDB service. Current
releases do not open or automatically convert legacy native directories. See
[Storage backends](./storage-backends.md).

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
  storage_backend: "rocksdb" # or experimental "thingdb"
  encryption_key: "<64 hexadecimal characters>"
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
events to the local persistent runtime. If `THINGD_CLUSTER_LEADER_FALLBACK_URL` is
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
