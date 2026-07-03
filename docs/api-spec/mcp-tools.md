# MCP Tools Reference

thingd exposes {{ $themeConfig.mcpToolCount }} MCP tools for AI agents. All tools are available via the stdio MCP server or Streamable HTTP endpoint.

**Tool count:** 27 (16 read-only, 11 write — 3 of which are destructive)

---

## Object Tools

### `thing_search`

Full-text search across objects and events using SQLite FTS5 with Porter stemming. `limit` defaults to 10, max 100.

```json
{
  "query": "search query (required, min 1 char)",
  "collections": ["optional array of collection/stream names"],
  "limit": 10,
  "filter": { "key": "value" }
}
```

**Returns:** `MemorySearchResult[]` — each result has `kind` ("object" or "event"), `id`, `collection`/`stream`, `score`, and `value`.

---

### `thing_get`

Read one object by collection and id.

```json
{ "collection": "users", "id": "user-001" }
```

**Returns:** `StoredMemoryObject | null`

---

### `thing_put`

Create or replace an object. Object must have an `id` field. Emits audit event `objects.put`.

```json
{
  "collection": "users",
  "object": { "id": "user-001", "name": "Alice", "role": "admin" },
  "expectedVersion": 1,
  "actor": "optional",
  "source": "optional"
}
```

**Optimistic locking (CAS):** Set `expectedVersion` to the expected current version. Returns error code `-32603` with detail containing `"Conflict"` if the version does not match.

Delete an object. Emits audit event `objects.delete`.

```json
{ "collection": "users", "id": "user-001", "actor": "optional", "source": "optional" }
```

**Returns:** `{ deleted: boolean }`---

### `thing_objects_list`

List objects in a collection with filter, sort, limit, and offset.

```json
{
  "collection": "users",
  "filter": { "role": "admin" },
  "sortBy": { "field": "created_at", "direction": "desc" },
  "limit": 10,
  "offset": 0
}
```

**Sort fields:** `id`, `collection`, `created_at`, `updated_at`, `version`

**Returns:** `StoredMemoryObject[]`

---

### `thing_objects_put_batch`

Create or replace multiple objects in a single operation. Max 1000 items per call. Emits audit event `objects.put_batch`.

```json
{
  "collection": "users",
  "objects": [
    { "id": "user-010", "name": "Zoe" },
    { "id": "user-011", "name": "Wang" }
  ]
}
```

**Returns:** `StoredMemoryObject[]`

---

### `thing_objects_delete_batch`

Delete multiple objects by ID. Max 1000 IDs per call. Emits audit event `objects.delete_batch`.

```json
{ "collection": "users", "ids": ["user-010", "user-011"] }
```

**Returns:** `{ deleted: number }`

---

## Event Tools

### `thing_events_append`

Append an event to a named stream. Events are append-only with auto-incremented sequence numbers. Emits audit event `events.append`.

```json
{
  "stream": "audit",
  "event": { "type": "user.login", "text": "Logged in from 192.168.1.1" },
  "actor": "optional",
  "source": "optional"
}
```

---

### `thing_events_list`

List events from a stream, optionally filtered by stream name, starting from a specific sequence.

```json
{
  "stream": "audit",
  "fromSequence": 10,
  "limit": 20
}
```

If `stream` is omitted, events from all streams are returned.

---

## Queue Tools

### `thing_queue_push`

Push a durable job onto a named queue. Emits audit event `queue.push`.

```json
{
  "queue": "email-queue",
  "payload": { "to": "alice@example.com", "subject": "Welcome" },
  "idempotencyKey": "optional-key",
  "maxAttempts": 3,
  "delayMs": 0,
  "actor": "optional",
  "source": "optional"
}
```

**Returns:** `QueueJob`

---

### `thing_queue_claim`

Claim the next ready job from a queue. Job is leased for `leaseMs` (default 30s). Emits audit event `queue.claim`.

```json
{ "queue": "email-queue", "leaseMs": 30000, "actor": "optional", "source": "optional" }
```

**Returns:** `QueueJob | null`

---

### `thing_queue_ack`

Mark a leased job as completed. Emits audit event `queue.ack`.

```json
{ "queue": "email-queue", "id": "job-uuid", "actor": "optional", "source": "optional" }
```

**Returns:** `QueueJobResult` — `{ ok: true, job }` or `{ ok: false, reason }`

---

### `thing_queue_nack`

Reject a leased job for retry or dead-letter routing. Emits audit event `queue.nack`.

```json
{
  "queue": "email-queue",
  "id": "job-uuid",
  "delayMs": 5000,
  "error": "SMTP connection failed",
  "actor": "optional",
  "source": "optional"
}
```

**Returns:** `QueueJobResult`

---

### `thing_queue_list`

List all jobs in a queue across all states.

```json
{ "queue": "email-queue" }
```

**Returns:** `QueueJob[]`

---

### `thing_queue_dead`

List dead-letter jobs in a queue.

```json
{ "queue": "email-queue" }
```

**Returns:** `QueueJob[]`

---

## Count Tools

### `thing_count_objects`

Count all objects across all collections.

```json
{}
```

**Returns:** `number`

---

### `thing_count_events`

Count all events across all streams.

```json
{}
```

**Returns:** `number`

---

### `thing_count_active_jobs`

Count all active (non-dead) queue jobs across all queues.

```json
{}
```

**Returns:** `number`

---

### `thing_count_dead_jobs`

Count all dead-letter queue jobs across all queues.

```json
{}
```

**Returns:** `number`

---

## Discovery Tools

### `thing_list_collections`

List all object collection names.

```json
{}
```

**Returns:** `string[]`

---

### `thing_list_streams`

List all event stream names.

```json
{}
```

**Returns:** `string[]`

---

### `thing_list_queues`

List all queue names.

```json
{}
```

**Returns:** `string[]`

---

## Link Tools

### `thing_link_create`

Create a directed graph link between two references. Emits audit event `link.create`.

```json
{
  "fromRef": "users/alice",
  "linkType": "authored",
  "toRef": "memories/post-1",
  "weight": 1.0,
  "metadataJson": "{\"key\": \"value\"}"
}
```

**Returns:** `Link`

---

### `thing_link_get`

Get a link by ID.

```json
{ "id": "link-uuid" }
```

**Returns:** `Link | null`

---

### `thing_link_delete`

Delete a link by ID. Emits audit event `link.delete`.

```json
{ "id": "link-uuid" }
```

**Returns:** `{ deleted: boolean }`

---

### `thing_link_neighbors`

Get all links connected to a reference, with optional direction and type filters.

```json
{
  "reference": "users/alice",
  "direction": "Outgoing",
  "linkType": "authored",
  "limit": 10
}
```

**Directions:** `Outgoing`, `Incoming`, `Both` (default)

**Returns:** `Link[]`

---

### `thing_link_count`

Count all links in the store.

```json
{}
```

**Returns:** `number`

---

## Annotations

| Annotation | Meaning |
|------------|---------|
| `readOnly: true` | Tool does not modify state |
| `destructive: true` | Tool permanently removes data |
| `idempotent: true` | Same call with same args produces same result |

## Audit Trail

All write operations emit audit events to the `__thingd:mcp:audit` stream. Each audit event records:

| Field | Description |
|-------|-------------|
| `tool` | Tool name (e.g. `thing_put`) |
| `actor` | Who performed the action (if provided via optional `actor` parameter) |
| `source` | Where the action originated (if provided via optional `source` parameter) |
| `timestamp` | Unix timestamp (seconds since epoch) |
| `result` | `success` or `error` |

The `__thingd:mcp:audit` stream is protected at the engine level — events cannot be deleted or modified. Direct writes to the audit stream from MCP tools or REST endpoints are rejected. Only the internal audit mechanism can append to this stream. Audit events can be queried via `thing_events_list` with `stream: "__thingd:mcp:audit"`.

## Annotations

All MCP tools include annotations in the `tools/list` response:

| Annotation | Meaning |
|------------|---------|
| `readOnlyHint` | Tool does not modify state |
| `destructiveHint` | Tool permanently removes data |
| `idempotentHint` | Same call with same args produces same result |
| `openWorldHint` | Tool may interact with the outside world |

## Validation Bounds

| Tool | Parameter | Bound |
|------|-----------|-------|
| `thing_search` | `limit` | Max 100 |
| `thing_queue_push` | `maxAttempts` | Max 100 |
| `thing_objects_put_batch` | `objects` | Min 1, max 1000 |
| `thing_objects_delete_batch` | `ids` | Min 1, max 1000 |
