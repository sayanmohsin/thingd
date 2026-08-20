# Data Model

thingd stores four core entity types: objects, events, queue jobs, and links. All timestamps are ISO 8601 strings. IDs are strings (UUIDs or custom).

## Optional schema documents

Applications may add a version-controlled `schema.thingd` file describing
collections, fields, annotations, and graph links. This document is a tooling
contract, not a requirement for storing objects: runtime objects remain JSON
and existing schemaless applications continue to work. The native Rust parser
normalizes a valid document to canonical JSON and computes a stable schema hash.
See the [schema files guide](/schema) and `thingd schema check`.

Persistent engines also keep the last applied canonical schema and durable
migration records in dedicated `schemas` and `migrations` keyspaces. These
records describe migration history; they do not turn runtime JSON objects into
rigid tables.

## Persistence and encryption

The logical data model is identical for memory, unencrypted native, and
encrypted native stores. Encryption does not add fields to objects, events,
queues, links, vectors, indexes, or search results. It protects persistent
values and metadata at rest; data is decoded in process memory while the engine
is running.

Encrypted databases require their configured key on every reopen. Filesystem
backups remain encrypted and require the same key. JSON snapshots and logical
exports are decrypted plaintext artifacts and must be protected separately.

Encrypted search does not persist a Tantivy directory. Search content is
rebuilt from decrypted records at startup and held in process memory, which
increases startup time and memory usage without changing search semantics.

## StoredMemoryObject

An object stored in a collection. Objects have arbitrary JSON bodies with required `id` field.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Unique identifier within the collection |
| `collection` | string | auto | Collection name (set by the store) |
| `body` | object | yes | Arbitrary JSON payload. Must include `id` field. |
| `version` | number | auto | Monotonic version counter, starts at 1 |
| `createdAt` | string (ISO 8601) | auto | Creation timestamp |
| `updatedAt` | string (ISO 8601) | auto | Last update timestamp |
| `vector` | number[] \| null | no | Optional vector embedding for similarity search (array of floats) |

**Example:**
```json
{
  "id": "user-001",
  "name": "Alice Chen",
  "email": "alice@example.com",
  "role": "admin",
  "collection": "users",
  "version": 1,
  "createdAt": "2026-05-30T04:09:36.811Z",
  "updatedAt": "2026-05-30T04:09:36.811Z"
}
```

**Constraints:**
- `id` must be a non-empty string
- Object body is merged with `id` — you can pass `{ id: "abc", text: "hello" }` or `{ id: "abc" }` with body fields as top-level keys
- Versions support optimistic locking — set `expectedVersion` to the expected current version; returns error on mismatch

## StoredMemoryEvent

An append-only event in a named stream. Events are ordered by auto-incremented sequence number.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | auto | Event ID (string representation of sequence number) |
| `stream` | string | yes | Stream name (e.g. `"users:alice"`, `"audit"`) |
| `type` | string | yes | Event type (e.g. `"user.created"`, `"decision.made"`) |
| `sequence` | number | auto | Monotonic sequence number within the stream |
| `body` | object | yes | Arbitrary JSON payload |
| `createdAt` | string (ISO 8601) | auto | Creation timestamp |

**Example:**
```json
{
  "id": "26",
  "type": "user.login",
  "text": "User logged in from 192.168.1.1",
  "stream": "users:alice",
  "sequence": 26,
  "createdAt": "2026-06-21T04:54:50.475Z"
}
```

**Constraints:**
- Events are append-only — no updates; protected streams (e.g. `__thingd:mcp:audit`) reject deletion
- Sequence numbers are auto-incremented per stream
- `type` must be a non-empty string
- Streams are created implicitly on first append

## QueueJob

A durable unit of work in a named queue. Jobs flow through a lifecycle: ready → leased → completed/dead.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | auto | UUID job identifier |
| `queue` | string | yes | Queue name |
| `payload` | object | yes | Arbitrary JSON payload |
| `status` | enum | auto | `"ready"` \| `"leased"` \| `"completed"` \| `"dead"` |
| `attempts` | number | auto | Number of attempts made (starts at 0) |
| `maxAttempts` | number | auto | Maximum attempts before dead-letter (default: 3) |
| `createdAt` | string (ISO 8601) | auto | Creation timestamp |
| `availableAt` | string (ISO 8601) | auto | When the job becomes claimable |
| `leasedAt` | string (ISO 8601) | auto | When the job was last claimed |
| `leaseExpiresAt` | string (ISO 8601) | auto | When the current lease expires |
| `completedAt` | string (ISO 8601) | auto | When the job was completed |
| `deadAt` | string (ISO 8601) | auto | When the job moved to dead-letter |
| `lastError` | string | auto | Error message from last nack |

**Example:**
```json
{
  "id": "85a08f45-5deb-4021-a8da-72298cb999b7",
  "queue": "email-queue",
  "payload": { "to": "alice@example.com", "subject": "Welcome" },
  "status": "ready",
  "attempts": 0,
  "maxAttempts": 3,
  "createdAt": "2026-06-21T04:54:57.783Z",
  "availableAt": "2026-06-21T04:54:57.783Z"
}
```

**Queue Lifecycle:**
```
  push()
    │
    ▼
  ┌───────┐   claim()   ┌────────┐   ack()    ┌───────────┐
  │ ready │ ──────────► │ leased │ ─────────► │ completed │
  └───────┘             └────┬───┘            └───────────┘
                             │
                             │ nack() with attempts left
                             │
                             ▼
                          ┌───────┐
                          │ ready │  (after delay)
                          └───────┘

  nack() with attempts exhausted
    │
    ▼
  ┌───────┐
  │  dead │  (dead-letter)
  └───────┘
```

**Semantics:**
- At-least-once delivery — a job may be claimed more than once if the lease expires
- Default lease duration: 30 seconds
- Idempotency keys prevent duplicate pushes within a time window
- Delayed jobs become available after `delayMs` milliseconds

## QueueJobResult

Discriminated union returned by `ack()` and `nack()`:

**Success:**
```json
{
  "ok": true,
  "job": { "id": "...", "queue": "...", "status": "completed", "..." }
}
```

**Failure:**
```json
{
  "ok": false,
  "reason": "not_found | not_leased | terminal"
}
```

| Reason | Meaning |
|--------|---------|
| `not_found` | Job ID does not exist in the queue |
| `not_leased` | Job exists but is not currently leased (can't ack/nack) |
| `terminal` | Job is already completed or dead |

## Link

A directed graph link connecting two references. Links model relationships between objects, events, or any named entity.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | auto | UUID link identifier |
| `fromRef` | string | yes | Source reference (e.g. `"users/alice"`) |
| `linkType` | string | yes | Relationship type (e.g. `"authored"`, `"depends_on"`) |
| `toRef` | string | yes | Target reference (e.g. `"memories/post-1"`) |
| `weight` | number | no | Ranking weight (0.0 to 1.0) |
| `metadataJson` | string | auto | Metadata as JSON string (default: `"{}"`) |
| `createdAt` | string (ISO 8601) | auto | Creation timestamp |

**Example:**
```json
{
  "id": "8d08a9c5-7ffa-44a9-8180-cf8dd179e61e",
  "fromRef": "users/alice",
  "linkType": "authored",
  "toRef": "memories/post-1",
  "weight": 1.0,
  "metadataJson": "{}",
  "createdAt": "2026-06-21T04:55:01.014Z"
}
```

**Constraints:**
- `fromRef` and `toRef` are arbitrary strings — they don't have to match existing objects
- `linkType` is a string — use domain-specific names like `"authored"`, `"supports"`, `"depends_on"`, `"chunk_of"`
- Multiple links between the same pair of references with different `linkType` values are allowed

## ConnectorSchema

Discovered schema for an external database table or file source.

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Table or source name |
| `columns` | array | List of discovered columns |
| `estimatedRows` | number \| null | Estimated row count (null if unknown) |

### Column

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Column name |
| `dataType` | string | `"text"` \| `"integer"` \| `"float"` \| `"boolean"` \| `"timestamp"` \| `"json"` \| `"unknown"` |
| `nullable` | boolean | Whether the column can be null |
| `sampleValues` | array | Sample values for type inference (may be empty) |

### ConnectorAuth

Authentication credentials for external database connectors.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `host` | string | yes | Database hostname |
| `port` | number | yes | Database port (e.g. 5432 for Postgres) |
| `database` | string | yes | Database name |
| `username` | string | yes | Database user |
| `password` | string | yes | Database password |
| `sslMode` | string | no | `"disable"` \| `"prefer"` (default) \| `"require"` |

### ConnectorType

Connector types are advertised dynamically by `GET /v1/connectors`; current
built-ins include `file`, `excel`, `google-sheets`, `postgres`, and `mysql`.
Clients must use the returned descriptors rather than hardcoding this list.

### ColumnAnnotation

Annotation metadata for NLQ schema context.

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Column name |
| `description` | string | Human-readable description |
| `primaryKey` | boolean | Whether this is a primary key |
| `foreignKey` | ForeignKey \| null | Foreign key reference |
| `type` | string | `"text"` \| `"integer"` \| `"float"` \| `"boolean"` \| `"timestamp"` \| `"currency"` \| `"enum"` \| `"json"` \| `"unknown"` |
| `enumValues` | string[] \| null | Allowed values for enum columns |
| `included` | boolean | Include in NLQ schema context |

### ForeignKey

| Field | Type | Description |
|-------|------|-------------|
| `table` | string | Referenced table |
| `column` | string | Referenced column |

---

## Aggregate Types

### AggregateFunction

Available aggregation functions.

| Value | Description |
|-------|-------------|
| `"count"` | Count objects (field ignored) |
| `"sum"` | Sum of numeric field values |
| `"avg"` | Average of numeric field values |
| `"min"` | Minimum of field values |
| `"max"` | Maximum of field values |

### AggregateOptions

Options for `POST /v1/aggregate` and `thing_aggregate` MCP tool.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `collection` | string | yes | Collection to aggregate over |
| `function` | AggregateFunction | yes | Aggregation function |
| `field` | string | no | Field to aggregate (required for sum/avg/min/max, ignored for count) |
| `groupBy` | string | no | Group results by this field |
| `filter` | object | no | Key-value pairs to filter objects before aggregation |

### AggregateResult

Result of an aggregation query.

| Field | Type | Description |
|-------|------|-------------|
| `total` | number | Total across all groups (or single result if no groupBy) |
| `groups` | AggregateGroupResult[] | Per-group results (empty if no groupBy) |

### AggregateGroupResult

| Field | Type | Description |
|-------|------|-------------|
| `key` | string | Group key (the field value) |
| `value` | number | Aggregated value for this group |

### TimeBucket

Available time bucket sizes.

| Value | Description |
|-------|-------------|
| `"hour"` | Group by hour |
| `"day"` | Group by day |
| `"week"` | Group by week |
| `"month"` | Group by month |

### TimeSeriesOptions

Options for `POST /v1/aggregate/timeseries` and `thing_timeseries` MCP tool.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `collection` | string | yes | Collection to aggregate over |
| `function` | AggregateFunction | yes | Aggregation function |
| `field` | string | no | Field to aggregate (ignored for count) |
| `bucket` | TimeBucket | yes | Time bucket size |
| `from` | string | no | Start of time range (ISO 8601) |
| `to` | string | no | End of time range (ISO 8601) |
| `filter` | object | no | Key-value pairs to filter objects before aggregation |

### TimeSeriesResult

Result of a time-series aggregation query.

| Field | Type | Description |
|-------|------|-------------|
| `buckets` | TimeSeriesBucket[] | Ordered time buckets |

### TimeSeriesBucket

| Field | Type | Description |
|-------|------|-------------|
| `label` | string | Bucket label (ISO 8601 truncated to bucket granularity) |
| `value` | number | Aggregated value for this bucket |

---

## Schema Types

### FieldSchema

Inferred field metadata for a collection, used by NLQ schema reflection.

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Field name |
| `type` | string | `"string"` \| `"number"` \| `"boolean"` \| `"date"` \| `"null"` \| `"unknown"` |
| `nullable` | boolean | Whether the field is absent or null in sampled objects |
| `sampleValues` | array | Example values from sampled objects (may be empty) |

### CollectionSchema

Reflected schema for a collection.

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Collection name |
| `objectCount` | number | Total number of objects in the collection |
| `fields` | FieldSchema[] | Inferred fields from sampled objects |

---

## Supporting Types

### SortBy

Sort specification for list queries.

| Field | Type | Values | Default |
|-------|------|--------|---------|
| `field` | string | `"id"` \| `"collection"` \| `"created_at"` \| `"updated_at"` \| `"version"` | — |
| `direction` | string | `"asc"` \| `"desc"` | `"asc"` |

### ListObjectsOptions

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `limit` | number | — | Maximum objects to return |
| `offset` | number | `0` | Number of objects to skip |
| `filter` | object | — | Key-value pairs to match against object bodies |
| `sortBy` | SortBy | — | Sort specification |

### ListEventsOptions

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `fromSequence` | number | — | Only return events with sequence > this value |
| `limit` | number | — | Maximum events to return |

### QueueJobOptions

Options for pushing a job.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `idempotencyKey` | string | — | Prevents duplicate pushes within a time window |
| `maxAttempts` | number | `3` | Maximum attempts before dead-letter |
| `delayMs` | number | `0` | Delay before the job becomes claimable |

### QueueClaimOptions

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `leaseMs` | number | `30000` | Lease duration in milliseconds |

### QueueNackOptions

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `delayMs` | number | `0` | Delay before retry can be claimed |
| `error` | string | — | Error message stored on the job |

### LinkDirection

Direction for neighbor queries.

| Value | Meaning |
|-------|---------|
| `"Outgoing"` | Only links where `fromRef` matches |
| `"Incoming"` | Only links where `toRef` matches |
| `"Both"` | Both directions (default) |

### LinkQueryOptions

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `linkType` | string | — | Filter by relationship type |
| `limit` | number | — | Maximum results to return |

### MemorySearchOptions

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `collections` | string[] | — | Limit search to these collection/stream names |
| `limit` | number | — | Maximum results to return |
| `filter` | object | — | Metadata key-value pairs to match |

### MemorySearchResult

Search results are a discriminated union — each result is either an object or an event.

**Object result:**
```json
{
  "kind": "object",
  "id": "user-001",
  "collection": "users",
  "score": 0.21,
  "value": { "id": "user-001", "name": "Alice", "..." }
}
```

**Event result:**
```json
{
  "kind": "event",
  "id": "26",
  "stream": "users:alice",
  "score": 0.15,
  "value": { "id": "26", "type": "user.login", "..." }
}
```

### VectorSearchOptions

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `topK` | number | — | Maximum number of results to return |
| `filter` | object | — | Metadata key-value pairs to match on the object body |

### VectorSearchHit

```json
{
  "id": "doc-001",
  "score": 0.95,
  "value": { /* StoredMemoryObject */ }
}
```

Vector search returns objects ranked by cosine similarity score (0.0 to 1.0). The `value` field is the full `StoredMemoryObject` including its body and metadata.
