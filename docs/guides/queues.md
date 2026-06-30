# Queue Deep Dive — Durable Background Jobs

thingd queues provide at-least-once background job processing with leases,
retries, delays, idempotency keys, and dead-letter handling.

---

## Why queues

You need queues when:

- Processing a user action triggers slow side effects (embedding, indexing, email)
- A background job may fail and needs retry with backoff
- Multiple workers coordinate on the same workload
- You want to decouple request handling from async processing

thingd queues are built into the engine — no Redis, no SQS, no sidekiq.

---

## Push a job

Every queue is a named collection of jobs. Push a job with a payload:

```bash
thingd queue push embed '{ "docId": "doc-1" }'
```

Or with the Node.js SDK:

```ts
const db = await ThingD.open({ driver: "native" });

await db.queue("embed").push(
  { docId: "doc-1", source: "upload" },
  { idempotencyKey: "embed:doc-1:v1", maxAttempts: 3 }
);
```

### Idempotency key

The `idempotencyKey` ensures the same job isn't pushed twice. If a push
succeeds but the client times out before receiving the response, retrying
with the same key returns the existing job instead of creating a duplicate.

### Delayed jobs

Set `availableAfter` to schedule a job for later:

```ts
await db.queue("email").push(
  { to: "user@example.com", template: "welcome" },
  { availableAfter: new Date(Date.now() + 3600_000) }
);
```

---

## Claim and process

Workers claim a batch of jobs. Each claimed job gets a lease — other workers
won't see it until the lease expires.

```bash
thingd queue claim embed
```

```ts
const jobs = await db.queue("embed").claim({ maxJobs: 5 });

for (const job of jobs) {
  try {
    await processJob(job.payload);
    // Mark complete
    await db.queue("embed").ack(job.id);
  } catch {
    // Mark failed (will retry)
    await db.queue("embed").nack(job.id);
  }
}
```

### Leases

When a job is claimed, it gets a lease duration (default 30 seconds). The
worker must ack or nack before the lease expires. If the worker crashes, the
lease expires and the job becomes available for other workers.

You can extend the lease for long-running jobs:

```bash
thingd queue ack embed <job-id> --extend-lease 60
```

---

## Ack and nack

| Action | Behavior |
|---|---|
| `ack` | Mark job as completed. Removed from the active queue. |
| `nack` | Mark job as failed. Increments attempt count. If attempts < max, requeues with optional delay. If attempts >= max, moves to dead-letter queue. |

```ts
// Job succeeded
await db.queue("embed").ack(job.id);

// Job failed — retry after 10 seconds
await db.queue("embed").nack(job.id, { delayMs: 10_000 });
```

### Retry behavior

| Attempt | Default behavior |
|---|---|
| 1 | Job processed normally |
| 2 | Failed — nack with no delay → immediate retry |
| 3 | Failed — reaches maxAttempts → dead-letter |
| ... | If maxAttempts > 3, continues until exhausted |

Set `maxAttempts` per job (default 3). Once exhausted, the job moves to the
dead-letter queue and requires manual intervention.

---

## Dead-letter queue

Jobs that exceed `maxAttempts` land in the dead-letter queue. Inspect them:

```bash
thingd queue dead embed
```

List dead jobs:

```bash
thingd queue dead-list embed
```

A dead-letter queue is a queue of last resort — you must review and decide
what to do with each job (re-queue, discard, or investigate).

---

## Monitor queues

List all queues:

```bash
thingd queues
```

```txt
Queue        Active   Dead   Last push
──────────── ─────── ────── ────────────────
embed        3        1      2 minutes ago
email        0        0      1 hour ago
index        12       0      30 seconds ago
```

Or via SDK:

```ts
const queues = await db.listQueues();
console.log(queues);

const activeCount = await db.countActiveJobs("embed");
const deadCount = await db.countDeadJobs("embed");
```

The [dashboard](./quickstart.md#3-try-the-cli) (`thingd dashboard`) also shows
queue state in real time.

---

## CLI reference

| Command | Description |
|---|---|
| `thingd queue push <name> <payload>` | Push a job |
| `thingd queue claim <name>` | Claim available jobs |
| `thingd queue ack <name> <id>` | Complete a job |
| `thingd queue nack <name> <id>` | Fail and retry a job |
| `thingd queue list <name>` | List active jobs |
| `thingd queue dead <name>` | List dead-letter jobs |
| `thingd queues` | List all queues with counts |

---

## SDK reference

See [api-spec/rest-api.md#queue](../api-spec/rest-api.md#queue) for the full
REST endpoint reference and [api-spec/mcp-tools.md](../api-spec/mcp-tools.md)
for MCP tool schemas.

## Next steps

- [Architecture: queues](../architecture.md) — how queuing works in the engine
- [Agent patterns: scheduler](./agent-patterns.md) — recurring jobs without cron
- [Operations: queue recovery](../operations.md) — maintenance and recovery
