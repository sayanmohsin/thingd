# Benchmarks

`thingd` keeps benchmarking lightweight while the storage engine is still
forming. The current benchmark exercises the Rust storage trait surface
directly, which is where the durable `rusqlite` adapter lives today.

## Rust Storage Benchmark

Run:

```bash
pnpm bench:rust
```

The benchmark compares:

- `in-memory`
- `sqlite-memory`
- `sqlite-file`

It measures:

- object writes
- object reads
- event appends
- event stream listing
- queue pushes
- queue claim plus ack loops

Use `THINGD_BENCH_ITERS` to change the operation count:

```bash
THINGD_BENCH_ITERS=20000 pnpm bench:rust
```

Or pass the iteration count directly to the benchmark example:

```bash
cargo run --release -p thingd --example storage_bench --features sqlite -- 20000
```

## Node.js SDK Benchmark

Run:

```bash
pnpm bench:node
```

This benchmarks the public `ThingD` SDK through the N-API native driver
(if built) and the in-memory fallback. It exercises the same operations
as the Rust benchmark but through the JS API:

- object put / get
- event append / list
- queue push / claim / ack

Use the iteration argument to scale:

```bash
node packages/thingd/bench/node-bench.mjs 20000
```

## Enforcement

Rerunning `pnpm bench:rust` does not update this file. Baseline updates are
intentional documentation changes: run the benchmark, review the output, update
the "Latest Local Baseline" section, and commit that change.

CI enforces that the benchmark stays buildable and runnable with a small smoke
run:

```bash
pnpm bench:rust:smoke
```

The smoke run uses 100 iterations. It should catch broken benchmark code or a
broken storage path without pretending GitHub-hosted runners are stable enough
for strict performance regression thresholds.

Do not add ops/sec failure thresholds on shared CI. If `thingd` later needs
hard performance gates, use a dedicated machine or self-hosted runner with a
pinned environment and a deliberately chosen tolerance.

## Reading Results

The benchmark is a local development signal, not a published performance
claim. Numbers depend on machine, filesystem, thermal state, Rust version, and
whether the database is in-memory or file-backed.

Expected shape:

- `in-memory` is the upper bound for trait overhead.
- `sqlite-memory` shows SQLite execution cost without filesystem durability.
- `sqlite-file` shows the current durable write path with one transaction per
  object or queue write.

## Latest Local Baseline

Run date: 2026-06-30

Environment:

- Rust: `rustc 1.96.0`
- Node: `v24.x`
- Iterations: `100` (smoke)
- Build: release
- Platform: darwin (arm64)

### Rust — In-Memory (100 iters)

| Operation | Elapsed | Ops/sec |
| --- | ---: | ---: |
| object_put | 155µs | 649,350 |
| object_batch | 49µs | 2,040,816 |
| object_get | 48µs | 2,083,333 |
| list_objects | 34µs | 6,060,606 |
| list_objects_filter | 141µs | 709,219 |
| list_objects_limit100 | 35µs | 2,857,142 |
| list_objects_page | 31µs | 3,333,333 |
| event_append | 41µs | 2,500,000 |
| event_batch | 19µs | 5,263,157 |
| event_list | 22µs | 9,090,909 |
| event_list_from_seq | 10µs | 10,000,000 |
| event_list_limit100 | 26µs | 3,846,153 |
| queue_push | 39µs | 2,564,102 |
| queue_batch | 37µs | 2,777,777 |
| queue_claim_ack | 52µs | 1,923,076 |
| queue_claim_ack2 | 56µs | 1,818,181 |
| search | 160µs | 2,500,000 |
| search_filtered | 90µs | 112,359 |
| put_batch_10 | 7µs | 1,666,666 |
| put_batch_100 | 57µs | 1,754,385 |
| put_batch_1000 | 673µs | 1,485,884 |
| delete_batch_10 | 4µs | 3,333,333 |
| delete_batch_100 | 27µs | 3,846,153 |
| delete_batch_1000 | 253µs | 3,952,569 |
| count_objects | 0ns | 1,000,000 |
| count_events | 0ns | 1,000,000 |
| object_delete | 34µs | 3,030,303 |
| concurrent_read_1t | 107µs | 934,579 |
| concurrent_read_2t | 117µs | 854,700 |
| concurrent_read_4t | 137µs | 729,927 |
| concurrent_read_8t | 143µs | 676,056 |
| contention_4r1w | 142µs | 709,219 |

### Rust — SQLite (in-memory, 100 iters)

| Operation | Elapsed | Ops/sec |
| --- | ---: | ---: |
| object_put | 3.70ms | 27,034 |
| object_batch | 3.77ms | 26,539 |
| object_get | 322µs | 311,526 |
| list_objects | 77µs | 2,597,402 |
| list_objects_filter | 78µs | 1,282,051 |
| list_objects_limit100 | 50µs | 2,040,816 |
| list_objects_page | 49µs | 2,083,333 |
| event_append | 2.29ms | 43,706 |
| event_batch | **816µs** | **122,549** |
| event_list | 82µs | 2,469,135 |
| event_list_from_seq | 40µs | 2,500,000 |
| event_list_limit100 | 43µs | 2,380,952 |
| queue_push | 1.73ms | 57,870 |
| queue_batch | 1.39ms | 71,736 |
| queue_claim_ack | 3.08ms | 32,509 |
| queue_claim_ack2 | 2.37ms | 42,229 |
| search | 5.28ms | 18,925 |
| search_filtered | 473µs | 21,141 |
| put_batch_10 | 755µs | 13,262 |
| put_batch_100 | 7.21ms | 13,863 |
| put_batch_1000 | 146ms | 6,811 |
| delete_batch_10 | 1.05ms | 9,541 |
| delete_batch_100 | 21.3ms | 4,690 |
| delete_batch_1000 | 115ms | 8,693 |
| count_objects | 23µs | 43,478 |
| count_events | 2µs | 500,000 |
| object_delete | 6.61ms | 15,130 |
| concurrent_read_1t | 395µs | 253,807 |
| concurrent_read_2t | 464µs | 215,982 |
| concurrent_read_4t | 607µs | 164,744 |
| concurrent_read_8t | 796µs | 120,754 |
| contention_4r1w | 1.63ms | 61,538 |

### Rust — SQLite (file-backed, 100 iters)

| Operation | Elapsed | Ops/sec |
| --- | ---: | ---: |
| object_put | 10.2ms | 9,819 |
| object_batch | 4.06ms | 24,624 |
| object_get | 463µs | 215,982 |
| list_objects | 80µs | 2,531,645 |
| list_objects_filter | 74µs | 1,369,863 |
| list_objects_limit100 | 52µs | 1,960,784 |
| list_objects_page | 64µs | 1,562,500 |
| event_append | 9.42ms | 10,620 |
| event_batch | **863µs** | **115,874** |
| event_list | 81µs | 2,500,000 |
| event_list_from_seq | 38µs | 2,702,702 |
| event_list_limit100 | 39µs | 2,564,102 |
| queue_push | 3.86ms | 25,926 |
| queue_batch | 1.39ms | 71,890 |
| queue_claim_ack | 8.08ms | 12,371 |
| queue_claim_ack2 | 4.05ms | 24,703 |
| search | 5.18ms | 19,290 |
| search_filtered | 507µs | 19,723 |
| put_batch_10 | 842µs | 11,890 |
| put_batch_100 | 7.09ms | 14,106 |
| put_batch_1000 | 148ms | 6,737 |
| delete_batch_10 | 1.14ms | 8,787 |
| delete_batch_100 | 20.3ms | 4,920 |
| delete_batch_1000 | 116ms | 8,645 |
| count_objects | 31µs | 33,333 |
| count_events | 4µs | 250,000 |
| object_delete | 13.3ms | 7,532 |
| concurrent_read_1t | 511µs | 196,078 |
| concurrent_read_2t | 698µs | 143,472 |
| concurrent_read_4t | 844µs | 118,623 |
| concurrent_read_8t | 772µs | 124,352 |
| contention_4r1w | 4.56ms | 21,939 |

### Performance Optimizations Applied

1. **UPSERT auto-version** — `put_object` / `put_objects_batch` no longer do a separate `SELECT version` before the UPSERT. SQLite's ON CONFLICT now atomically increments `version = version + 1`, eliminating one round-trip per write. **+15%** object_put (sqlite-memory), **+3%** (sqlite-file).

2. **Batch delete single-statement** — `delete_objects_batch` uses a single OR-chained `DELETE` instead of N individual statements (chunked at 500 to avoid SQLite expression depth limits). **+117%** delete_batch_10 (sqlite-file), **+12%** delete_batch_1000 (sqlite-file). Fixes crash at 1000+ item batches.

3. **parking_lot::Mutex** — Sidecar engine switched from `tokio::sync::Mutex` to `parking_lot::Mutex` for the per-engine lock. No async yield overhead, faster uncontended acquisition. Pool engine cache uses `parking_lot::RwLock` for concurrent multi-tenant lookups.

4. **RETURNING clause** — Eliminated timestamp read-back round-trip for `put_object` and `push_job` operations
5. **Object clone removal** — `put_objects_batch` now consumes objects directly instead of cloning
6. **Deferred FTS updates** — Batch operations collect FTS updates and execute after all INSERTs
7. **Parameterized queries** — Fixed SQL injection vulnerability in `get_neighbors` type filter
8. **N-API batch APIs** — Added `putObjectsBatchJson`, `appendEventsBatchJson`, `pushJobsBatchJson` to native binding
9. **Multi-row batch INSERT** — `put_objects_batch` and `append_events_batch` now use a single multi-row `INSERT ... VALUES (...), (...), ... RETURNING` instead of N individual `query_row` calls. **+153%** event_batch (sqlite-memory from 2.07ms → 816µs), **+130%** event_batch (sqlite-file from 1.99ms → 863µs).
10. **Reader/writer connection pool** — Sidecar now holds 3 reader connections + 1 writer connection per database. Read handlers use the reader pool (concurrent via separate SQLite WAL readers), write handlers use the writer. No benchmark impact for single-threaded tests, but significantly improves throughput under concurrent load.

### Node.js (5000 iterations)

| Store | Operation | Elapsed | Ops/sec |
| --- | --- | ---: | ---: |
| `memory` | object put | 15.44ms | 323,826 |
| `memory` | object get | 11.51ms | 434,427 |
| `memory` | event append | 8.93ms | 559,798 |
| `memory` | event list | 471ms | 10,614 |
| `memory` | queue push | 161ms | 31,071 |
| `memory` | queue claim | 278ms | 17,962 |
| `native` | object put | 1.98s | 2,526 |
| `native` | object get | 25ms | 199,970 |
| `native` | event append | 140ms | 35,690 |
| `native` | event list | 28.9s | 173 |
| `native` | queue push | 126ms | 39,799 |
| `native` | queue claim | 1.04s | 4,819 | |

Current read: native event_list is slow because N-API returns all events as a
single JSON string that must be deserialized in JS. The native object_get path
(192k ops/s) is competitive with in-memory (2.9M ops/s) — the N-API boundary
cost is ~15x, which is expected for a跨语言 IPC hop. object_put through native
(9.8k ops/s) is dominated by SQLite writes plus JSON serialization overhead.
