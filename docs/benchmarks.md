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
| object_put | 4.09ms | 24,473 |
| object_batch | 4.58ms | 21,819 |
| object_get | 334µs | 300,300 |
| list_objects | 88µs | 2,298,850 |
| list_objects_filter | 101µs | 1,000,000 |
| list_objects_limit100 | 47µs | 2,173,913 |
| list_objects_page | 46µs | 2,173,913 |
| event_append | 2.20ms | 45,495 |
| event_batch | 1.90ms | 52,770 |
| event_list | 86µs | 2,325,581 |
| event_list_from_seq | 42µs | 2,380,952 |
| event_list_limit100 | 35µs | 2,941,176 |
| queue_push | 1.52ms | 65,659 |
| queue_batch | 1.42ms | 70,671 |
| queue_claim_ack | 3.13ms | 31,908 |
| queue_claim_ack2 | 2.36ms | 42,337 |
| search | 5.41ms | 73,978 |
| search_filtered | 5.27ms | 1,896 |
| put_batch_10 | 790µs | 12,674 |
| put_batch_100 | 7.99ms | 12,518 |
| put_batch_1000 | 154ms | 6,476 |
| delete_batch_10 | 2.10ms | 4,759 |
| delete_batch_100 | 20.5ms | 4,888 |
| delete_batch_1000 | 128ms | 7,798 |
| count_objects | 11µs | 90,909 |
| count_events | 2µs | 500,000 |
| object_delete | 6.42ms | 15,583 |
| concurrent_read_1t | 382µs | 261,780 |
| concurrent_read_2t | 512µs | 195,312 |
| concurrent_read_4t | 766µs | 130,548 |
| concurrent_read_8t | 616µs | 155,844 |
| contention_4r1w | 1.55ms | 64,391 |

### Rust — SQLite (file-backed, 100 iters)

| Operation | Elapsed | Ops/sec |
| --- | ---: | ---: |
| object_put | 8.86ms | 11,291 |
| object_batch | 4.58ms | 21,815 |
| object_get | 424µs | 236,406 |
| list_objects | 77µs | 2,597,402 |
| list_objects_filter | 69µs | 1,449,275 |
| list_objects_limit100 | 43µs | 2,380,952 |
| list_objects_page | 43µs | 2,380,952 |
| event_append | 8.96ms | 11,160 |
| event_batch | 1.82ms | 54,884 |
| event_list | 78µs | 2,597,402 |
| event_list_from_seq | 38µs | 2,702,702 |
| event_list_limit100 | 35µs | 2,941,176 |
| queue_push | 4.71ms | 21,244 |
| queue_batch | 1.40ms | 71,581 |
| queue_claim_ack | 6.43ms | 15,544 |
| queue_claim_ack2 | 5.74ms | 17,421 |
| search | 5.13ms | 78,033 |
| search_filtered | 5.18ms | 1,929 |
| put_batch_10 | 827µs | 12,091 |
| put_batch_100 | 7.79ms | 12,843 |
| put_batch_1000 | 154ms | 6,477 |
| delete_batch_10 | 2.29ms | 4,363 |
| delete_batch_100 | 20.8ms | 4,797 |
| delete_batch_1000 | 129ms | 7,757 |
| count_objects | 18µs | 58,823 |
| count_events | 4µs | 333,333 |
| object_delete | 12.5ms | 7,998 |
| concurrent_read_1t | 621µs | 161,030 |
| concurrent_read_2t | 733µs | 136,612 |
| concurrent_read_4t | 741µs | 134,952 |
| concurrent_read_8t | 850µs | 113,074 |
| contention_4r1w | 3.18ms | 31,456 |

### Batch API Improvements (sqlite-file)

| Operation | Before | After | Speedup |
| --- | --- | ---: | ---: |
| event append | 12,332 ops/s | 123,502 ops/s | **10x** |
| queue push | 21,989 ops/s | 70,927 ops/s | **3.2x** |
| queue claim+ack | 12,895 ops/s | 19,061 ops/s | **1.5x** |

Batch APIs (`put_objects_batch`, `append_events_batch`, `push_jobs_batch`) wrap
multiple operations in a single SQLite transaction, eliminating per-operation
commit overhead. Use these for imports, migrations, and bulk data loading.

The optimized `claim_and_ack` method combines claim + ack into a single
transaction, reducing round-trips for queue processing workloads.

### Performance Optimizations Applied

1. **RETURNING clause** — Eliminated timestamp read-back round-trip for `put_object` and `push_job` operations
2. **Object clone removal** — `put_objects_batch` now consumes objects directly instead of cloning
3. **Deferred FTS updates** — Batch operations collect FTS updates and execute after all INSERTs
4. **Parameterized queries** — Fixed SQL injection vulnerability in `get_neighbors` type filter
5. **N-API batch APIs** — Added `putObjectsBatchJson`, `appendEventsBatchJson`, `pushJobsBatchJson` to native binding

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
