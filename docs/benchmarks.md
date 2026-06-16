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
| `in-memory` | object put | `818µs` | `1,222,493` |
| `in-memory` | object batch | `597µs` | `1,675,041` |
| `in-memory` | object get | `420µs` | `2,380,952` |
| `in-memory` | event append | `357µs` | `2,801,120` |
| `in-memory` | event batch | `178µs` | `5,617,977` |
| `in-memory` | event list | `158µs` | `12,658,227` |
| `in-memory` | queue push | `2.55ms` | `392,310` |
| `in-memory` | queue batch | `1.88ms` | `533,049` |
| `in-memory` | queue claim+ack | `7.83ms` | `127,681` |
| `in-memory` | queue claim+ack (optimized) | `4.93ms` | `202,675` |
| `sqlite-memory` | object put | `106ms` | `9,410` |
| `sqlite-memory` | object batch | `239ms` | `4,181` |
| `sqlite-memory` | object get | `3.12ms` | `320,204` |
| `sqlite-memory` | event append | `20.88ms` | `47,904` |
| `sqlite-memory` | event batch | `7.37ms` | `135,703` |
| `sqlite-memory` | event list | `542µs` | `3,690,036` |
| `sqlite-memory` | queue push | `14.58ms` | `68,591` |
| `sqlite-memory` | queue batch | `12.45ms` | `80,327` |
| `sqlite-memory` | queue claim+ack | `30.89ms` | `32,373` |
| `sqlite-memory` | queue claim+ack (optimized) | `24.03ms` | `41,614` |
| `sqlite-file` | object put | `151ms` | `6,595` |
| `sqlite-file` | object batch | `242ms` | `4,122` |
| `sqlite-file` | object get | `4.32ms` | `231,588` |
| `sqlite-file` | event append | `81.09ms` | `12,332` |
| `sqlite-file` | event batch | `9.01ms` | `110,987` |
| `sqlite-file` | event list | `573µs` | `3,490,401` |
| `sqlite-file` | queue push | `42.73ms` | `23,404` |
| `sqlite-file` | queue batch | `13.59ms` | `73,605` |
| `sqlite-file` | queue claim+ack | `75.12ms` | `13,311` |
| `sqlite-file` | queue claim+ack (optimized) | `47.04ms` | `21,257` |

### Batch API Improvements (sqlite-file)

| Operation | Before | After | Speedup |
| --- | --- | ---: | ---: |
| event append | 12,332 ops/s | 110,987 ops/s | **9x** |
| queue push | 23,404 ops/s | 73,605 ops/s | **3x** |
| queue claim+ack | 13,311 ops/s | 21,257 ops/s | **1.6x** |

Batch APIs (`put_objects_batch`, `append_events_batch`, `push_jobs_batch`) wrap
multiple operations in a single SQLite transaction, eliminating per-operation
commit overhead. Use these for imports, migrations, and bulk data loading.

The optimized `claim_and_ack` method combines claim + ack into a single
transaction, reducing round-trips for queue processing workloads.

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
