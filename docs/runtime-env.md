# Runtime Environment

This is the current environment variable reference for the `thingd` HTTP MCP
runtime and Docker sidecar.

## App SDK

```txt
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=change-me
```

When `THINGD_URL` is set, `ThingD.open()` uses the SDK remote driver and
talks to the sidecar over Streamable HTTP MCP. If the URL has no path, `/mcp` is
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
```

When `THINGD_HOST` is a non-loopback host, `THINGD_AUTH_TOKEN` is required.
`THINGD_ALLOW_UNAUTHENTICATED=true` is only for local experiments.

## MCP Audit

```txt
THINGD_MCP_AUDIT=true
THINGD_MCP_ACTOR=mcp-client
THINGD_MCP_SOURCE=thingd-mcp
THINGD_MCP_AUDIT_STREAM=__thingd:mcp:audit
```

Write tools append audit events by default. Set `THINGD_MCP_AUDIT=false` only
when you explicitly do not want MCP write events recorded.

## Bridge And Cluster

```txt
THINGD_CLUSTER_MODE=single
THINGD_CLUSTER_DISCOVERY=none
THINGD_CLUSTER_LEADER_URL=
THINGD_CLUSTER_FORWARD_AUTH_TOKEN=
THINGD_CLUSTER_PEERS=
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

Current bridge behavior is intentionally conservative: followers forward MCP
traffic to the leader. Follower local replica catch-up is not implemented yet.

## Runtime Endpoints

```txt
GET  /healthz
POST /mcp
GET  /cluster/status
GET  /cluster/peers
```
