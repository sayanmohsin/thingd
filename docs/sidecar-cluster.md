# Sidecar Cluster

thingd supports leader/follower cluster deployments using a sidecar pattern. Each
application pod runs a local `thingd` sidecar that persists data in SQLite.
Followers forward writes to the leader and pull-replicate changes in the
background.

## Cluster Modes

| Mode | Behavior |
|------|----------|
| `single` | Standalone runtime. Local reads and writes. No replication. |
| `leader` | Handles local reads and writes. Records change events to `__thingd:system:replication`. Serves incremental replication logs to followers. |
| `follower` | Forwards all MCP write requests to the leader. Pull-replicates changes every 500ms. Local reads are eventually consistent. |

Set the mode with `THINGD_CLUSTER_MODE`:

```bash
THINGD_CLUSTER_MODE=leader  # or follower, single
```

## Peer Discovery

### Static

List peer URLs explicitly:

```bash
THINGD_CLUSTER_DISCOVERY=static
THINGD_CLUSTER_PEERS=http://thingd-0:8757,http://thingd-1:8757,http://thingd-2:8757
```

### Kubernetes DNS

Auto-discover peers via Kubernetes service DNS:

```bash
THINGD_CLUSTER_DISCOVERY=kubernetes
THINGD_CLUSTER_SERVICE=thingd
THINGD_CLUSTER_NAMESPACE=default
THINGD_CLUSTER_PORT=8757
```

This resolves to `http://thingd.<namespace>.svc.cluster.local:<port>`.

## Write Forwarding

Followers automatically forward all MCP POST requests to the configured leader.
If the primary leader is unreachable and `THINGD_CLUSTER_LEADER_FALLBACK_URL` is
set, the follower tries the fallback URL before returning a 503 error.

```bash
THINGD_CLUSTER_MODE=follower
THINGD_CLUSTER_LEADER_URL=http://thingd-0:8757
THINGD_CLUSTER_LEADER_FALLBACK_URL=http://thingd-1:8757
```

## Replication

Followers pull changes from the leader every 500ms via
`GET /v1/replication/events?after=<sequence>`. Replicated events include:

- `replication.objects.put` — object created or updated
- `replication.objects.delete` — object removed
- `replication.events.append` — event appended to a stream

Internal collections (`__thingd*`) are excluded from replication.

Replication status is persisted in `__thingd_meta.replication_status` and
reported via `/cluster/status`.

## Leader Election

Leader election provides automatic failover for static deployments (e.g.,
Kubernetes StatefulSets with ordered pod names).

```bash
THINGD_CLUSTER_LEADER_ELECTION=true
THINGD_CLUSTER_LEADER_ELECTION_MAX_FAILURES=3
THINGD_CLUSTER_PEERS=http://thingd-0:8757,http://thingd-1:8757,http://thingd-2:8757
THINGD_ADVERTISE_URL=http://thingd-1:8757
```

### How it works

1. Follower polls leader for replication events every 500ms.
2. If the poll fails, `consecutiveReplicationFailures` increments.
3. After `electionMaxFailures` (default 3) consecutive failures, election triggers.
4. `findNextLeaderCandidate()` scans the ordered peer list starting after the
   current leader's position.
5. If the next candidate is **this node** (`advertiseUrl` matches), it promotes
   itself to leader.
6. If the next candidate is a **different peer**, the follower redirects its
   `leaderUrl` to that peer and retries.
7. The peer list wraps around — after the last peer, it tries from the beginning.

### Example: 3-pod StatefulSet

```yaml
# thingd-0 (leader)
THINGD_CLUSTER_MODE=leader
THINGD_CLUSTER_PEERS=http://thingd-0:8757,http://thingd-1:8757,http://thingd-2:8757
THINGD_ADVERTISE_URL=http://thingd-0:8757

# thingd-1 (follower)
THINGD_CLUSTER_MODE=follower
THINGD_CLUSTER_LEADER_URL=http://thingd-0:8757
THINGD_CLUSTER_LEADER_ELECTION=true
THINGD_CLUSTER_PEERS=http://thingd-0:8757,http://thingd-1:8757,http://thingd-2:8757
THINGD_ADVERTISE_URL=http://thingd-1:8757

# thingd-2 (follower)
THINGD_CLUSTER_MODE=follower
THINGD_CLUSTER_LEADER_URL=http://thingd-0:8757
THINGD_CLUSTER_LEADER_ELECTION=true
THINGD_CLUSTER_PEERS=http://thingd-0:8757,http://thingd-1:8757,http://thingd-2:8757
THINGD_ADVERTISE_URL=http://thingd-2:8757
```

If `thingd-0` goes down:
- `thingd-1` detects 3 consecutive replication failures
- Election triggers, next candidate after `thingd-0` is `thingd-1` (self)
- `thingd-1` promotes itself to leader
- `thingd-2` eventually detects the failure, next candidate is `thingd-2` (self),
  but `thingd-1` is already leader — `thingd-2` redirects to `thingd-1`

### Limitations

- **Static peer list only** — no dynamic discovery or Raft consensus
- **No split-brain protection** — in a network partition, two nodes could
  independently promote. Use in environments with reliable network (e.g.,
  single-AZ Kubernetes).
- **No fencing** — a stale leader could briefly accept writes after a new leader
  is elected. The replication stream is append-only so this is unlikely to cause
  data loss, but writes to the old leader won't replicate until it steps down.
- **No re-election on leader recovery** — if the old leader comes back, it
  does not automatically step down. Restart it in `follower` mode.

## Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/healthz` | GET | Health check with cluster status |
| `/cluster/status` | GET | Full cluster status (mode, replication lag, peers, election config) |
| `/cluster/peers` | GET | Peer list and discovery mode |
| `/mcp` | POST | MCP endpoint (forwarded to leader on followers) |
| `/v1/replication/events` | GET | Incremental replication log (leader only) |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `THINGD_CLUSTER_MODE` | `single` | `single`, `leader`, or `follower` |
| `THINGD_CLUSTER_LEADER_URL` | — | Leader URL (required for followers) |
| `THINGD_CLUSTER_LEADER_FALLBACK_URL` | — | Fallback leader URL |
| `THINGD_CLUSTER_FORWARD_AUTH_TOKEN` | — | Bearer token for cross-pod forwarding. Falls back to `THINGD_AUTH_TOKEN` |
| `THINGD_CLUSTER_PEERS` | — | Comma-separated peer URLs |
| `THINGD_CLUSTER_DISCOVERY` | `none` | `none`, `static`, or `kubernetes` |
| `THINGD_CLUSTER_SERVICE` | — | Kubernetes service name for DNS discovery |
| `THINGD_CLUSTER_NAMESPACE` | `default` | Kubernetes namespace |
| `THINGD_CLUSTER_PORT` | `8757` | Port for Kubernetes service discovery |
| `THINGD_CLUSTER_LEADER_ELECTION` | `false` | Enable automatic leader failover |
| `THINGD_CLUSTER_LEADER_ELECTION_MAX_FAILURES` | `3` | Consecutive failures before election |
| `THINGD_ADVERTISE_URL` | — | Self-advertised URL for cluster status |

## Docker Compose

A local leader/follower pair:

```yaml
services:
  leader:
    image: sayanmohsin/thingd
    ports:
      - "8757:8757"
    volumes:
      - leader-data:/data
    environment:
      - THINGD_AUTH_TOKEN=change-me
      - THINGD_CLUSTER_MODE=leader
      - THINGD_CLUSTER_PEERS=http://leader:8757,http://follower:8757
      - THINGD_ADVERTISE_URL=http://leader:8757

  follower:
    image: sayanmohsin/thingd
    ports:
      - "8758:8757"
    volumes:
      - follower-data:/data
    environment:
      - THINGD_AUTH_TOKEN=change-me
      - THINGD_CLUSTER_MODE=follower
      - THINGD_CLUSTER_LEADER_URL=http://leader:8757
      - THINGD_CLUSTER_LEADER_ELECTION=true
      - THINGD_CLUSTER_PEERS=http://leader:8757,http://follower:8757
      - THINGD_ADVERTISE_URL=http://follower:8757
    depends_on:
      - leader

volumes:
  leader-data:
  follower-data:
```

See [deploy/docker-compose.yml](../deploy/docker-compose.yml) for the full
example.

## Kubernetes

- [deploy/kubernetes/sidecar.yaml](../deploy/kubernetes/sidecar.yaml) — app
  container with a local `thingd` sidecar in `single` mode.
- [deploy/kubernetes/leader-follower.yaml](../deploy/kubernetes/leader-follower.yaml)
  — explicit leader/follower with StatefulSet-style naming.

## Cluster Status Response

`GET /cluster/status` returns:

```json
{
  "mode": "follower",
  "writable": false,
  "forwarding": true,
  "leaderUrl": "http://thingd-0:8757",
  "activeLeaderUrl": "http://thingd-0:8757",
  "discovery": "static",
  "peers": ["http://thingd-0:8757", "http://thingd-1:8757"],
  "leaderElection": true,
  "electionMaxFailures": 3,
  "replication": {
    "lastReplicatedSequence": 42,
    "status": "syncing",
    "lag": 2
  }
}
```

Use `lag` for Kubernetes liveness/readiness probes.

## Testing

```bash
pnpm test:cli    # includes cluster.test.mjs (5 tests)
pnpm smoke:docker  # builds image, checks /healthz and /cluster/status
```
