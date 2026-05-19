# Benchmarks

`memoryd` keeps benchmarking lightweight while the storage engine is still
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

Use `MEMORYD_BENCH_ITERS` to change the operation count:

```bash
MEMORYD_BENCH_ITERS=20000 pnpm bench:rust
```

Or pass the iteration count directly to the benchmark example:

```bash
cargo run --release -p memoryd-core --example storage_bench --features sqlite -- 20000
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

Node.js SDK benchmarks should be added next now that the private N-API
`NativeMemoryStore` exists and can exercise the real public package path with
`driver: "native"`.

## Latest Local Baseline

Run date: 2026-05-19

Command:

```bash
pnpm bench:rust
```

Environment:

- Rust: `rustc 1.95.0`
- Iterations: `5000`
- Build: release

Results:

| Store | Operation | Elapsed | Ops/sec |
| --- | --- | ---: | ---: |
| `in-memory` | object put | `2.33ms` | `2,145,922` |
| `in-memory` | object get | `1.908792ms` | `2,620,545` |
| `in-memory` | event append | `926.542us` | `5,399,568` |
| `in-memory` | event list | `285.459us` | `17,543,859` |
| `in-memory` | queue push | `19.91375ms` | `251,092` |
| `in-memory` | queue claim+ack | `44.285167ms` | `112,905` |
| `sqlite-memory` | object put | `32.832167ms` | `152,290` |
| `sqlite-memory` | object get | `9.6165ms` | `519,966` |
| `sqlite-memory` | event append | `16.277375ms` | `307,181` |
| `sqlite-memory` | event list | `817.625us` | `6,119,951` |
| `sqlite-memory` | queue push | `45.531167ms` | `109,815` |
| `sqlite-memory` | queue claim+ack | `163.144667ms` | `30,647` |
| `sqlite-file` | object put | `233.239916ms` | `21,437` |
| `sqlite-file` | object get | `12.801916ms` | `390,594` |
| `sqlite-file` | event append | `201.404125ms` | `24,825` |
| `sqlite-file` | event list | `823.792us` | `6,075,334` |
| `sqlite-file` | queue push | `403.709708ms` | `12,385` |
| `sqlite-file` | queue claim+ack | `943.060667ms` | `5,301` |

Current read: the durable file path is already fine for early local-app
workloads, but queue claim+ack now does real lease-expiry maintenance and
timestamp writes. Write-heavy object/event paths and queue loops will benefit
from explicit batching, fewer one-row transactions, and purpose-built claim
indexes as the API grows.
