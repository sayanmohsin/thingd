# Search

thingd supports both full-text search and vector similarity search. Full-text uses Tantivy (pure Rust BM25); vector search uses cosine similarity. Each is available via MCP and REST.

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

Results are ranked by BM25 relevance score (lower = more relevant). The score is included in each search result:

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

This uses JSON equality matching (not FTS5), applied as a post-filter after full-text search.

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
- BM25 scoring is computed at query time
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

Search objects by cosine similarity to a query vector. Vectors are stored alongside objects via the optional `vector` field on put (array of floats). Returns results ranked by similarity score (0.0 to 1.0).

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
