# Sidecar And Cluster Runtime

This document describes the current sidecar/bridge shape and the remaining
cluster work.

Current implementation:

- Docker runtime starts the Streamable HTTP MCP server.
- `/healthz`, `/cluster/status`, and `/cluster/peers` are available.
- `THINGD_CLUSTER_MODE=single|leader|follower` is parsed by the runtime.
- followers forward MCP traffic to `THINGD_CLUSTER_LEADER_URL`.
- static peer lists and Kubernetes service hints are exposed in status.

Not implemented yet:

- automatic leader election
- consensus/failover

The goal is to keep app integration simple while letting `thingd` handle the
bridge between pods when an app runs in Kubernetes or another clustered
environment.

## Product Goal

Apps should use one public API:

```ts
import { ThingD } from "thingd";

const db = await ThingD.open();
```

Deployment decides what `open()` connects to:

- local development can use embedded SQLite
- a single server can use a local `thingd` process
- Kubernetes can use a local sidecar
- cluster mode can forward writes to the current leader

App code should not need to know which pod is leader.

## Runtime Modes

### Embedded Mode

No separate process.

```txt
Node app
  -> thingd
  -> native Rust binding
  -> local SQLite file
```

This is the simplest mode for local development, single-process apps, CLIs,
desktop tools, and small services.

### Server Mode

One standalone `thingd` process owns the database file.

```txt
Node app
  -> thingd server
  -> local SQLite file
```

This is useful when multiple processes on one machine should share the same
store without each process opening SQLite directly.

### Sidecar Mode

One `thingd` sidecar runs beside the app container in the same pod.

```txt
Pod
  app container
    -> http://127.0.0.1:8757
  thingd sidecar
    -> /data/thingd.db
```

The app only talks to localhost. The sidecar owns storage, queue leases, MCP
tools, and future bridge behavior.

### Cluster Mode

Every pod can have a sidecar. In the current bridge scaffold, deployment tells
each sidecar whether it is `leader` or `follower`.

```txt
Pod A sidecar = leader
Pod B sidecar = follower
Pod C sidecar = follower
```

MCP traffic from any pod goes to the local sidecar first. If that sidecar is a
follower, it forwards the request to the configured leader.

```txt
Pod B app
  -> Pod B thingd sidecar
  -> forwards MCP request to Pod A leader
  -> leader writes SQLite
  -> leader appends event
```

This is the major difference from plain SQLite: SQLite stores local state, while
`thingd` starts to own discovery metadata, write routing, queue leasing, and
agent-safe APIs. Replicated local reads are intentionally not claimed yet.

## Node API Shape

Keep the public API centered on `ThingD`.

```ts
const db = await ThingD.open();
```

Resolution order:

1. `THINGD_URL` set: connect to server or sidecar.
2. explicit `path` plus `driver: "native"`: open embedded SQLite through the
   native binding.
3. no env: use the in-memory proof store for development.

Explicit targets:

```ts
await ThingD.open("thingd://127.0.0.1:8757");
await ThingD.open("http://127.0.0.1:8757");
```

Explicit options:

```ts
await ThingD.open({
  path: "./thingd.db",
  driver: "native",
});

await ThingD.open({
  url: "http://127.0.0.1:8757/mcp",
  driver: "remote",
  authToken: "change-me",
});
```

`store` remains useful for tests and custom adapters.

The remote SDK driver uses MCP over Streamable HTTP. A separate HTTP+JSON app
API may still come later if benchmarks or ergonomics justify it.

## Server API Shape

The first server protocol can be HTTP+JSON because it is easy to debug and easy
for agents to inspect. A faster protocol can come later if benchmarks justify
it.

Planned app routes:

```txt
PUT    /v1/objects/:collection/:id
GET    /v1/objects/:collection/:id
DELETE /v1/objects/:collection/:id
POST   /v1/events/:stream
GET    /v1/events
POST   /v1/queues/:queue/jobs
POST   /v1/queues/:queue/claim
POST   /v1/queues/:queue/jobs/:id/ack
POST   /v1/queues/:queue/jobs/:id/nack
GET    /v1/queues/:queue/jobs
GET    /v1/queues/:queue/dead
POST   /v1/search
```

Current cluster routes:

```txt
GET    /healthz
GET    /cluster/status
GET    /cluster/peers
POST   /mcp
```

Planned app and replication routes:

```txt
GET    /v1/replication/events?after=:sequence
POST   /v1/replication/apply
```

Every mutating route should accept an idempotency key so forwarding retries do
not duplicate writes.

## Environment Variables

Env vars should make app and sidecar setup boring.

App container:

```bash
THINGD_URL=http://127.0.0.1:8757
```

Embedded mode:

```bash
THINGD_DRIVER=native
THINGD_PATH=./thingd.db
```

Server or sidecar mode:

```bash
THINGD_HOST=0.0.0.0
THINGD_PORT=8757
THINGD_PATH=/data/thingd.db
THINGD_CLUSTER_MODE=single
```

Current cluster/bridge mode:

```bash
THINGD_HOST=0.0.0.0
THINGD_PORT=8757
THINGD_ADVERTISE_URL=http://$(POD_IP):8757
THINGD_CLUSTER_DISCOVERY=kubernetes
THINGD_CLUSTER_SERVICE=thingd
THINGD_CLUSTER_NAMESPACE=default
THINGD_CLUSTER_MODE=leader
```

Static peer fallback:

```bash
THINGD_CLUSTER_DISCOVERY=static
THINGD_CLUSTER_PEERS=http://thingd-0:8757,http://thingd-1:8757
```

Follower forwarding:

```bash
THINGD_CLUSTER_MODE=follower
THINGD_CLUSTER_LEADER_URL=http://thingd-leader:8757
THINGD_CLUSTER_FORWARD_AUTH_TOKEN=change-me
```

## Kubernetes Shape

App pod with sidecar examples live in:

```txt
deploy/kubernetes/sidecar.yaml
deploy/kubernetes/leader-follower.yaml
```

Minimal app pod with sidecar:

```yaml
containers:
  - name: app
    image: your-node-app
    env:
      - name: THINGD_URL
        value: http://127.0.0.1:8757

  - name: thingd
    image: ghcr.io/sayanmohsin/thingd
    env:
      - name: THINGD_PATH
        value: /data/thingd.db
      - name: THINGD_HOST
        value: 0.0.0.0
      - name: THINGD_CLUSTER_MODE
        value: single
    ports:
      - containerPort: 8757
    volumeMounts:
      - name: thingd-data
        mountPath: /data
```

Service for peer discovery metadata:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: thingd
spec:
  clusterIP: None
  selector:
    app: your-app
  ports:
    - name: thingd
      port: 8757
```

## Bridge Helpers

- peer discovery
- write forwarding
- health/readiness status
- MCP write attribution
- event-log replication and follower catch-up
- local versus strong read consistency

Current helpers:

- **leader fallback URL** (`THINGD_CLUSTER_LEADER_FALLBACK_URL`): when the primary
  leader is unreachable, followers try this URL for both write forwarding and
  replication polling. The active leader URL is reported in `/cluster/status`.

Future helpers:

- automatic leader election
- idempotent forwarded writes
- queue claim forwarding with failover semantics

The app should not call these helpers directly. They are server-side internals
used by the sidecar and cluster runtime.

Reads:

- **Strong consistency (`strong`)**: Route to the leader for strong read-after-write guarantees.
- **Eventual consistency (`local`)**: Read directly from the follower's local SQLite replica, drastically increasing read throughput and offloading the leader.

Events and Replication:

- Leader assigns a monotonic event sequence to all change events written to `__thingd:system:replication`.
- Followers poll `GET /v1/replication/events?after=:sequence` every `500ms` and apply the mutations locally to keep in sync.
- Local objects, search indexes, and timelines are derived directly from the replicated change events.

## Non-goals For First Cluster Version

- no multi-primary writes
- no automatic conflict merge between independently written SQLite files
- no consensus protocol unless demand proves it is needed
- no strict read-after-write guarantee from local followers unless the caller
  asks for `strong`

## Phase Plan

### Sidecar Phase A - Server Binary

- add `crates/thingd-server`
- expose HTTP+JSON API over `thingd-core`
- support `THINGD_PATH`, `THINGD_HOST`, `THINGD_PORT`, and health checks
- keep cluster disabled

### Sidecar Phase B - Docker And Sidecar Mode

- [x] build Docker runtime image scaffold
- [x] add Kubernetes sidecar example
- [x] app connects through `THINGD_URL=http://127.0.0.1:8757`
- [x] document readiness/liveness checks

### Sidecar Phase C - Cluster Bridge

- [x] add peer metadata
- [x] add static peer mode first
- [x] add Kubernetes service discovery metadata
- [x] add follower MCP forwarding to configured leader
- [x] add leader fallback URL for basic failover

### Sidecar Phase D - Replication

- [x] add event replication from leader to followers
- [x] add local/strong read consistency option
- [ ] add leader election

### Sidecar Phase E - Cluster Hardening

- [x] follower catch-up tests
- [x] observability metrics
- [ ] leader failover tests
- [ ] idempotency tests for forwarded writes
- [ ] queue claim tests across sidecars
- [ ] optional Helm chart
