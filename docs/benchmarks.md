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

Run date: 2026-06-07

Environment:

- Rust: `rustc 1.96.0`
- Node: `v24.x`
- Iterations: `5000`
- Build: release

### Rust

| Store | Operation | Elapsed | Ops/sec |
| --- | --- | ---: | ---: |
| `in-memory` | object put | `5.76ms` | `868,809` |
| `in-memory` | object get | `2.62ms` | `1,906,941` |
| `in-memory` | event append | `2.33ms` | `2,143,163` |
| `in-memory` | event list | `542µs` | `9,225,092` |
| `in-memory` | queue push | `32.68ms` | `152,984` |
| `in-memory` | queue claim+ack | `56.17ms` | `89,010` |
| `sqlite-memory` | object put | `1.94s` | `2,573` |
| `sqlite-memory` | object get | `15.30ms` | `326,733` |
| `sqlite-memory` | event append | `105.04ms` | `47,601` |
| `sqlite-memory` | event list | `1.31ms` | `3,831,417` |
| `sqlite-memory` | queue push | `74.04ms` | `67,535` |
| `sqlite-memory` | queue claim+ack | `154.83ms` | `32,293` |
| `sqlite-file` | object put | `2.20s` | `2,277` |
| `sqlite-file` | object get | `21.03ms` | `237,812` |
| `sqlite-file` | event append | `355.80ms` | `14,053` |
| `sqlite-file` | event list | `1.30ms` | `3,837,298` |
| `sqlite-file` | queue push | `195.66ms` | `25,554` |
| `sqlite-file` | queue claim+ack | `338.15ms` | `14,786` |

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
