# REST API Reference

thingd exposes a REST API on port 8757 (default) under the `/v1` prefix. All requests and responses use JSON.

**Base URL:** `http://localhost:8757/v1`

**Authentication:** `Authorization: Bearer <token>` header. Required when `THINGD_AUTH_TOKEN` or `auth.token` is configured on the server.

**CORS:** Configurable via `hardening.cors_allowed_origins`. Default: `http://localhost:8757`.

**Rate Limiting:** Per-IP token bucket via `hardening.rate_limit_enabled` (default: enabled, 300 rpm).

**Error Mode:** In production mode (`server.production_mode: true`), internal error details are sanitized. See [Error Codes](#error-codes).

---

## Response Format

**Success:**
```json
{ "data": <result> }
```

**Error:**
```json
{
  "error": {
    "type": "error_code",
    "title": "Error Title",
    "status": 400,
    "detail": "Human-readable description"
  }
}
```

(Internal error details are sanitized in production mode — `detail` is empty for 500 errors.)

---

## Health

### `GET /v1/health`

Basic health check.

```bash
curl http://localhost:8757/v1/health
```

```json
{
  "data": {
    "status": "ok"
  }
}
```

> **Note:** Use `GET /v1/counts/objects`, `GET /v1/counts/events`, and `GET /v1/counts/links` for aggregate counts.

---

## Counts

### `GET /v1/counts/objects`

```bash
curl http://localhost:8757/v1/counts/objects
```

```json
{ "data": { "count": 12 } }
```

### `GET /v1/counts/events`

```bash
curl http://localhost:8757/v1/counts/events
```

```json
{ "data": { "count": 5 } }
```

### `GET /v1/counts/links`

```bash
curl http://localhost:8757/v1/counts/links
```

```json
{ "data": { "count": 3 } }
```

---

## Metadata

### `GET /v1/collections`

List all collection names that have at least one object.

```bash
curl http://localhost:8757/v1/collections
```

```json
{ "data": ["users", "posts"] }
```

### `GET /v1/streams`

List all stream names that have at least one event.

```bash
curl http://localhost:8757/v1/streams
```

```json
{ "data": ["audit", "users:alice"] }
```

### `GET /v1/queues`

List all queue names that have at least one job.

```bash
curl http://localhost:8757/v1/queues
```

```json
{ "data": ["email-queue", "sync-queue"] }
```

### `GET /v1/indexes`

List all custom functional indexes. Returns `[collection, field]` pairs.

```bash
curl http://localhost:8757/v1/indexes
```

```json
{ "data": [["users", "email"], ["orders", "status"]] }
```

### `POST /v1/indexes`

Create a functional index on a JSON body field for a collection. Idempotent.

```bash
curl -X POST http://localhost:8757/v1/indexes \
  -H "Content-Type: application/json" \
  -d '{"collection": "users", "field": "email"}'
```

```json
{ "created": true }
```

### `GET /v1/collections/schema`

Reflect the schema of all collections that have objects. Returns the inferred
field names, types, and sample values for each collection.

```bash
curl http://localhost:8757/v1/collections/schema
```

```json
{
  "data": [
    {
      "name": "orders",
      "objectCount": 150,
      "fields": [
        { "name": "id", "type": "string", "nullable": false, "sampleValues": ["ord-001", "ord-002"] },
        { "name": "product", "type": "string", "nullable": false, "sampleValues": ["Widget", "Gadget"] },
        { "name": "revenue", "type": "number", "nullable": false, "sampleValues": [29.99, 49.99] },
        { "name": "region", "type": "string", "nullable": true, "sampleValues": ["North", "South"] },
        { "name": "date", "type": "date", "nullable": false, "sampleValues": ["2026-06-01T00:00:00Z"] }
      ]
    }
  ]
}
```

### `GET /v1/collections/{name}/schema`

Reflect the schema of a single collection.

```bash
curl http://localhost:8757/v1/collections/orders/schema
```

Returns the same format as the list endpoint but scoped to one collection.
Returns 404 if the collection has no objects.

---

## Objects

### `GET /v1/objects` — List objects

**Query parameters:**

| Param | Required | Description |
|-------|----------|-------------|
| `collection` | yes | Collection name |
| `limit` | no | Max objects to return |
| `offset` | no | Skip N objects (default: 0) |
| `filter.key` | no | Filter by key=value in object body |
| `sortBy` | no | Sort field: `id`, `created_at`, `updated_at`, `version` |
| `sortDir` | no | `asc` (default) or `desc` |

```bash
curl "http://localhost:8757/v1/objects?collection=users&limit=2&sortBy=created_at&sortDir=desc"
```

```json
{
  "data": [
    {
      "id": "user-002",
      "collection": "users",
      "body": { "name": "Bob" },
      "version": 1,
      "createdAt": "2026-06-21T04:54:50.475Z",
      "updatedAt": "2026-06-21T04:54:50.475Z"
    },
    {
      "id": "user-001",
      "collection": "users",
      "body": { "name": "Alice" },
      "version": 1,
      "createdAt": "2026-06-21T04:54:49.475Z",
      "updatedAt": "2026-06-21T04:54:49.475Z"
    }
  ]
}
```

**Filter example:** `?filter.role=admin&filter.status=active`

### `PUT /v1/objects/:collection/:id` — Upsert object

```bash
curl -X PUT http://localhost:8757/v1/objects/users/user-001 \
  -H "Content-Type: application/json" \
  -d '{"name": "Alice Chen", "email": "alice@example.com", "role": "admin"}'
```

```json
{
  "data": {
    "id": "user-001",
    "collection": "users",
    "version": 2,
    "createdAt": "2026-06-21T04:54:49.475Z",
    "updatedAt": "2026-06-21T04:55:01.014Z"
  }
}
```

**Optimistic locking (CAS):** Set the `If-Match` header to the expected version:

```bash
curl -X PUT http://localhost:8757/v1/objects/users/user-001 \
  -H "Content-Type: application/json" \
  -H "If-Match: 1" \
  -d '{"name": "Alice Chen", "role": "admin"}'
```

If the current version does not match, returns `409 Conflict` with error type `"conflict"`.

### `GET /v1/objects/:collection/:id` — Get single object

```bash
curl http://localhost:8757/v1/objects/users/user-001
```

```json
{
  "data": {
    "id": "user-001",
    "collection": "users",
    "body": { "name": "Alice Chen", "email": "alice@example.com", "role": "admin" },
    "version": 2,
    "createdAt": "2026-06-21T04:54:49.475Z",
    "updatedAt": "2026-06-21T04:55:01.014Z"
  }
}
```

Returns 404 if not found.

### `DELETE /v1/objects/:collection/:id` — Delete object

```bash
curl -X DELETE http://localhost:8757/v1/objects/users/user-001
```

```json
{ "data": { "deleted": true } }
```

Returns `deleted: true` even if the object didn't exist (idempotent).

### `PUT /v1/objects/batch` — Batch upsert

**Query parameter:** `collection` (required)

**Body:** array of objects or `{ "objects": [...] }`

```bash
curl -X PUT "http://localhost:8757/v1/objects/batch?collection=users" \
  -H "Content-Type: application/json" \
  -d '[
    {"id": "user-010", "name": "Zoe"},
    {"id": "user-011", "name": "Wang"}
  ]'
```

```json
{
  "data": [
    { "id": "user-010", "name": "Zoe", "collection": "users", "version": 1, "..." : "..." },
    { "id": "user-011", "name": "Wang", "collection": "users", "version": 1, "..." : "..." }
  ]
}
```

### `DELETE /v1/objects/batch` — Batch delete

**Query parameter:** `collection` (required)

**Body:** array of IDs or `{ "ids": [...] }`

```bash
curl -X DELETE "http://localhost:8757/v1/objects/batch?collection=users" \
  -H "Content-Type: application/json" \
  -d '["user-010", "user-011"]'
```

```json
{ "data": { "deleted": 2 } }
```

### `GET /v1/objects/batch` — Batch read

**Query parameter:** `collection` (required)

**Body:** array of IDs or `{ "ids": [...] }`

```bash
curl -X GET "http://localhost:8757/v1/objects/batch?collection=users" \
  -H "Content-Type: application/json" \
  -d '["user-010", "user-011", "user-999"]'
```

```json
{
  "data": [
    { "id": "user-010", "collection": "users", "body": { "..." : "..." } },
    { "id": "user-011", "collection": "users", "body": { "..." : "..." } },
    null
  ]
}
```

Missing IDs return `null` entries, preserving input order.
```

---

## Search

### `POST /v1/search` — Full-text search

**Body:**

| Field | Required | Description |
|-------|----------|-------------|
| `query` | yes | FTS5 query string |
| `collections` | no | Limit to these collection/stream names |
| `limit` | no | Max results |
| `filter` | no | Metadata key-value pairs to match |

```bash
curl -X POST http://localhost:8757/v1/search \
  -H "Content-Type: application/json" \
  -d '{"query": "alice", "limit": 5}'
```

```json
{
  "data": [
    {
      "kind": "object",
      "id": "user-001",
      "collection": "users",
      "score": 0.21,
      "value": {
        "id": "user-001",
        "name": "Alice Chen",
        "collection": "users",
        "version": 2
      }
    }
  ]
}
```

Results are sorted by relevance score (descending). Each result is either an object or an event.

---

### `POST /v1/search/vector` — Vector similarity search

**Body:**

| Field | Required | Description |
|-------|----------|-------------|
| `collection` | yes | Collection name |
| `vector` | yes | Query vector as array of floats |
| `topK` | no | Max results |
| `filter` | no | Metadata key-value pairs to match on the object body |

```bash
curl -X POST http://localhost:8757/v1/search/vector \
  -H "Content-Type: application/json" \
  -d '{"collection": "docs", "vector": [0.1, 0.2, 0.3]}'
```

```json
{
  "data": [
    {
      "id": "doc-001",
      "score": 0.95,
      "value": { "id": "doc-001", "collection": "docs", "body": {"text":"hello"}, "version": 1, "createdAt": "...", "updatedAt": "..." }
    }
  ]
}
```

Results are sorted by cosine similarity (descending). Collections without vectors return empty results.

---

## Events

### `POST /v1/events/:stream` — Append event

**Body:**

| Field | Required | Description |
|-------|----------|-------------|
| `type` | yes | Event type (e.g. `"user.created"`) |
| `body` | no | Arbitrary JSON payload |
| `text` | no | Free-text content |

```bash
curl -X POST http://localhost:8757/v1/events/audit \
  -H "Content-Type: application/json" \
  -d '{"type": "user.login", "text": "User logged in from 192.168.1.1"}'
```

```json
{
  "data": {
    "id": "26",
    "type": "user.login",
    "text": "User logged in from 192.168.1.1",
    "stream": "audit",
    "sequence": 26,
    "createdAt": "2026-06-21T04:54:50.475Z"
  }
}
```

### `GET /v1/events` — List events

**Query parameters:**

| Param | Description |
|-------|-------------|
| `stream` | Filter by stream name (optional — omit to list all) |
| `fromSequence` | Only return events with sequence > this value |
| `limit` | Max events to return |

```bash
curl "http://localhost:8757/v1/events?stream=audit&limit=10"
```

```json
{
  "data": [
    { "id": "24", "type": "user.login", "stream": "audit", "sequence": 24, "..." : "..." },
    { "id": "25", "type": "user.logout", "stream": "audit", "sequence": 25, "..." : "..." }
  ]
}
```

---

## Queues

### `POST /v1/queues/:queue/push` — Push job

**Body:**

| Field | Required | Description |
|-------|----------|-------------|
| `payload` | yes | Job payload (arbitrary JSON) |
| `idempotencyKey` | no | Prevents duplicate pushes |
| `maxAttempts` | no | Max attempts before dead-letter (default: 3) |
| `delayMs` | no | Delay before job becomes claimable |

```bash
curl -X POST http://localhost:8757/v1/queues/email-queue/push \
  -H "Content-Type: application/json" \
  -d '{"payload": {"to": "alice@example.com", "subject": "Welcome"}}'
```

```json
{
  "data": {
    "id": "85a08f45-5deb-4021-a8da-72298cb999b7",
    "queue": "email-queue",
    "payload": { "to": "alice@example.com", "subject": "Welcome" },
    "status": "ready",
    "attempts": 0,
    "maxAttempts": 3,
    "createdAt": "2026-06-21T04:54:57.783Z",
    "availableAt": "2026-06-21T04:54:57.783Z"
  }
}
```

### `POST /v1/queues/:queue/claim` — Claim job

**Body:**

| Field | Required | Description |
|-------|----------|-------------|
| `leaseMs` | no | Lease duration in ms (default: 30000) |

```bash
curl -X POST http://localhost:8757/v1/queues/email-queue/claim \
  -H "Content-Type: application/json" \
  -d '{}'
```

Returns the claimed job, or `null` if no jobs are available.

### `POST /v1/queues/:queue/ack` — Complete job

**Body:**

| Field | Required | Description |
|-------|----------|-------------|
| `jobId` | yes | ID of the job to ack |

```bash
curl -X POST http://localhost:8757/v1/queues/email-queue/ack \
  -H "Content-Type: application/json" \
  -d '{"jobId": "85a08f45-5deb-4021-a8da-72298cb999b7"}'
```

```json
{ "data": { "id": "...", "queue": "email-queue", "status": "completed", "..." : "..." } }
```

Returns 400 with `not_found`, `not_leased`, or `terminal` error code on failure.

### `POST /v1/queues/:queue/nack` — Fail job

**Body:**

| Field | Required | Description |
|-------|----------|-------------|
| `jobId` | yes | ID of the job to nack |
| `delayMs` | no | Delay before retry |
| `error` | no | Error message |

```bash
curl -X POST http://localhost:8757/v1/queues/email-queue/nack \
  -H "Content-Type: application/json" \
  -d '{"jobId": "85a08f45-...", "error": "SMTP connection failed"}'
```

### `GET /v1/queues/:queue/jobs` — List active jobs

```bash
curl http://localhost:8757/v1/queues/email-queue/jobs
```

```json
{
  "data": [
    { "id": "...", "queue": "email-queue", "status": "ready", "..." : "..." }
  ]
}
```

### `GET /v1/queues/:queue/dead` — List dead-lettered jobs

```bash
curl http://localhost:8757/v1/queues/email-queue/dead
```

```json
{
  "data": [
    { "id": "...", "queue": "email-queue", "status": "dead", "lastError": "SMTP timeout", "..." : "..." }
  ]
}
```

---

## Links

### `POST /v1/links` — Create link

**Body:**

| Field | Required | Description |
|-------|----------|-------------|
| `fromRef` | yes | Source reference (e.g. `"users/alice"`) |
| `linkType` | yes | Relationship type (e.g. `"authored"`) |
| `toRef` | yes | Target reference (e.g. `"memories/post-1"`) |
| `weight` | no | Ranking weight (0.0 to 1.0) |
| `metadataJson` | no | Metadata as JSON string |

```bash
curl -X POST http://localhost:8757/v1/links \
  -H "Content-Type: application/json" \
  -d '{"fromRef": "users/alice", "linkType": "authored", "toRef": "memories/post-1"}'
```

```json
{
  "data": {
    "id": "8d08a9c5-7ffa-44a9-8180-cf8dd179e61e",
    "fromRef": "users/alice",
    "linkType": "authored",
    "toRef": "memories/post-1",
    "weight": 1.0,
    "metadataJson": "{}",
    "createdAt": "2026-06-21T04:55:01.014Z"
  }
}
```

### `GET /v1/links/:id` — Get link by ID

```bash
curl http://localhost:8757/v1/links/8d08a9c5-7ffa-44a9-8180-cf8dd179e61e
```

### `GET /v1/links?reference=...` — Get neighbors

**Query parameters:**

| Param | Required | Description |
|-------|----------|-------------|
| `reference` | yes | Reference to find neighbors for |
| `direction` | no | `Outgoing`, `Incoming`, or `Both` (default) |
| `linkType` | no | Filter by link type |
| `limit` | no | Max results |

```bash
curl "http://localhost:8757/v1/links?reference=users/alice&direction=Outgoing"
```

```json
{
  "data": [
    { "id": "...", "fromRef": "users/alice", "linkType": "authored", "toRef": "memories/post-1", "..." : "..." }
  ]
}
```

### `DELETE /v1/links/:id` — Delete link

```bash
curl -X DELETE http://localhost:8757/v1/links/8d08a9c5-7ffa-44a9-8180-cf8dd179e61e
```

```json
{ "data": true }
```

---

## Aggregation

### `POST /v1/aggregate` — General aggregation

Run a count, sum, avg, min, or max aggregation over objects in a collection, with optional grouping.

**Body:**

| Field | Required | Description |
|-------|----------|-------------|
| `collection` | yes | Collection name |
| `function` | yes | `count`, `sum`, `avg`, `min`, `max` |
| `field` | no | Field to aggregate (required for sum/avg/min/max, ignored for count) |
| `groupBy` | no | Group results by this field |
| `filter` | no | Key-value pairs to filter objects before aggregation |

```bash
curl -X POST http://localhost:8757/v1/aggregate \
  -H "Content-Type: application/json" \
  -d '{"collection": "sales", "function": "sum", "field": "amount", "groupBy": "region"}'
```

```json
{
  "data": {
    "total": 45000,
    "groups": [
      { "key": "North", "value": 15000 },
      { "key": "South", "value": 12000 },
      { "key": "East", "value": 10000 },
      { "key": "West", "value": 8000 }
    ]
  }
}
```

### `POST /v1/aggregate/timeseries` — Time-bucketed aggregation

Run a time-bucketed aggregation over objects, grouped by hour/day/week/month.

**Body:**

| Field | Required | Description |
|-------|----------|-------------|
| `collection` | yes | Collection name |
| `function` | yes | `count`, `sum`, `avg`, `min`, `max` |
| `bucket` | yes | `hour`, `day`, `week`, `month` |
| `field` | no | Field to aggregate (ignored for count) |
| `from` | no | Start of time range (ISO 8601) |
| `to` | no | End of time range (ISO 8601) |
| `filter` | no | Key-value pairs to filter objects before aggregation |

```bash
curl -X POST http://localhost:8757/v1/aggregate/timeseries \
  -H "Content-Type: application/json" \
  -d '{"collection": "sales", "function": "sum", "field": "amount", "bucket": "month", "from": "2026-01-01T00:00:00Z", "to": "2026-07-01T00:00:00Z"}'
```

```json
{
  "data": {
    "buckets": [
      { "label": "2026-01-01T00:00:00Z", "value": 7200 },
      { "label": "2026-02-01T00:00:00Z", "value": 8100 },
      { "label": "2026-03-01T00:00:00Z", "value": 9500 }
    ]
  }
}
```

---

## Connectors

### `GET /v1/connectors` — List available connectors

Returns the list of available connector types.

```bash
curl http://localhost:8757/v1/connectors
```

```json
{ "data": ["file", "postgres", "mysql"] }
```

### `POST /v1/connectors/{type}/ping` — Test connection

Test connectivity to an external database without importing data or discovering schema.

**Path parameter:** `type` — connector type (`"postgres"`, `"mysql"`, `"file"`)

**Body:**

| Field | Required | Description |
|-------|----------|-------------|
| `auth` | for db connectors | Database credentials (host, port, database, username, password, sslMode) |

```bash
curl -X POST http://localhost:8757/v1/connectors/postgres/ping \
  -H "Content-Type: application/json" \
  -d '{
    "auth": {
      "host": "localhost",
      "port": 5432,
      "database": "mydb",
      "username": "user",
      "password": "pass"
    }
  }'
```

```json
{ "data": { "ok": true, "connector": "postgres" } }
```

On failure, returns a `400 Bad Request` with connection error details.

---

### `POST /v1/connectors/{type}/schema` — Discover schema

Discover the schema of an external table or file source without importing data.

**Path parameter:** `type` — connector type (`"postgres"`, `"mysql"`, `"file"`)

**Body:**

| Field | Required | Description |
|-------|----------|-------------|
| `auth` | for db connectors | Database credentials (host, port, database, username, password, sslMode) |
| `source` | for file connector | File path or connection string |
| `query` | yes | Table name for DB connectors, or file path for file connector |

```bash
curl -X POST http://localhost:8757/v1/connectors/postgres/schema \
  -H "Content-Type: application/json" \
  -d '{
    "auth": {
      "host": "localhost",
      "port": 5432,
      "database": "mydb",
      "username": "user",
      "password": "pass"
    },
    "query": "users"
  }'
```

```json
{
  "data": {
    "name": "users",
    "columns": [
      { "name": "id", "dataType": "integer", "nullable": false, "sampleValues": [1, 2, 3] },
      { "name": "name", "dataType": "text", "nullable": true, "sampleValues": ["Alice", "Bob"] },
      { "name": "created_at", "dataType": "timestamp", "nullable": false, "sampleValues": [] }
    ],
    "estimatedRows": null
  }
}
```

### `POST /v1/connectors/{type}/pull` — Import data

Pull data from an external source into a thingd collection. Each row becomes an object in the specified collection.

**Path parameter:** `type` — connector type (`"postgres"`, `"mysql"`, `"file"`)

**Body:**

| Field | Required | Description |
|-------|----------|-------------|
| `auth` | for db connectors | Database credentials |
| `source` | for file connector | File path |
| `collection` | yes | Target thingd collection name |
| `query` | yes | SQL query (for DB) or table name. For files, the file path. |
| `batchSize` | no | Rows per batch (default: 1000) |
| `columnMapping` | no | Map external column names to thingd field names: `{ "old_name": "new_name" }` |
| `syncStrategy` | no | `"full"` (default) or `"incremental"` |

```bash
curl -X POST http://localhost:8757/v1/connectors/postgres/pull \
  -H "Content-Type: application/json" \
  -d '{
    "auth": {
      "host": "localhost",
      "port": 5432,
      "database": "mydb",
      "username": "user",
      "password": "pass"
    },
    "collection": "imported_users",
    "query": "SELECT * FROM users"
  }'
```

```json
{
  "data": {
    "imported": 150,
    "collection": "imported_users"
  }
}
```

---

## Natural Language Query

### `POST /v1/nlq` — Natural language query

Ask a natural language question about your data. Uses an LLM to convert the question into a structured query. Read-only.

**Body:**

| Field | Required | Description |
|-------|----------|-------------|
| `question` | yes | Natural language question (e.g. "total sales by region") |
| `collections` | no | Limit to these collection names |
| `model` | no | Override the server's default LLM model |

```bash
curl -X POST http://localhost:8757/v1/nlq \
  -H "Content-Type: application/json" \
  -d '{"question": "What were total sales by region last month?"}'
```

```json
{
  "data": {
    "answer": "Total sales by region last month: North $15,000, South $12,000, East $10,000, West $8,000",
    "data": [
      { "region": "North", "total": 15000 },
      { "region": "South", "total": 12000 }
    ]
  }
}
```

Requires LLM configuration on the server (`nlq.enabled`, `nlq.model`, `nlq.endpoint`). Returns 400 with `nlq_not_enabled` if not configured.

---

## Error Codes

| Code | HTTP Status | Meaning |
|------|-------------|---------|
| `bad_request` | 400 | Missing required field or invalid input |
| `not_found` | 404 | Resource does not exist |
| `conflict` | 409 | Operation conflicts with current state |
| `too_many_requests` | 429 | Rate limit exceeded (when rate limiting is enabled) |
| `internal_error` | 500 | Unexpected server error |

**Production mode:** When `server.production_mode` is `true`, `internal_error` responses return an empty detail field. Full error details are logged server-side only.

## Server Configuration

### TLS / HTTPS

thingd does not serve HTTPS directly. Deploy behind a reverse proxy (nginx, Caddy) for TLS termination. See [Security](../security.md) for configuration examples.

### CORS

```yaml
hardening:
  cors_allowed_origins:
    - "http://localhost:8757"
  cors_max_age_secs: 86400
```

- Empty list = permissive (`Access-Control-Allow-Origin: *`)
- Specific origins restrict access to listed domains
- Methods: `GET, POST, PUT, DELETE, OPTIONS`
- Headers: `Authorization, Content-Type, MCP-Protocol-Version`

### Rate Limiting

```yaml
hardening:
  rate_limit_enabled: true
  rate_limit_requests_per_minute: 300
```

- Per-IP token bucket (keyed by `X-Forwarded-For` or connection address)
- Returns `429 Too Many Requests` with `Retry-After` header when exceeded
- Enabled by default (300 rpm per IP). Set `rate_limit_enabled: false` to disable.

### Input Validation

- Filter keys for `json_extract` must match `[a-zA-Z0-9_.]+`
- Collection, stream, and queue names are validated at the handler level
- Payload size limited by `hardening.max_payload_bytes` (default 512KB)
- LIMIT and OFFSET use bound SQL parameters (no injection)
