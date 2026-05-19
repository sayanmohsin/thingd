# Runtime Environment

This is the current environment variable reference for the `memoryd` HTTP MCP
runtime and Docker sidecar.

## App SDK

```txt
MEMORYD_URL=http://127.0.0.1:8757
MEMORYD_AUTH_TOKEN=change-me
```

When `MEMORYD_URL` is set, `MemoryD.open()` uses the SDK remote driver and
talks to the sidecar over Streamable HTTP MCP. If the URL has no path, `/mcp` is
used automatically.

## Storage

```txt
MEMORYD_PATH=/data/memoryd.db
MEMORYD_DRIVER=native
```

`MEMORYD_DRIVER` can be `memory` or `native`. Use `native` for the Rust-backed
SQLite store after the native package has been built into the runtime image.

## HTTP

```txt
MEMORYD_HOST=0.0.0.0
MEMORYD_PORT=8757
MEMORYD_AUTH_TOKEN=change-me
MEMORYD_ALLOW_UNAUTHENTICATED=false
```

When `MEMORYD_HOST` is a non-loopback host, `MEMORYD_AUTH_TOKEN` is required.
`MEMORYD_ALLOW_UNAUTHENTICATED=true` is only for local experiments.

## MCP Audit

```txt
MEMORYD_MCP_AUDIT=true
MEMORYD_MCP_ACTOR=mcp-client
MEMORYD_MCP_SOURCE=memoryd-mcp
MEMORYD_MCP_AUDIT_STREAM=__memoryd:mcp:audit
```

Write tools append audit events by default. Set `MEMORYD_MCP_AUDIT=false` only
when you explicitly do not want MCP write events recorded.

## Bridge And Cluster

```txt
MEMORYD_CLUSTER_MODE=single
MEMORYD_CLUSTER_DISCOVERY=none
MEMORYD_CLUSTER_LEADER_URL=
MEMORYD_CLUSTER_FORWARD_AUTH_TOKEN=
MEMORYD_CLUSTER_PEERS=
MEMORYD_ADVERTISE_URL=
MEMORYD_CLUSTER_SERVICE=
MEMORYD_CLUSTER_NAMESPACE=default
MEMORYD_CLUSTER_PORT=8757
```

Modes:

```txt
single    standalone runtime, default
leader    owns writes locally
follower  forwards MCP traffic to MEMORYD_CLUSTER_LEADER_URL
```

Discovery modes:

```txt
none        no peer list
static      read MEMORYD_CLUSTER_PEERS
kubernetes  derive a service URL from MEMORYD_CLUSTER_SERVICE and namespace
```

Current bridge behavior is intentionally conservative: followers forward MCP
traffic to the leader. Follower local replica catch-up is not implemented yet.

## Runtime Endpoints

```txt
GET  /healthz
POST /mcp
GET  /cluster/status
GET  /cluster/peers
```
