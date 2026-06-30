---
name: fix-search-consistency
description: |
  Use when investigating or fixing search index consistency in thingd's
  Rust engine (FTS5-backed SqliteThingStore). Covers root cause analysis,
  transactional audit, defensive query patterns, and integration tests.
---

# Fix Search Index Consistency

## 1. Reproduce the symptom

```bash
cargo test -p thingd -- test_fts5_search_indexing_and_stemming --nocapture
```

## 2. Audit transaction boundaries

Search `crates/thingd/src/sqlite.rs` for all `DELETE FROM search_index`
and `INSERT INTO search_index` occurrences. Verify every mutation updates
*both* the main table and the FTS index inside the *same* explicit
`self.connection.transaction()` block.

Key mutation methods to check:

- `put_object` / `put_objects_batch` / `put_object_with_options`
- `delete_object` / `delete_objects_batch`
- `append_event` / `append_events_batch`
- `delete_last_event` / `delete_stream`

Each entry in the FTS table has a `(collection, id, kind)` tuple that
joins back to `objects(collection, id)` or `events(stream, sequence)`.

## 3. Add or verify defensive WHERE clause

The search query at `sqlite.rs:~1545` performs:

```sql
FROM search_index s
LEFT JOIN objects o ON s.kind = 'object' AND s.collection = o.collection AND s.id = o.id
LEFT JOIN events  e ON s.kind = 'event'  AND s.collection = e.stream  AND s.id = CAST(e.sequence AS TEXT)
```

A `LEFT JOIN` without `IS NOT NULL` guards means orphaned FTS entries
(rows with no matching `objects`/`events` row) will hit
`row.get::<_, String>(4)` on a NULL column and crash. Add:

```sql
WHERE search_index MATCH ?1
  AND (s.kind != 'object' OR o.collection IS NOT NULL)
  AND (s.kind != 'event'  OR e.stream IS NOT NULL)
```

## 4. Add integration tests

**4a. Search consistency after the specific mutation path** — reproduce
the exact sequence from the bug report, then assert search results match.

**4b. Orphaned FTS entry defense** — insert an FTS row directly via SQL
(bypassing the API), then verify search silently excludes it.

Use `SqliteThingStore::open_in_memory()` with `MemoryObject::new(...)`.

## 5. Verify

```bash
cargo test --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check
```

## Relevant files

- `crates/thingd/src/sqlite.rs` — FTS5 search + index updates
- `crates/thingd/src/in_memory.rs` — simple substring search (no FTS)
- `crates/thingd/src/store.rs` — SearchOptions + trait definition
- `docs/api-spec/search.md` — search documentation
