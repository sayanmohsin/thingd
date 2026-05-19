# Sidecar And Cluster Plan

This document plans the future `memoryd` sidecar/server mode. It is not
implemented yet.

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

Every pod has a sidecar. Sidecars discover each other and pick one leader.

```txt
Pod A sidecar = leader
Pod B sidecar = follower
Pod C sidecar = follower
```

Writes from any pod go to the local sidecar first. If that sidecar is not the
leader, it forwards the write to the leader.

```txt
Pod B app
  -> Pod B memoryd sidecar
  -> forwards write to Pod A leader
  -> leader writes SQLite
  -> leader appends event
  -> followers replicate event
  -> followers update local SQLite replicas
```

This is the major difference from plain SQLite: SQLite stores local state, while
`memoryd` owns discovery, write routing, event replication, queue leasing, and
agent-safe APIs.

## Node API Shape

Keep the public API centered on `MemoryD`.

```ts
const db = await MemoryD.open();
```

Resolution order:

1. `MEMORYD_URL` set: connect to server or sidecar.
2. `MEMORYD_DB` set: open embedded SQLite through native binding.
3. no env: use default local path for development once persistence is enabled.

Explicit targets:

```ts
await MemoryD.open("./memoryd.db");
await MemoryD.open("memoryd://127.0.0.1:8757");
await MemoryD.open("http://127.0.0.1:8757");
```

Explicit options:

```ts
await MemoryD.open("./memoryd.db", {
  mode: "embedded",
  driver: "sqlite",
});

await MemoryD.open("memoryd://127.0.0.1:8757", {
  mode: "remote",
  readConsistency: "strong",
});
```

Planned option shape:

```ts
type MemoryDOpenOptions = {
  mode?: "auto" | "embedded" | "remote";
  driver?: "sqlite" | "memory";
  store?: MemoryStore;
  readConsistency?: "strong" | "local";
};
```

`store` remains useful for tests and custom adapters.

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

Planned cluster routes:

```txt
GET    /v1/health
GET    /v1/cluster/status
GET    /v1/cluster/peers
POST   /v1/cluster/forward
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
MEMORYD_MODE=embedded
MEMORYD_DB=./memoryd.db
```

Server or sidecar mode:

```bash
MEMORYD_MODE=server
MEMORYD_BIND=0.0.0.0:8757
MEMORYD_DB=/data/memoryd.db
```

Cluster mode:

```bash
MEMORYD_MODE=cluster
MEMORYD_BIND=0.0.0.0:8757
MEMORYD_ADVERTISE_URL=http://$(POD_IP):8757
MEMORYD_CLUSTER_DISCOVERY=kubernetes
MEMORYD_CLUSTER_SERVICE=memoryd
MEMORYD_CLUSTER_NAMESPACE=default
MEMORYD_CLUSTER_ROLE=auto
MEMORYD_READ_CONSISTENCY=local
MEMORYD_WRITE_POLICY=leader
```

Static peer fallback:

```bash
MEMORYD_CLUSTER_DISCOVERY=static
MEMORYD_CLUSTER_PEERS=http://memoryd-0:8757,http://memoryd-1:8757
```

## Kubernetes Shape

App pod with sidecar:

```yaml
containers:
  - name: app
    image: your-node-app
    env:
      - name: MEMORYD_URL
        value: http://127.0.0.1:8757

  - name: memoryd
    image: ghcr.io/sayanmohsin/memoryd
    args:
      - server
    env:
      - name: MEMORYD_MODE
        value: cluster
      - name: MEMORYD_DB
        value: /data/memoryd.db
      - name: MEMORYD_BIND
        value: 0.0.0.0:8757
      - name: MEMORYD_CLUSTER_DISCOVERY
        value: kubernetes
      - name: MEMORYD_CLUSTER_SERVICE
        value: memoryd
    ports:
      - containerPort: 8757
    volumeMounts:
      - name: memoryd-data
        mountPath: /data
```

Headless service for peer discovery:

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

The sidecar should hide cluster details behind helpers:

- peer discovery
- leader election
- write forwarding
- idempotent forwarded writes
- event-log streaming
- follower catch-up
- local versus strong reads
- health/readiness status
- queue claim forwarding
- MCP write attribution

The app should not call these helpers directly. They are server-side internals
used by the sidecar and cluster runtime.

## Consistency Rules

Writes:

- always handled by the leader
- followers forward writes to the leader
- queue `claim`, `ack`, and `nack` are writes
- forwarded writes must be idempotent

Reads:

- `strong`: route to leader
- `local`: read local follower replica

Events:

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

### Phase 7 - Server Binary

- add `crates/memoryd-server`
- expose HTTP+JSON API over `memoryd-core`
- support `MEMORYD_DB`, `MEMORYD_BIND`, and health checks
- keep cluster disabled

### Phase 8 - Docker And Sidecar Mode

- build `memoryd` Rust binary Docker image
- add Kubernetes sidecar example
- app connects through `MEMORYD_URL=http://127.0.0.1:8757`
- document readiness/liveness checks

### Phase 9 - Cluster Bridge

- add peer discovery
- add static peer mode first
- add Kubernetes headless-service discovery
- add leader election
- add write forwarding
- add event replication from leader to followers
- add local/strong read consistency option

### Phase 10 - Cluster Hardening

- follower catch-up tests
- leader failover tests
- idempotency tests for forwarded writes
- queue claim tests across sidecars
- observability metrics
- optional Helm chart
