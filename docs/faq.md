# FAQ — Hard questions about thingd

Honest answers for experienced developers evaluating thingd.

## Consistency, correctness, and data model

### What consistency model does thingd guarantee?

**Single-node (embedded / sidecar):** Strong consistency. SQLite serializes writes; reads observe the latest committed write within the same connection.

**Cluster (leader/follower):** Two modes:
- **Strong** — route reads to the leader. Guarantees read-after-write.
- **Local** — read from the follower's local SQLite replica. May serve stale data (eventual consistency). Replication is async (500ms poll interval).

thingd does **not** support tunable consistency per-collection, quorum reads, or multi-primary writes.

### How does object versioning work?

Every object has a monotonic, store-assigned `version` field that increments on each put. The version is metadata — it is not used for optimistic concurrency control.

thingd does **not** support compare-and-swap (CAS) or version-based conflict detection. Two concurrent writers to the same object in cluster mode will race; the last write wins.

### Can two writers modify the same object in parallel in cluster mode?

Yes, and the last write wins. There is no distributed lock per object. For single-node, SQLite serializes writes.

### What does "atomic write" mean in thingd?

Each SDK operation (`put`, `delete`, `appendEvent`, `queue.push`, `queue.ack`, `queue.nack`) is atomic within a single SQLite transaction. Object updates also atomically update the FTS5 search index.

Cross-collection atomicity (e.g., put an object and append an event in one atomic operation) is not supported — they are separate SDK calls.

### Are events and objects transactional together?

No. An object write and an event append are separate operations. If you need both, you must handle partial failure in application code.

## Storage engine and durability

### What happens on crash during a queue ack or object write?

SQLite ensures atomicity: an incomplete transaction is rolled back on next open. WAL mode provides crash recovery. The queue job stays in its pre-operation status (leased or ready).

thingd sets `synchronous = NORMAL` and uses WAL journal mode. These are standard SQLite durability settings suitable for most local and server workloads.

### How durable are queues under failure?

At-least-once delivery. A worker that claims a job but crashes before acking will see the job re-appear as `ready` after its lease expires (defaults are configurable). Jobs that exhaust `maxAttempts` are moved to the dead-letter queue.

### Are writes fsync'd per operation or batched?

SQLite controls fsync. With `synchronous = NORMAL`, SQLite fsyncs at critical checkpoints in WAL mode. thingd does not batch multiple operations into a single fsync — each SDK call commits its own transaction.

### How do you prevent data loss in in-memory mode?

You don't — in-memory mode is ephemeral by design. It exits cleanly for testing and prototyping. For persistence, use the native SQLite driver or the sidecar mode.

### What's the recovery story after corruption?

SQLite corruption is rare but possible (hardware faults, improper shutdown). Recovery relies on standard SQLite tooling: `PRAGMA integrity_check`, `.recover`, or restoring from a CLI snapshot backup. thingd does not currently provide automatic corruption detection or repair.

### How large can datasets grow before performance degrades?

thingd is designed for small-to-medium datasets (hundreds of MB to low GBs). SQLite itself can handle terabytes, but thingd's queue polling and FTS5 search performance should be validated against your workload. Benchmarks publish per-operation latency at small scale; we do not yet have published degradation curves for large datasets.

## Performance

### What are read/write latencies?

Published benchmarks (local development hardware, 5000 iterations):

| Operation | In-memory | SQLite (file) |
|-----------|-----------|---------------|
| Object put | 868k ops/sec | 2.2k ops/sec |
| Object get | 1.9M ops/sec | 237k ops/sec |
| Event append | 2.1M ops/sec | — |
| Queue claim+ack | — | 14.7k ops/sec |

Node.js (native driver, 1000 iterations):

| Operation | In-memory | Native (SQLite) |
|-----------|-----------|-----------------|
| Object put | 435k ops/sec | 9.8k ops/sec |
| Object get | 2.9M ops/sec | 192k ops/sec |
| Event append | — | 37k ops/sec |

See [benchmarks.md](./benchmarks.md) for full methodology.

### How does search scale with millions of records?

FTS5 with BM25 scoring performs well into millions of rows on modern hardware. Metadata filtering adds a post-query pass — performance depends on the selectivity of the filter. We recommend testing with your specific data shape and query patterns.

### Are queues O(1) or do they degrade with backlog size?

Queue claim scans by `(queue, status, created_at)` index — performance is O(log n) with backlog growth, not O(1). Very large backlogs (millions of `ready` jobs) will slow claim times.

### Is thingd single-threaded or multi-threaded?

The Rust SQLite adapter is single-threaded per connection. WAL mode permits concurrent reads from separate connections. The HTTP MCP server can handle concurrent requests, but SQLite write serialization applies.

## Concurrency and scaling

### How does leader/follower replication work?

The leader assigns a monotonic sequence to each event. Followers poll the leader every 500ms for new events and apply them to their local SQLite replica. Object state and search indexes on followers are derived from the replicated event stream.

### Is replication synchronous or async?

Async. Followers poll on a timer. There is no synchronous replication, no write-ahead log shipping, and no quorum acknowledgment.

### What happens during leader failover?

With `THINGD_CLUSTER_LEADER_ELECTION=true` and `THINGD_CLUSTER_PEERS` configured,
followers auto-promote the next peer in the peer list when the current leader is
unreachable for `THINGD_CLUSTER_LEADER_ELECTION_MAX_FAILURES` consecutive
replication cycles (default: 3, each cycle is 500ms). The promoted peer begins
serving MCP writes and replication events directly. Other peers automatically
redirect their `leaderUrl` to the new leader.

Without leader election, nothing automatic. A manual process is required:
promote a follower by reconfiguring env vars and updating DNS/service discovery.

### Can followers serve stale reads?

Yes, in `local` consistency mode. Followers may lag behind the leader by up to 500ms (one poll cycle) plus any replication processing delay. Route reads to the leader for strong consistency.

### How is split-brain prevented?

thingd does not prevent split-brain. There is no consensus protocol, no fencing, and no lease-based leader lock. Preventing split-brain is the operator's responsibility (e.g., using a single shared volume or a Kubernetes StatefulSet with stable identity).

### Can you horizontally scale writes?

No. Writes go through a single leader. Followers forward writes to the leader. Read throughput can scale horizontally with followers.

## Queue semantics

### Is delivery at-least-once or exactly-once?

At-least-once. Consumers should be idempotent.

### How is idempotency enforced?

`queue.push` accepts an optional `idempotencyKey`. If a job with the same `(queue, id)` already exists, the push is a no-op and returns the existing job. For consumers, the recommended pattern is to treat duplicate claims as safe no-ops by checking downstream state before processing.

### What happens if a worker dies mid-processing?

The lease expires (configurable `leaseMs`) and the job becomes available for re-claim by any worker. The job's `attempts` counter is not incremented on lease expiry — it increments only on explicit `nack`. This means a crash can cause at-least-once delivery even for a job that had almost completed.

### Are retries exponential backoff or fixed delay?

thingd provides a per-nack `delayMs` parameter. The caller controls the delay. There is no built-in exponential backoff — the application or worker implements the desired backoff strategy.

### Is FIFO strict within a queue?

Jobs are claimed in `created_at` order within each queue. This approximates FIFO but is not guaranteed under concurrent claimers. There is no strict FIFO guarantee or message ordering.

### How do dead-letter queues get inspected and replayed?

CLI commands: `thingd queues dead <queue>` lists dead jobs. To replay, delete and re-push, or use the TUI's retry action. There is no automatic DLQ replay or scheduled retry from DLQ.

## MCP / AI-agent integration

### What MCP tools are exposed?

12 tools: `thing_search`, `thing_get`, `thing_put`, `thing_delete`, `thing_events_append`, `thing_events_list`, `thing_queue_push`, `thing_queue_claim`, `thing_queue_ack`, `thing_queue_nack`, `thing_queue_list`, `thing_queue_dead`.

### Can agents bypass allowlists accidentally?

No. The MCP layer enforces collection allowlists at the tool level. Write operations to collections not in `THINGD_MCP_COLLECTIONS` are rejected with a tool-level error before reaching the store.

### How are write operations from agents secured?

- Bearer token authentication (required for non-loopback HTTP binding).
- Collection allowlists (`THINGD_MCP_COLLECTIONS`).
- Read-only mode (`THINGD_MCP_READ_ONLY=true`) disables all write tools.
- Payload size limits (`THINGD_MCP_MAX_PAYLOAD_BYTES`).
- Non-loopback binding refused without `THINGD_AUTH_TOKEN`.

### Is there audit logging for every tool call?

Yes. Every write tool call appends an audit event to the `__thingd:mcp:audit` event stream. Tools can include optional `actor` and `source` fields. The audit stream name and behavior are configurable via env vars.

### Can multiple agents coordinate safely on queues?

Yes. Queues use leases for safe multi-worker coordination. Each agent claims a job with a lease, processes it, and acks. If the agent crashes, the lease expires and the job becomes available to another agent.

### What prevents prompt injection from mutating data?

thingd does not currently have MCP-level prompt injection defenses. The application should treat MCP tool access as a privileged API boundary — control which agents have access, use read-only mode when agents only need to search, and validate tool arguments at the application level.

## Schema and data model

### Is there a schema validation layer?

No. Objects are stored as JSON blobs with no server-side validation. Any valid JSON can be stored in any collection.

### Can you enforce types or constraints per collection?

Not currently. Collections are flat namespaces with no schema, no type enforcement, and no constraint system.

### How do you handle migrations?

thingd has internal schema migrations for the storage layer (SQLite table structure). There is no migration system for user data shapes. Application-level schema evolution is the caller's responsibility.

### What happens when object shapes evolve over time?

Older and newer object shapes coexist in the same collection. Search indexing indexes whatever text the object contains. Applications should handle missing fields gracefully. There is no migration tool to reshape existing objects.

## Deployment and operations

### How do backups and restores work?

CLI snapshot commands: `thingd snapshot create --out backup.thingd.json` (exports all objects, events, and queue jobs as JSON lines). Restore with `thingd snapshot restore --in backup.thingd.json`. For file-level backups, copy the SQLite `.db` file while the engine is not writing.

### Can I run this in Kubernetes with persistence?

Yes. Kubernetes deployment manifests are in `deploy/kubernetes/`. Use a PersistentVolumeClaim for the SQLite database file. For leader/follower mode, use a StatefulSet with stable pod identity.

### What happens during rolling upgrades?

thingd does not have built-in zero-downtime upgrade support. Restarting the process reopens the SQLite file. Connection-based recovery is the application's responsibility. Schema migrations are forward-only and non-breaking within the same major schema version.

### Is there a health check / observability API?

- `GET /healthz` — basic health check.
- `GET /cluster/status` — cluster health, role, peer info, replication lag.
- CLI: `thingd doctor`, `thingd metrics`, `thingd status`.

There is no Prometheus/metrics endpoint, no structured logging configuration, and no tracing integration.

## thingd-cloud

### What is thingd-cloud?

thingd-cloud does not exist yet. It is a future concept for a hosted/cloud gateway. The current cloud story: run the sidecar on a VM or Kubernetes and connect via `THINGD_URL`.

### Will APIs differ between local and cloud?

The SDK is designed to be driver-agnostic — the same `ThingD.open()` call works for in-memory, native, and remote drivers. The cloud API is expected to be the same MCP-based interface.

### Is multi-tenancy supported?

Not at the engine level. Each SQLite database is a single tenant. For multi-tenant deployments, run one sidecar per tenant or implement tenant isolation at the application layer.

## Positioning

### Why not just use Postgres + Redis + Elasticsearch?

thingd targets a different point on the complexity curve. If you already operate Postgres, Redis, and Elasticsearch in production, you likely don't need thingd. thingd is for:

- Apps that want one local runtime instead of three infrastructure dependencies.
- AI agents that need MCP-native memory and queues without custom integrations.
- Single-node and small-cluster deployments where operational overhead of multiple services is disproportionate.

See the [comparison table](../README.md#comparison) in the README.

### What's the real novelty?

Integration and design. thingd is not a novel storage engine — it's a novel combination of primitives (objects + events + queues + search + MCP) designed together as a single abstraction rather than bolted together from separate systems. The MCP-native access model is a first-class design constraint, not an afterthought.

### What workloads is thingd not suitable for?

- Multi-primary geo-distributed workloads.
- Strict exactly-once queue delivery.
- Datasets requiring relational joins or complex queries.
- High-throughput write workloads (>10k writes/sec on a single node).
- Use cases requiring per-collection schema enforcement.
- Production clusters requiring automatic failover.

### Is thingd optimized for developer experience or infrastructure reduction?

Both, but honestly: it prioritizes developer experience first. The infrastructure reduction is a consequence of having fewer services to deploy, not a claim of superior throughput or durability vs. specialized systems.

### What's the long-term tradeoff of using thingd?

The tradeoff is: simpler deployment + unified API vs. less operational maturity than specialized systems. If thingd's abstraction fits your data model, you save on integration complexity. If your requirements outgrow thingd's single-writer SQLite foundation, migration to a more scalable system will require architectural changes.
