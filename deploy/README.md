# Deployment Examples

These examples show the current `memoryd` runtime shape.

## Docker Compose

```bash
docker compose -f deploy/docker-compose.yml up --build
```

This starts:

- `memoryd-leader` on `http://127.0.0.1:8757`
- `memoryd-follower` on `http://127.0.0.1:8758`

Both use the same demo bearer token: `change-me`.

Point a Node app at either runtime with:

```bash
MEMORYD_URL=http://127.0.0.1:8757
MEMORYD_AUTH_TOKEN=change-me
```

Then app code can use:

```ts
const db = await MemoryD.open();
```

## Kubernetes

```bash
kubectl apply -f deploy/kubernetes/sidecar.yaml
```

The sidecar example places `memoryd` next to an app container and exposes it on
`http://127.0.0.1:8757` inside the pod.

```bash
kubectl apply -f deploy/kubernetes/leader-follower.yaml
```

The leader/follower example shows explicit bridge env vars. Followers forward
MCP traffic to the leader.

## TLS Proxy

`deploy/proxy/Caddyfile` shows the public HTTPS shape:

```txt
agent or ChatGPT
  -> https://memoryd.example.com/mcp
  -> Caddy
  -> memoryd leader
```

Keep `MEMORYD_AUTH_TOKEN` enabled behind the proxy.
