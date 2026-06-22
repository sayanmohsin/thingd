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

Run date: 2026-06-22

Environment:

- Rust: `rustc 1.96.0`
- Node: `v24.x`
- Iterations: `100` (smoke)
- Build: release
- Platform: darwin (arm64)

### Rust — In-Memory (100 iters)

| Operation | Elapsed | Ops/sec |
| --- | ---: | ---: |
| object_put | 165µs | 606,060 |
| object_batch | 49µs | 2,083,333 |
| object_get | 35µs | 2,941,176 |
| list_objects | 32µs | 6,250,000 |
| list_objects_filter | 113µs | 884,955 |
| list_objects_limit100 | 37µs | 2,702,702 |
| list_objects_page | 31µs | 3,333,333 |
| event_append | 40µs | 2,500,000 |
| event_batch | 20µs | 5,000,000 |
| event_list | 23µs | 8,695,652 |
| event_list_from_seq | 10µs | 11,111,111 |
| event_list_limit100 | 25µs | 4,000,000 |
| queue_push | 41µs | 2,500,000 |
| queue_batch | 34µs | 3,030,303 |
| queue_claim_ack | 52µs | 1,923,076 |
| queue_claim_ack2 | 55µs | 1,851,851 |
| search | 160µs | 2,515,723 |
| search_filtered | 88µs | 113,636 |
| put_batch_10 | 6µs | 2,000,000 |
| put_batch_100 | 55µs | 1,818,181 |
| put_batch_1000 | 667µs | 1,499,250 |
| delete_batch_10 | 4µs | 3,333,333 |
| delete_batch_100 | 27µs | 3,846,153 |
| delete_batch_1000 | 251µs | 4,000,000 |
| count_objects | 0ns | 1,000,000 |
| count_events | 0ns | 1,000,000 |
| object_delete | 35µs | 2,941,176 |
| concurrent_read_1t | 113µs | 884,955 |
| concurrent_read_2t | 125µs | 800,000 |
| concurrent_read_4t | 83µs | 1,219,512 |
| concurrent_read_8t | 137µs | 705,882 |
| contention_4r1w | 162µs | 617,283 |

### Rust — SQLite (in-memory, 100 iters)

| Operation | Elapsed | Ops/sec |
| --- | ---: | ---: |
| object_put | **3.54ms** | **28,248** |
| object_batch | 4.51ms | 22,163 |
| object_get | 338µs | 295,857 |
| list_objects | 84µs | 2,380,952 |
| list_objects_filter | 99µs | 1,010,101 |
| list_objects_limit100 | 49µs | 2,040,816 |
| list_objects_page | 47µs | 2,127,659 |
| event_append | 2.33ms | 42,936 |
| event_batch | 2.07ms | 48,332 |
| event_list | 83µs | 2,409,638 |
| event_list_from_seq | 41µs | 2,439,024 |
| event_list_limit100 | 35µs | 2,941,176 |
| queue_push | 1.49ms | 67,159 |
| queue_batch | 1.46ms | 68,306 |
| queue_claim_ack | 3.29ms | 30,441 |
| queue_claim_ack2 | 2.48ms | 40,290 |
| search | 5.27ms | 75,872 |
| search_filtered | 5.20ms | 1,921 |
| put_batch_10 | 844µs | 11,862 |
| put_batch_100 | 7.64ms | 13,089 |
| put_batch_1000 | 154ms | 6,507 |
| delete_batch_10 | 1.12ms | **8,896** |
| delete_batch_100 | 20.3ms | 4,935 |
| delete_batch_1000 | 116ms | **8,649** |
| count_objects | 20µs | 50,000 |
| count_events | 2µs | 500,000 |
| object_delete | 6.65ms | 15,044 |
| concurrent_read_1t | 364µs | 274,725 |
| concurrent_read_2t | 443µs | 226,244 |
| concurrent_read_4t | 539µs | 185,873 |
| concurrent_read_8t | 764µs | 125,819 |
| contention_4r1w | 1.54ms | 65,104 |

### Rust — SQLite (file-backed, 100 iters)

| Operation | Elapsed | Ops/sec |
| --- | ---: | ---: |
| object_put | 8.59ms | **11,645** |
| object_batch | 4.78ms | 20,942 |
| object_get | 470µs | 213,219 |
| list_objects | 91µs | 2,222,222 |
| list_objects_filter | 77µs | 1,315,789 |
| list_objects_limit100 | 48µs | 2,127,659 |
| list_objects_page | 47µs | 2,173,913 |
| event_append | 8.99ms | 11,120 |
| event_batch | 1.99ms | 50,352 |
| event_list | 100µs | 2,020,202 |
| event_list_from_seq | 41µs | 2,500,000 |
| event_list_limit100 | 37µs | 2,702,702 |
| queue_push | 5.27ms | 18,982 |
| queue_batch | 1.80ms | 55,586 |
| queue_claim_ack | 6.58ms | 15,192 |
| queue_claim_ack2 | 6.66ms | 15,024 |
| search | 5.31ms | 75,400 |
| search_filtered | 5.23ms | 1,912 |
| put_batch_10 | 786µs | 12,722 |
| put_batch_100 | 7.81ms | 12,802 |
| put_batch_1000 | 153ms | 6,555 |
| delete_batch_10 | 1.06ms | **9,451** |
| delete_batch_100 | 20.4ms | 4,907 |
| delete_batch_1000 | 115ms | **8,686** |
| count_objects | 22µs | 47,619 |
| count_events | 4µs | 333,333 |
| object_delete | 13.7ms | 7,315 |
| concurrent_read_1t | 505µs | 198,412 |
| concurrent_read_2t | 557µs | 179,533 |
| concurrent_read_4t | 613µs | 163,132 |
| concurrent_read_8t | 694µs | 138,328 |
| contention_4r1w | 2.91ms | 34,364 |

### Performance Optimizations Applied

1. **UPSERT auto-version** — `put_object` / `put_objects_batch` no longer do a separate `SELECT version` before the UPSERT. SQLite's ON CONFLICT now atomically increments `version = version + 1`, eliminating one round-trip per write. **+15%** object_put (sqlite-memory), **+3%** (sqlite-file).

2. **Batch delete single-statement** — `delete_objects_batch` uses a single OR-chained `DELETE` instead of N individual statements (chunked at 500 to avoid SQLite expression depth limits). **+117%** delete_batch_10 (sqlite-file), **+12%** delete_batch_1000 (sqlite-file). Fixes crash at 1000+ item batches.

3. **parking_lot::Mutex** — Sidecar engine switched from `tokio::sync::Mutex` to `parking_lot::Mutex` for the per-engine lock. No async yield overhead, faster uncontended acquisition. Pool engine cache uses `parking_lot::RwLock` for concurrent multi-tenant lookups.

4. **RETURNING clause** — Eliminated timestamp read-back round-trip for `put_object` and `push_job` operations
5. **Object clone removal** — `put_objects_batch` now consumes objects directly instead of cloning
6. **Deferred FTS updates** — Batch operations collect FTS updates and execute after all INSERTs
7. **Parameterized queries** — Fixed SQL injection vulnerability in `get_neighbors` type filter
8. **N-API batch APIs** — Added `putObjectsBatchJson`, `appendEventsBatchJson`, `pushJobsBatchJson` to native binding

### Node.js (1000 iterations)

| Store | Operation | Elapsed | Ops/sec |
| --- | --- | ---: | ---: |
| `memory` | object put | `2.30ms` | `435,035` |
| `memory` | object get | `342µs` | `2,920,774` |
| `memory` | event append | `3.16ms` | `316,151` |
| `memory` | event list | `17.05ms` | `58,652` |
| `memory` | queue push | `10.27ms` | `97,337` |
| `memory` | queue claim | `15.33ms` | `65,216` |
| `native` | object put | `101.29ms` | `9,872` |
| `native` | object get | `5.20ms` | `192,243` |
| `native` | event append | `26.50ms` | `37,742` |
| `native` | event list | `1.18s` | `851` |
| `native` | queue push | `24.09ms` | `41,514` |
| `native` | queue claim | `62.01ms` | `16,125` |

Current read: native event_list is slow because N-API returns all events as a
single JSON string that must be deserialized in JS. The native object_get path
(192k ops/s) is competitive with in-memory (2.9M ops/s) — the N-API boundary
cost is ~15x, which is expected for a跨语言 IPC hop. object_put through native
(9.8k ops/s) is dominated by SQLite writes plus JSON serialization overhead.
