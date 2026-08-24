# Search

thingd supports both full-text search and vector similarity search. Full-text uses Tantivy (pure Rust BM25); vector search uses cosine similarity. Each is available via MCP and REST.

## Consistency and degraded search

In persistent modes, object and event writes commit to the durable primary
store first. Tantivy indexing is asynchronous and eventually consistent: the
background worker coalesces updates and commits them at a bounded interval or
batch size. Search may briefly lag a successful write.

If the bounded search queue overflows or a Tantivy commit fails, the primary
write is still retained. thingd marks search stale, serves fallback scanning,
and schedules a bounded rebuild. Readiness remains available when primary
storage is healthy; startup recovery is the exception and keeps `/ready`
unavailable until the required recovery phases finish. The maintenance fields
`search_queue_depth`, `search_queue_capacity`, `search_mutations_queued`,
`search_mutations_coalesced`, `search_mutations_committed`,
`search_last_commit_unix_ms`, `search_last_commit_duration_ms`,
`search_last_error`, `search_retry_count`, and `search_stale` expose this state
through diagnostics.

`persistent-no-rebuild` never writes to Tantivy and uses fallback scanning for
correctness. `disabled` does not open Tantivy.

## Full-Text Search

## Query Syntax

### Basic keywords

Separate words with spaces. All words must match (AND logic).

```
alice bob
```

Matches documents containing both "alice" AND "bob".

### Exact phrases

Wrap phrases in double quotes:

```
"alice chen"
```

Matches the exact phrase "alice chen".

### Prefix matching

Add `*` to the end of a word:

```
alic*
```

Matches "alice", "alicea", etc.

### NOT (exclusion)

Prefix a word with `-`:

```
alice -bob
```

Matches documents containing "alice" but NOT "bob".

### Column filtering

Prefix a word with `column:` to search specific fields:

```
name:alice
```

This only works for indexed columns. In thingd, the search index is built from the JSON-serialized object body, so column filtering is not available for arbitrary fields.

### Mixed queries

```
"alice chen" -bob role:admin
```

## Porter Stemming

Tantivy uses a configurable tokenizer with stemming. The default English stemmer reduces words to their root form:

| Query | Matches |
|-------|---------|
| `choose` | "choose", "chooses", "choosing", "chose", "chosen" |
| `implement` | "implement", "implements", "implementing", "implemented", "implementation" |
| `running` | "run", "runs", "running", "ran" |

This means you don't need to match exact conjugations.

## Scoring

The persistent Tantivy path is the source of full-text matching and stemming.
BM25 score propagation and filtered-result ordering are still being hardened;
the current persistent result envelope may expose a placeholder score until
that work is complete. The intended result shape is:

```json
{
  "kind": "object",
  "id": "user-001",
  "collection": "users",
  "score": 0.21,
  "value": { "..." : "..." }
}
```

## Filtering

### MCP filter (metadata filter)

The `filter` parameter matches against top-level fields in the object body:

```json
{
  "query": "alice",
  "filter": { "role": "admin" }
}
```

This uses JSON equality matching rather than full-text search. The persistent implementation is
being hardened to apply this filter before satisfying the requested result
limit.

### REST filter (query parameter filters)

REST endpoints support `filter.key=value` query parameters for object listing:

```bash
curl "http://localhost:8757/v1/objects?collection=users&filter.role=admin&filter.status=active"
```

## Collections

Limit search to specific collections or streams:

```json
{
  "query": "alice",
  "collections": ["users", "audit"]
}
```

If omitted, search covers all collections and streams.

## Limit

Control the maximum number of results:

```json
{
  "query": "alice",
  "limit": 10
}
```

Default varies by implementation. Max is 100.

## Result Types

Each search result is either an object or an event:

**Object result:**
```json
{
  "kind": "object",
  "id": "user-001",
  "collection": "users",
  "score": 0.21,
  "value": { "id": "user-001", "name": "Alice", "..." : "..." }
}
```

**Event result:**
```json
{
  "kind": "event",
  "id": "26",
  "stream": "users:alice",
  "score": 0.15,
  "value": { "id": "26", "type": "user.login", "..." : "..." }
}
```

## Performance

- Tantivy is used for indexing and querying (pure Rust BM25)
- BM25 scoring and score propagation are a Phase 23 hardening item
- Stemming is done at index time (not query time)
- In-memory mode uses simple substring matching (no Tantivy)
- Performance is comparable to dedicated search engines for < 1M documents

## Examples

### MCP

```json
{
  "tool": "thing_search",
  "arguments": {
    "query": "alice",
    "collections": ["users"],
    "limit": 5
  }
}
```

### REST

```bash
curl -X POST http://localhost:8757/v1/search \
  -H "Content-Type: application/json" \
  -d '{"query": "alice", "collections": ["users"], "limit": 5}'
```

## Vector Search

Search objects by cosine similarity to a query vector. Vectors are stored alongside objects via the optional `vector` field on put (array of floats). The current implementation performs an exact collection scan and returns results ranked by similarity score (-1.0 to 1.0). HNSW/ANN is planned for larger datasets.

### MCP

**Tool:** `thing_vector_search`

**Input:**
```json
{
  "collection": "docs",
  "vector": [0.1, 0.2, 0.3, 0.4, 0.5],
  "topK": 10,
  "filter": { "tag": "important" }
}
```

**Returns:** `VectorSearchHit[]`
```json
[
  {
    "id": "doc-001",
    "score": 0.95,
    "value": { /* StoredMemoryObject */ }
  }
]
```

### REST

```
POST /v1/search/vector
```

```json
{
  "collection": "docs",
  "vector": [0.1, 0.2, 0.3, 0.4, 0.5],
  "topK": 10,
  "filter": { "tag": "important" }
}
```

**Response:**
```json
{
  "data": [
    {
      "id": "doc-001",
      "score": 0.95,
      "value": { "id": "doc-001", "collection": "docs", "body": { "text": "hello" }, "version": 1, "createdAt": "...", "updatedAt": "..." }
    }
  ]
}
```

### Behavior

- Collections without vectors return empty results (not an error)
- Vector dimension must match existing vectors in the collection (validated at search time)
- Results sorted by descending cosine similarity score
- Metadata filter supports exact key-value matching on the object body
