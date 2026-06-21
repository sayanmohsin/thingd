# Error Taxonomy

thingd uses structured error responses across all interfaces (MCP, REST, SDK). Every error has a machine-readable `code` and a human-readable `message`.

## REST Error Format

```json
{
  "error": {
    "code": "error_code",
    "message": "Human-readable description"
  }
}
```

HTTP status codes map to error categories:

| HTTP Status | Meaning |
|-------------|---------|
| 400 | Bad request (invalid input, missing fields) |
| 404 | Resource not found |
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

The TypeScript SDK throws `ThingDError` with `code` and `message` properties:

```typescript
try {
  await thingd.put("users", { id: "user-001" });
} catch (err) {
  if (err.code === "not_found") { ... }
  if (err.code === "bad_request") { ... }
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

**Example response:**
```json
{
  "error": {
    "code": "bad_request",
    "message": "Query parameter 'collection' is required"
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
    "code": "not_found",
    "message": "Object 'nonexistent' not found in collection 'users'"
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
    "code": "not_leased",
    "message": "Ack failed: not_leased"
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
    "code": "terminal",
    "message": "Nack failed: terminal"
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

**Example response:**
```json
{
  "error": {
    "code": "internal_error",
    "message": "SQLITE_CONSTRAINT: UNIQUE constraint failed"
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
const res = await fetch("http://localhost:4100/v1/objects/users/user-001");
const json = await res.json();

if (!res.ok) {
  switch (json.error.code) {
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
import { ThingDError } from "@thingd/sdk";

try {
  await thingd.put("users", { id: "user-001" });
} catch (err) {
  if (err instanceof ThingDError) {
    switch (err.code) {
      case "not_found":
        break;
      case "bad_request":
        break;
      default:
        break;
    }
  }
}
```
