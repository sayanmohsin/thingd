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
  object write.

Queue benchmarks should be added after SQLite queue persistence is implemented.
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
| `in-memory` | object put | `3.708041ms` | `1,348,435` |
| `in-memory` | object get | `1.789667ms` | `2,794,857` |
| `in-memory` | event append | `1.341959ms` | `3,728,560` |
| `in-memory` | event list | `353.083us` | `14,164,305` |
| `sqlite-memory` | object put | `41.395459ms` | `120,787` |
| `sqlite-memory` | object get | `9.564125ms` | `522,793` |
| `sqlite-memory` | event append | `16.484125ms` | `303,324` |
| `sqlite-memory` | event list | `830.542us` | `6,024,096` |
| `sqlite-file` | object put | `199.284459ms` | `25,089` |
| `sqlite-file` | object get | `12.1275ms` | `412,303` |
| `sqlite-file` | event append | `185.5375ms` | `26,948` |
| `sqlite-file` | event list | `801.125us` | `6,242,197` |

Current read: the durable file path is already fine for early local-app
workloads, but write-heavy paths will benefit from explicit batching and fewer
one-row transactions once the API grows batch primitives.
