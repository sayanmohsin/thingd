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
cargo run --release -p thingd-core --example storage_bench --features sqlite -- 20000
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

Run date: 2026-06-15

Environment:

- Rust: `rustc 1.96.0`
- Node: `v24.x`
- Iterations: `1000`
- Build: release

### Rust

| Store | Operation | Elapsed | Ops/sec |
| --- | --- | ---: | ---: |
| `in-memory` | object put | `885µs` | `1,131,221` |
| `in-memory` | object batch | `1.24ms` | `808,407` |
| `in-memory` | object get | `511µs` | `1,960,784` |
| `in-memory` | event append | `423µs` | `2,369,668` |
| `in-memory` | event batch | `206µs` | `4,878,048` |
| `in-memory` | event list | `183µs` | `10,989,010` |
| `in-memory` | queue push | `1.59ms` | `630,517` |
| `in-memory` | queue batch | `1.77ms` | `564,334` |
| `in-memory` | queue claim+ack | `6.75ms` | `148,257` |
| `in-memory` | queue claim+ack (optimized) | `7.04ms` | `142,085` |
| `sqlite-memory` | object put | `109ms` | `9,189` |
| `sqlite-memory` | object batch | `265ms` | `3,771` |
| `sqlite-memory` | object get | `3.12ms` | `321,027` |
| `sqlite-memory` | event append | `20.90ms` | `47,853` |
| `sqlite-memory` | event batch | `7.68ms` | `130,208` |
| `sqlite-memory` | event list | `544µs` | `3,676,470` |
| `sqlite-memory` | queue push | `16.65ms` | `60,063` |
| `sqlite-memory` | queue batch | `15.05ms` | `66,458` |
| `sqlite-memory` | queue claim+ack | `32.36ms` | `30,901` |
| `sqlite-memory` | queue claim+ack (optimized) | `24.41ms` | `40,968` |
| `sqlite-file` | object put | `158ms` | `6,345` |
| `sqlite-file` | object batch | `245ms` | `4,083` |
| `sqlite-file` | object get | `4.29ms` | `233,045` |
| `sqlite-file` | event append | `81.09ms` | `12,331` |
| `sqlite-file` | event batch | `8.10ms` | `123,502` |
| `sqlite-file` | event list | `512µs` | `3,913,894` |
| `sqlite-file` | queue push | `47.69ms` | `20,966` |
| `sqlite-file` | queue batch | `14.10ms` | `70,927` |
| `sqlite-file` | queue claim+ack | `80.57ms` | `12,412` |
| `sqlite-file` | queue claim+ack (optimized) | `52.46ms` | `19,061` |

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
