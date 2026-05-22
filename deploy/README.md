# Deployment Examples

These examples show the current `thingd` runtime shape.

## Docker Compose

```bash
docker compose -f deploy/docker-compose.yml up --build
```

This starts:

- `thingd-leader` on `http://127.0.0.1:8757`
- `thingd-follower` on `http://127.0.0.1:8758`

Both use the same demo bearer token: `change-me`.

Point a Node app at either runtime with:

```bash
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=change-me
```

Then app code can use:

```ts
const db = await ThingD.open();
```

## Kubernetes

```bash
kubectl apply -f deploy/kubernetes/sidecar.yaml
```

The sidecar example places `thingd` next to an app container and exposes it on
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
  -> https://thingd.example.com/mcp
  -> Caddy
  -> thingd leader
```

Keep `THINGD_AUTH_TOKEN` enabled behind the proxy.
