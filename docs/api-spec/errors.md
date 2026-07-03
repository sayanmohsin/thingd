# Error Taxonomy

thingd uses structured error responses across all interfaces (MCP, REST, SDK). Every error has a `type` (machine-readable), `title`, `status` (HTTP code), and `detail` (human-readable).

## REST Error Format

```json
{
  "error": {
    "type": "error_code",
    "title": "Human-readable title",
    "status": 400,
    "detail": "Detailed description"
  }
}
```

**Production mode:** When `server.production_mode` is `true`, the `detail` field is empty for `internal_error` responses. Full details are logged server-side.

HTTP status codes map to error categories:

| HTTP Status | Meaning |
|-------------|---------|
| 400 | Bad request (invalid input, missing fields) |
| 404 | Resource not found |
| 409 | Conflict (duplicate, state mismatch) |
| 429 | Too many requests (rate limit exceeded) |
| 500 | Internal server error |

## MCP Error Format

MCP errors are returned via the `isError: true` flag in the tool result:

```json
{
  "content": [{ "type": "text", "text": "Error message" }],
  "isError": true
}
```

## SDK Error Format

The TypeScript SDK throws `Error` objects with descriptive messages. The REST client (CloudThingStore) may throw plain `Error` objects from HTTP errors, while the MCP client throws errors received from the MCP tool layer:

```typescript
try {
  await thingd.put("users", { id: "user-001" });
} catch (err) {
  const message = err instanceof Error ? err.message : String(err);
  // Check for error type or HTTP status in the message
}
```

## Error Codes

### `bad_request`

**HTTP 400** — Invalid input or missing required field.

| Scenario | Example |
|----------|---------|
| Missing query param | `GET /v1/objects` without `collection` |
| Missing body field | `POST /v1/search` without `query` |
| Invalid JSON | Malformed request body |
| Missing required fields | `POST /v1/links` without `fromRef` |
| Invalid enum value | `sortBy.field` not in allowed list |
| Write to protected stream | `POST /v1/events/__thingd:mcp:audit` |

**Example response:**
```json
{
  "error": {
    "type": "bad_request",
    "title": "Bad Request",
    "status": 400,
    "detail": "Query parameter 'collection' is required"
  }
}
```

---

### `not_found`

**HTTP 404** — Resource does not exist.

| Scenario | Example |
|----------|---------|
| Object not in collection | `GET /v1/objects/users/nonexistent` |
| Link not found | `GET /v1/links?id=invalid` |
| No route matched | `GET /v1/nonexistent` |

**Example response:**
```json
{
  "error": {
    "type": "not_found",
    "title": "Not Found",
    "status": 404,
    "detail": "Object 'nonexistent' not found in collection 'users'"
  }
}
```

---

### `conflict`

**HTTP 409** — Operation conflicts with current state.

| Scenario | Example |
|----------|---------|
| Duplicate entry | Creating a resource that already exists |
| State mismatch | Operation incompatible with current resource state |

**Example response:**
```json
{
  "error": {
    "type": "conflict",
    "title": "Conflict",
    "status": 409,
    "detail": "Resource already exists"
  }
}
```

---

### `too_many_requests`

**HTTP 429** — Rate limit exceeded.

Returned when `hardening.rate_limit_enabled` is true and the client exceeds the configured rate limit.

**Example response:**
```json
{
  "error": {
    "type": "too_many_requests",
    "title": "Too Many Requests",
    "status": 429,
    "detail": "Rate limit exceeded. Try again later."
  }
}
```

---

### `not_leased`

**HTTP 400** — Queue job is not currently leased.

The job exists but is in `ready` or `completed`/`dead` state, so `ack()` or `nack()` cannot be applied.

**Example response:**
```json
{
  "error": {
    "type": "not_leased",
    "title": "Not Leased",
    "status": 400,
    "detail": "Ack failed: not_leased"
  }
}
```

---

### `terminal`

**HTTP 400** — Queue job is already completed or dead.

The job has reached a terminal state and cannot be acked or nacked again.

**Example response:**
```json
{
  "error": {
    "type": "terminal",
    "title": "Terminal",
    "status": 400,
    "detail": "Nack failed: terminal"
  }
}
```

---

### `internal_error`

**HTTP 500** — Unexpected server error.

| Scenario | Example |
|----------|---------|
| Database corruption | SQLite disk I/O error |
| Unhandled exception | Unexpected null reference |
| Store failure | Underlying storage backend error |

**Example response (development mode):**
```json
{
  "error": {
    "type": "internal_error",
    "title": "Internal Server Error",
    "status": 500,
    "detail": "storage error: disk I/O error"
  }
}
```

**Example response (production mode):**
```json
{
  "error": {
    "type": "internal_error",
    "title": "Internal Server Error",
    "status": 500,
    "detail": ""
  }
}
```

## Queue-Specific Errors

Queue operations (`ack`, `nack`) return a discriminated union instead of throwing:

**Success:**
```json
{ "ok": true, "job": { "..." : "..." } }
```

**Failure:**
```json
{ "ok": false, "reason": "not_found" }
```

| Reason | Meaning |
|--------|---------|
| `not_found` | Job ID does not exist |
| `not_leased` | Job exists but is not currently leased |
| `terminal` | Job is completed or dead |

## Error Handling Best Practices

### REST

```javascript
const res = await fetch("http://localhost:8757/v1/objects/users/user-001");
const json = await res.json();

if (!res.ok) {
  switch (json.error.type) {
    case "not_found":
      // handle missing object
      break;
    case "bad_request":
      // handle invalid input
      break;
    default:
      // handle unexpected error
  }
}
```

### MCP

MCP errors are returned as tool results with `isError: true`. The agent should handle these gracefully:

```json
{
  "content": [{ "type": "text", "text": "Object 'user-001' not found in collection 'users'" }],
  "isError": true
}
```

### SDK

```typescript
try {
  await thingd.put("users", { id: "user-001" });
} catch (err) {
  const message = err instanceof Error ? err.message : String(err);
  // Handle based on message content or HTTP status
}
```
