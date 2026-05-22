# Docker Runtime

`thingd` can be packaged as a container that runs the MCP server over
Streamable HTTP.

This is the first remote-capable runtime shape. It is intended for local
experiments, self-hosting, and the future `thingd-cloud` gateway.

## Build

```bash
docker build -t thingd:local .
```

## Run

```bash
docker run --rm \
  -p 8757:8757 \
  -v thingd-data:/data \
  -e THINGD_AUTH_TOKEN=change-me \
  thingd:local
```

The container starts:

```txt
node packages/thingd-mcp/dist/http-cli.js
```

Default container environment:

```txt
THINGD_PATH=/data/thingd.db
THINGD_DRIVER=native
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
THINGD_CLUSTER_FORWARD_AUTH_TOKEN=change-me
THINGD_CLUSTER_DISCOVERY=none|static|kubernetes
THINGD_CLUSTER_PEERS=http://thingd-0:8757,http://thingd-1:8757
THINGD_ADVERTISE_URL=http://thingd-0:8757
```

Current behavior:

- `single`: standalone runtime
- `leader`: handles MCP traffic locally and reports itself writable
- `follower`: forwards MCP traffic to `THINGD_CLUSTER_LEADER_URL`

Follower local replica catch-up is not implemented yet. This bridge solves
write routing before attempting true replicated local reads.

## Smoke Test

```bash
pnpm smoke:docker
```

The smoke script builds the image, starts a container, checks `/healthz`,
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

## Current Limitations

- no TLS termination inside the container
- no OAuth
- no multi-tenant routing
- no production prebuild matrix
- no follower local replica catch-up yet

Put TLS, domains, and public exposure behind a proper reverse proxy or hosted
gateway.
