# Benchmarks

`memoryd` keeps benchmarking lightweight while the storage engine is still
forming. The current benchmark exercises the Rust storage trait surface
directly, which is where the durable `rusqlite` adapter lives today.

## Rust Storage Benchmark

Run:

```bash
npm run bench:rust
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

Use `MEMORYD_BENCH_ITERS` to change the operation count:

```bash
MEMORYD_BENCH_ITERS=20000 npm run bench:rust
```

Or pass the iteration count directly to the benchmark example:

```bash
cargo run --release -p memoryd-core --example storage_bench --features sqlite -- 20000
```

## Enforcement

Rerunning `npm run bench:rust` does not update this file. Baseline updates are
intentional documentation changes: run the benchmark, review the output, update
the "Latest Local Baseline" section, and commit that change.

CI enforces that the benchmark stays buildable and runnable with a small smoke
run:

```bash
npm run bench:rust:smoke
```

The smoke run uses 100 iterations. It should catch broken benchmark code or a
broken storage path without pretending GitHub-hosted runners are stable enough
for strict performance regression thresholds.

Do not add ops/sec failure thresholds on shared CI. If `memoryd` later needs
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

Node.js SDK benchmarks should be added after the N-API `NativeMemoryStore`
exists, so the benchmark can exercise the real public package path.

## Latest Local Baseline

Run date: 2026-05-19

Command:

```bash
npm run bench:rust
```

Environment:

- Rust: `rustc 1.95.0`
- Iterations: `5000`
- Build: release

Results:

| Store | Operation | Elapsed | Ops/sec |
| --- | --- | ---: | ---: |
| `in-memory` | object put | `3.345875ms` | `1,494,768` |
| `in-memory` | object get | `1.637125ms` | `3,054,367` |
| `in-memory` | event append | `1.580166ms` | `3,164,556` |
| `in-memory` | event list | `502.416us` | `9,960,159` |
| `in-memory` | queue push | `29.113791ms` | `171,744` |
| `in-memory` | queue claim+ack | `29.841583ms` | `167,554` |
| `sqlite-memory` | object put | `32.989416ms` | `151,565` |
| `sqlite-memory` | object get | `8.891084ms` | `562,366` |
| `sqlite-memory` | event append | `15.433917ms` | `323,981` |
| `sqlite-memory` | event list | `841.666us` | `5,945,303` |
| `sqlite-memory` | queue push | `36.469333ms` | `137,102` |
| `sqlite-memory` | queue claim+ack | `115.987ms` | `43,108` |
| `sqlite-file` | object put | `292.691166ms` | `17,082` |
| `sqlite-file` | object get | `12.23875ms` | `408,563` |
| `sqlite-file` | event append | `293.291792ms` | `17,047` |
| `sqlite-file` | event list | `982.416us` | `5,091,649` |
| `sqlite-file` | queue push | `246.495416ms` | `20,284` |
| `sqlite-file` | queue claim+ack | `523.464958ms` | `9,551` |

Current read: the durable file path is already fine for early local-app
workloads, but write-heavy object/event paths and queue claim+ack loops will
benefit from explicit batching, fewer one-row transactions, and purpose-built
claim indexes as the API grows.
