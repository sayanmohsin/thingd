# thingd

A fast object-first data engine for applications and AI agents.

thingd is a high-performance object-first data engine that combines persistent storage, durable queues, event streams, full-text search, and MCP-native access into a single system that can run embedded, standalone, or in the cloud.

## Quick start

```bash
docker run -p 8757:8757 sayanmohsin/thingd
```

This starts an HTTP MCP server at `http://localhost:8757/mcp`.

Multi-arch images are available for `linux/amd64` and `linux/arm64`.

## Persist data

```bash
docker run -p 8757:8757 \
  -v ./data:/data \
  sayanmohsin/thingd
```

Data is stored at `/data/thingd.db` inside the container.

## Securing the server

```bash
docker run -p 8757:8757 \
  -e THINGD_AUTH_TOKEN=your-secret \
  -e THINGD_ENCRYPTION_KEY=<64-hex-characters> \
  sayanmohsin/thingd
```

Without an auth token, the server only binds to loopback (127.0.0.1). Setting `THINGD_AUTH_TOKEN` enables non-loopback binding.

The encryption key is consumed at database startup and is never sent to MCP
clients. Use an orchestration secret rather than committing it to a Compose
file or image layer. Missing or wrong keys stop startup instead of creating a
memory database. Encrypted directory backups require the same key to restore.

## Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `THINGD_PATH` | `/data/thingd.db` | Path to the persistent database directory |
| `THINGD_DRIVER` | `native` | Storage driver (`native` or `memory`) |
| `THINGD_STORAGE_BACKEND` | `rocksdb` | Durable backend (`rocksdb` or experimental `thingdb`) |
| `THINGD_ENCRYPTION_KEY` | — | Optional 64-character hexadecimal key for native persistent storage |
| `THINGD_SEARCH_MODE` | `persistent` | `persistent`, `persistent-async`, `persistent-no-rebuild`, or `disabled` |
| `THINGD_SEARCH_COMMIT_INTERVAL_MS` | `250` | Debounce interval before the async Tantivy commit |
| `THINGD_SEARCH_COMMIT_BATCH_SIZE` | `32` | Maximum coalesced search mutations per commit |
| `THINGD_SEARCH_QUEUE_MAX_KEYS` | `10000` | Bounded search queue capacity; overflow uses fallback search |
| `THINGD_HOST` | `0.0.0.0` | Bind address |
| `THINGD_PORT` | `8757` | HTTP server port |
| `THINGD_AUTH_TOKEN` | — | Bearer token for `/mcp` endpoint. Required for non-loopback binding |
| `THINGD_ALLOW_UNAUTHENTICATED` | `false` | Set to `true` to allow non-loopback binding without auth (local experiments only) |
| `THINGD_MCP_AUDIT` | `true` | Enable audit events for MCP write tools |
| `THINGD_MCP_ACTOR` | — | Default actor name for MCP audit events |
| `THINGD_MCP_SOURCE` | — | Default source name for MCP audit events |
| `THINGD_MCP_COLLECTIONS` | — | Comma-separated allowlist of collection names |
| `THINGD_MCP_READ_ONLY` | `false` | Set to `true` to disable all write tools |
| `THINGD_MCP_MAX_PAYLOAD_BYTES` | `524288` | Maximum MCP request payload size in bytes |

RocksDB is the default durable backend. Set `THINGD_STORAGE_BACKEND=thingdb`
only for the experimental Rust-native backend; it uses a separate format and
requires logical repack when changing an existing database. See
[Storage backends](./storage-backends.md) for compatibility and rollback-safe
repack guidance.

## Cluster mode

thingd supports leader/follower cluster deployments.

```bash
# Leader
docker run -p 8757:8757 \
  -e THINGD_CLUSTER_MODE=leader \
  sayanmohsin/thingd

# Follower
docker run -p 8757:8757 \
  -e THINGD_CLUSTER_MODE=follower \
  -e THINGD_CLUSTER_LEADER_URL=http://leader:8757 \
  sayanmohsin/thingd
```

### Cluster environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `THINGD_CLUSTER_MODE` | `single` | `single`, `leader`, or `follower` |
| `THINGD_CLUSTER_LEADER_URL` | — | Leader URL (required for followers) |
| `THINGD_CLUSTER_LEADER_FALLBACK_URL` | — | Fallback leader URL for basic failover |
| `THINGD_CLUSTER_FORWARD_AUTH_TOKEN` | — | Bearer token for cross-pod forwarding. Falls back to `THINGD_AUTH_TOKEN` |
| `THINGD_CLUSTER_PEERS` | — | Comma-separated peer URLs for discovery |
| `THINGD_CLUSTER_DISCOVERY` | — | `none`, `static`, or `kubernetes` |
| `THINGD_CLUSTER_SERVICE` | — | Kubernetes service name for DNS discovery |
| `THINGD_CLUSTER_NAMESPACE` | `default` | Kubernetes namespace |
| `THINGD_CLUSTER_PORT` | `8757` | Port for Kubernetes service discovery |
| `THINGD_CLUSTER_LEADER_ELECTION` | `false` | Enable automatic leader failover via static peer list |
| `THINGD_CLUSTER_LEADER_ELECTION_MAX_FAILURES` | `3` | Consecutive replication failures before triggering election |
| `THINGD_ADVERTISE_URL` | — | Self-advertised URL reported in cluster status |

## Docker Compose

```yaml
services:
  thingd:
    image: sayanmohsin/thingd
    ports:
      - "8757:8757"
    volumes:
      - ./data:/data
    environment:
      - THINGD_AUTH_TOKEN=change-me
```

See the [deploy/docker-compose.yml](https://github.com/sayanmohsin/thingd/blob/main/deploy/docker-compose.yml) for a full leader/follower example.

### Leader election

Enable automatic failover for static deployments:

```bash
docker run -p 8757:8757 \
  -e THINGD_CLUSTER_MODE=follower \
  -e THINGD_CLUSTER_LEADER_URL=http://leader:8757 \
  -e THINGD_CLUSTER_LEADER_ELECTION=true \
  -e THINGD_CLUSTER_PEERS=http://leader:8757,http://follower:8757 \
  -e THINGD_ADVERTISE_URL=http://follower:8757 \
  sayanmohsin/thingd
```

See [runtime-env.md](./runtime-env.md) and [mcp-server.md](./mcp-server.md) for the public cluster and runtime configuration reference.

## Kubernetes

Kubernetes deployment manifests (sidecar and leader/follower) are available in the [deploy/kubernetes](https://github.com/sayanmohsin/thingd/tree/main/deploy/kubernetes) directory.

## Connecting with the SDK

```ts
import { ThingD } from "@thingd/sdk";

const db = await ThingD.open({
  url: "http://localhost:8757/mcp",
  authToken: "change-me",
  driver: "cloud",
});
```

## Health check

```
GET /healthz
GET /ready
```

## Cluster status

```
GET /cluster/status
```

## Managed hosting

Don't want to run your own server? [thingd.cloud](https://thingd.cloud)
hosts thingd for you — managed MCP endpoints, API key management, team
dashboard, tenant isolation, and backups.

## Links

- [GitHub](https://github.com/sayanmohsin/thingd)
- [npm (SDK)](https://www.npmjs.com/package/@thingd/sdk)
- [npm (CLI)](https://www.npmjs.com/package/@thingd/cli)
- [Documentation](https://github.com/sayanmohsin/thingd/tree/main/docs)
