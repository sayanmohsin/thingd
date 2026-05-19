# Sidecar And Cluster Runtime

This document describes the current sidecar/bridge shape and the remaining
cluster work.

Current implementation:

- Docker runtime starts the Streamable HTTP MCP server.
- `/healthz`, `/cluster/status`, and `/cluster/peers` are available.
- `MEMORYD_CLUSTER_MODE=single|leader|follower` is parsed by the runtime.
- followers forward MCP traffic to `MEMORYD_CLUSTER_LEADER_URL`.
- static peer lists and Kubernetes service hints are exposed in status.

Not implemented yet:

- automatic leader election
- follower local replica catch-up
- event-log streaming replication
- consensus/failover

The goal is to keep app integration simple while letting `memoryd` handle the
bridge between pods when an app runs in Kubernetes or another clustered
environment.

## Product Goal

Apps should use one public API:

```ts
import { MemoryD } from "@sayanmohsin/memoryd";

const db = await MemoryD.open();
```

Deployment decides what `open()` connects to:

- local development can use embedded SQLite
- a single server can use a local `memoryd` process
- Kubernetes can use a local sidecar
- cluster mode can forward writes to the current leader

App code should not need to know which pod is leader.

## Runtime Modes

### Embedded Mode

No separate process.

```txt
Node app
  -> @sayanmohsin/memoryd
  -> native Rust binding
  -> local SQLite file
```

This is the simplest mode for local development, single-process apps, CLIs,
desktop tools, and small services.

### Server Mode

One standalone `memoryd` process owns the database file.

```txt
Node app
  -> memoryd server
  -> local SQLite file
```

This is useful when multiple processes on one machine should share the same
store without each process opening SQLite directly.

### Sidecar Mode

One `memoryd` sidecar runs beside the app container in the same pod.

```txt
Pod
  app container
    -> http://127.0.0.1:8757
  memoryd sidecar
    -> /data/memoryd.db
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
  -> Pod B memoryd sidecar
  -> forwards MCP request to Pod A leader
  -> leader writes SQLite
  -> leader appends event
```

This is the major difference from plain SQLite: SQLite stores local state, while
`memoryd` starts to own discovery metadata, write routing, queue leasing, and
agent-safe APIs. Replicated local reads are intentionally not claimed yet.

## Node API Shape

Keep the public API centered on `MemoryD`.

```ts
const db = await MemoryD.open();
```

Resolution order:

1. `MEMORYD_URL` set: connect to server or sidecar.
2. explicit `path` plus `driver: "native"`: open embedded SQLite through the
   native binding.
3. no env: use the in-memory proof store for development.

Explicit targets:

```ts
await MemoryD.open("memoryd://127.0.0.1:8757");
await MemoryD.open("http://127.0.0.1:8757");
```

Explicit options:

```ts
await MemoryD.open({
  path: "./memoryd.db",
  driver: "native",
});

await MemoryD.open({
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
MEMORYD_URL=http://127.0.0.1:8757
```

Embedded mode:

```bash
MEMORYD_DRIVER=native
MEMORYD_PATH=./memoryd.db
```

Server or sidecar mode:

```bash
MEMORYD_HOST=0.0.0.0
MEMORYD_PORT=8757
MEMORYD_PATH=/data/memoryd.db
MEMORYD_CLUSTER_MODE=single
```

Current cluster/bridge mode:

```bash
MEMORYD_HOST=0.0.0.0
MEMORYD_PORT=8757
MEMORYD_ADVERTISE_URL=http://$(POD_IP):8757
MEMORYD_CLUSTER_DISCOVERY=kubernetes
MEMORYD_CLUSTER_SERVICE=memoryd
MEMORYD_CLUSTER_NAMESPACE=default
MEMORYD_CLUSTER_MODE=leader
```

Static peer fallback:

```bash
MEMORYD_CLUSTER_DISCOVERY=static
MEMORYD_CLUSTER_PEERS=http://memoryd-0:8757,http://memoryd-1:8757
```

Follower forwarding:

```bash
MEMORYD_CLUSTER_MODE=follower
MEMORYD_CLUSTER_LEADER_URL=http://memoryd-leader:8757
MEMORYD_CLUSTER_FORWARD_AUTH_TOKEN=change-me
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
      - name: MEMORYD_URL
        value: http://127.0.0.1:8757

  - name: memoryd
    image: ghcr.io/sayanmohsin/memoryd
    env:
      - name: MEMORYD_PATH
        value: /data/memoryd.db
      - name: MEMORYD_HOST
        value: 0.0.0.0
      - name: MEMORYD_CLUSTER_MODE
        value: single
    ports:
      - containerPort: 8757
    volumeMounts:
      - name: memoryd-data
        mountPath: /data
```

Service for peer discovery metadata:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: memoryd
spec:
  clusterIP: None
  selector:
    app: your-app
  ports:
    - name: memoryd
      port: 8757
```

## Bridge Helpers

The sidecar should hide cluster details behind helpers. Current helpers:

- peer discovery
- write forwarding
- health/readiness status
- MCP write attribution

Future helpers:

- leader election
- idempotent forwarded writes
- event-log streaming
- follower catch-up
- local versus strong reads
- queue claim forwarding with failover semantics

The app should not call these helpers directly. They are server-side internals
used by the sidecar and cluster runtime.

## Consistency Rules

Writes:

- always handled by the leader
- followers forward writes to the leader
- queue `claim`, `ack`, and `nack` are writes
- forwarded writes must be idempotent

Reads today:

- follower MCP traffic is forwarded to the leader
- local follower reads are not exposed as a consistency mode yet

Future reads:

- `strong`: route to leader
- `local`: read local follower replica after catch-up exists

Future events:

- leader assigns monotonic event sequence
- followers replicate events from the leader
- indexes and local objects are derived from events where possible

## Non-goals For First Cluster Version

- no multi-primary writes
- no automatic conflict merge between independently written SQLite files
- no consensus protocol unless demand proves it is needed
- no strict read-after-write guarantee from local followers unless the caller
  asks for `strong`

## Phase Plan

### Sidecar Phase A - Server Binary

- add `crates/memoryd-server`
- expose HTTP+JSON API over `memoryd-core`
- support `MEMORYD_PATH`, `MEMORYD_HOST`, `MEMORYD_PORT`, and health checks
- keep cluster disabled

### Sidecar Phase B - Docker And Sidecar Mode

- [x] build Docker runtime image scaffold
- [x] add Kubernetes sidecar example
- [x] app connects through `MEMORYD_URL=http://127.0.0.1:8757`
- [x] document readiness/liveness checks

### Sidecar Phase C - Cluster Bridge

- [x] add peer metadata
- [x] add static peer mode first
- [ ] add Kubernetes service discovery metadata
- [x] add follower MCP forwarding to configured leader

### Sidecar Phase D - Replication

- add leader election
- add event replication from leader to followers
- add local/strong read consistency option

### Sidecar Phase E - Cluster Hardening

- follower catch-up tests
- leader failover tests
- idempotency tests for forwarded writes
- queue claim tests across sidecars
- observability metrics
- optional Helm chart
