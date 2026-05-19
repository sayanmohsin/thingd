# Docker Runtime

`memoryd` can be packaged as a container that runs the MCP server over
Streamable HTTP.

This is the first remote-capable runtime shape. It is intended for local
experiments, self-hosting, and the future `memoryd-cloud` gateway.

## Build

```bash
docker build -t memoryd:local .
```

## Run

```bash
docker run --rm \
  -p 8757:8757 \
  -v memoryd-data:/data \
  -e MEMORYD_AUTH_TOKEN=change-me \
  memoryd:local
```

The container starts:

```txt
node packages/memoryd-mcp/dist/http-cli.js
```

Default container environment:

```txt
MEMORYD_PATH=/data/memoryd.db
MEMORYD_DRIVER=native
MEMORYD_HOST=0.0.0.0
MEMORYD_PORT=8757
MEMORYD_CLUSTER_MODE=single
MEMORYD_CLUSTER_DISCOVERY=none
MEMORYD_CLUSTER_PORT=8757
MEMORYD_MCP_AUDIT=true
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

Do not set `MEMORYD_ALLOW_UNAUTHENTICATED=true` in a public or shared runtime.

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
MEMORYD_URL=http://127.0.0.1:8757
MEMORYD_AUTH_TOKEN=change-me
```

```ts
const db = await MemoryD.open();
```

For ChatGPT or cloud-hosted agents, localhost is not enough. The MCP endpoint
must be available at a public HTTPS URL with authentication.

```txt
ChatGPT / hosted agent
  -> https://your-domain.example/mcp
  -> memoryd container
  -> /data/memoryd.db
```

Do not expose a tokenless MCP endpoint to the public internet.

## Audit Events

MCP write tools append audit events to `__memoryd:mcp:audit` by default.

Useful environment variables:

```txt
MEMORYD_MCP_AUDIT=true
MEMORYD_MCP_ACTOR=mcp-client
MEMORYD_MCP_SOURCE=memoryd-mcp
MEMORYD_MCP_AUDIT_STREAM=__memoryd:mcp:audit
```

Set `MEMORYD_MCP_AUDIT=false` only when you explicitly do not want MCP write
events recorded.

## Bridge Mode

The container accepts bridge/cluster env vars:

```txt
MEMORYD_CLUSTER_MODE=single|leader|follower
MEMORYD_CLUSTER_LEADER_URL=http://memoryd-leader:8757
MEMORYD_CLUSTER_FORWARD_AUTH_TOKEN=change-me
MEMORYD_CLUSTER_DISCOVERY=none|static|kubernetes
MEMORYD_CLUSTER_PEERS=http://memoryd-0:8757,http://memoryd-1:8757
MEMORYD_ADVERTISE_URL=http://memoryd-0:8757
```

Current behavior:

- `single`: standalone runtime
- `leader`: handles MCP traffic locally and reports itself writable
- `follower`: forwards MCP traffic to `MEMORYD_CLUSTER_LEADER_URL`

Follower local replica catch-up is not implemented yet. This bridge solves
write routing before attempting true replicated local reads.

## Smoke Test

```bash
npm run smoke:docker
```

The smoke script builds the image, starts a container, checks `/healthz`,
checks `/cluster/status`, and uses an MCP client to list tools. It maps the
container to host port `18757` by default to avoid clashing with a local dev
runtime. Override with `MEMORYD_DOCKER_PORT`.

## Compose And Kubernetes

- [deploy/docker-compose.yml](../deploy/docker-compose.yml) runs a local
  leader/follower pair.
- [deploy/kubernetes/sidecar.yaml](../deploy/kubernetes/sidecar.yaml) shows an
  app container with a local `memoryd` sidecar.
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
