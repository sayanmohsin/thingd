# Docker Runtime

`thingd` can be packaged as a container that runs the MCP server over
Streamable HTTP.

This is the first remote-capable runtime shape. It is intended for local
experiments, and self-hosting.

## Build

Local build:

```bash
docker build -f docker-context/Dockerfile -t thingd:local docker-context
```

## Multi-Arch Builds

The release workflow builds multi-arch images for `linux/amd64` and `linux/arm64`:

```bash
docker pull sayanmohsin/thingd:latest   # auto-selects your arch
```

Local multi-arch build:

```bash
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -f docker-context/Dockerfile \
  -t sayanmohsin/thingd:latest \
  --push docker-context
```

## Run

```bash
docker run --rm \
  -p 8757:8757 \
  -v thingd-data:/data \
  -e THINGD_AUTH_TOKEN=change-me \
  -e THINGD_ENCRYPTION_KEY=<64-hex-characters> \
  thingd:local
```

The container starts the static `thingd-server` binary:

```txt
thingd-server --config /etc/thingd/config.yaml
```

Default container environment:

```txt
THINGD_PATH=/data/thingd.db
THINGD_DRIVER=native
THINGD_ENCRYPTION_KEY=<64 hexadecimal characters, optional>
THINGD_HOST=0.0.0.0
THINGD_PORT=8757
THINGD_CLUSTER_MODE=single
THINGD_CLUSTER_DISCOVERY=none
THINGD_CLUSTER_PORT=8757
THINGD_MCP_AUDIT=true
```

## Endpoints

```txt
GET  /healthz
GET  /ready
POST /mcp
GET  /cluster/status
GET  /cluster/peers
```

`/mcp` requires a bearer token when the runtime binds to a non-loopback host
such as the container default `0.0.0.0`:

```txt
Authorization: Bearer <token>
```

Do not set `THINGD_ALLOW_UNAUTHENTICATED=true` in a public or shared runtime.

The encryption key is a startup secret for the native persistent directory. It
is not sent to MCP clients or included in MCP requests. Inject it through a
secret mechanism rather than committing it to Compose files or image layers.
Missing or wrong keys terminate startup; the server does not create an
in-memory replacement. Encrypted backups remain encrypted and require the same
key to restore.

## Health Check

```bash
curl http://127.0.0.1:8757/healthz
curl http://127.0.0.1:8757/cluster/status
```

## MCP Client URL

Local URL:

```txt
http://127.0.0.1:8757/mcp
```

Node apps can use the SDK remote driver through the sidecar:

```bash
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=change-me
```

```ts
const db = await ThingD.open();
```

For ChatGPT or cloud-hosted agents, localhost is not enough. The MCP endpoint
must be available at a public HTTPS URL with authentication.

```txt
ChatGPT / hosted agent
  -> https://your-domain.example/mcp
  -> thingd container
  -> /data/thingd.db
```

Do not expose a tokenless MCP endpoint to the public internet.

## Audit Events

MCP write tools append audit events to `__thingd:mcp:audit` by default.

Useful environment variables:

```txt
THINGD_MCP_AUDIT=true
THINGD_MCP_ACTOR=mcp-client
THINGD_MCP_SOURCE=thingd-mcp
THINGD_MCP_AUDIT_STREAM=__thingd:mcp:audit
```

Set `THINGD_MCP_AUDIT=false` only when you explicitly do not want MCP write
events recorded.

## Bridge Mode

The container accepts bridge/cluster env vars:

```txt
THINGD_CLUSTER_MODE=single|leader|follower
THINGD_CLUSTER_LEADER_URL=http://thingd-leader:8757
THINGD_CLUSTER_LEADER_FALLBACK_URL=http://thingd-leader-2:8757
THINGD_CLUSTER_FORWARD_AUTH_TOKEN=change-me
THINGD_CLUSTER_DISCOVERY=none|static|kubernetes
THINGD_CLUSTER_PEERS=http://thingd-0:8757,http://thingd-1:8757
THINGD_CLUSTER_LEADER_ELECTION=false
THINGD_CLUSTER_LEADER_ELECTION_MAX_FAILURES=3
THINGD_ADVERTISE_URL=http://thingd-0:8757
```

Supported cluster modes:

- `single`: Standalone runtime serving local database requests.
- `leader`: Handles local reads and writes, records change events to the system stream `__thingd:system:replication`, and serves incremental replication logs.
- `follower`: Enforces eventually consistent local reads and strict write forwarding:
  - **Write Forwarding**: Automatically forwards all incoming MCP write requests to the active Leader. If primary leader is unreachable and `THINGD_CLUSTER_LEADER_FALLBACK_URL` is set, the follower tries the fallback URL.
  - **Pull Replication**: In follower mode, replication applies change events to the follower's local persistent runtime. Active replication behavior and status are exposed through the cluster endpoints.

This cluster leader/follower mode is separate from provider-neutral
Thingd-to-Thingd sync. The latter uses `/v1/replication/events`,
`/v1/replication/apply`, `/v1/replication/status`, `/v1/replication/conflicts`,
and `/v1/replication/snapshot`, with explicit source/replica configuration and
cloud-target protection.

Replication lag and diagnostics are monitored dynamically via `/cluster/status`, which reports active peer sequence indexes and computed lag (events difference between leader and follower) for Kubernetes liveness/readiness probes.

## Smoke Test

```bash
pnpm smoke:docker
```

The smoke script builds the image, starts a container, checks `/healthz` and
`/ready`,
checks `/cluster/status`, and uses an MCP client to list tools. It maps the
container to host port `18757` by default to avoid clashing with a local dev
runtime. Override with `THINGD_DOCKER_PORT`.

## Compose And Kubernetes

- [deploy/docker-compose.yml](../deploy/docker-compose.yml) runs a local
  leader/follower pair.
- [deploy/kubernetes/sidecar.yaml](../deploy/kubernetes/sidecar.yaml) shows an
  app container with a local `thingd` sidecar.
- [deploy/kubernetes/leader-follower.yaml](../deploy/kubernetes/leader-follower.yaml)
  shows explicit leader/follower runtime env.
- [deploy/proxy/Caddyfile](../deploy/proxy/Caddyfile) shows a TLS reverse proxy
  shape.

## Failover

With leader election enabled, followers automatically detect leader failure and
promote the next peer in the ordered peer list:

```bash
THINGD_CLUSTER_LEADER_ELECTION=true
THINGD_CLUSTER_LEADER_ELECTION_MAX_FAILURES=3
THINGD_CLUSTER_PEERS=http://thingd-0:8757,http://thingd-1:8757,http://thingd-2:8757
THINGD_ADVERTISE_URL=http://thingd-1:8757
```

The election is static-config based — no Raft or distributed consensus. Best
suited for StatefulSets or environments with ordered, predictable pod names.

For full details, see [sidecar-cluster.md](./sidecar-cluster.md).

## Current Limitations

- no TLS termination inside the container
- no OAuth
- no multi-tenant routing
- static-config leader election only (no consensus)

Put TLS, domains, and public exposure behind a proper reverse proxy or hosted
gateway. For cluster details, see [sidecar-cluster.md](./sidecar-cluster.md).
