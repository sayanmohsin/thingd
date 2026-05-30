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

Node.js SDK benchmarks should be added next now that the private N-API
`NativeThingStore` exists and can exercise the real public package path with
`driver: "native"`.

## Latest Local Baseline

Run date: 2026-05-29

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
| `in-memory` | object put | `3.06ms` | `1,633,453` |
| `in-memory` | object get | `1.57ms` | `3,166,561` |
| `in-memory` | event append | `1.26ms` | `3,940,110` |
| `in-memory` | event list | `359.04us` | `13,927,576` |
| `in-memory` | queue push | `27.06ms` | `184,733` |
| `in-memory` | queue claim+ack | `51.41ms` | `97,240` |
| `sqlite-memory` | object put | `43.09ms` | `116,030` |
| `sqlite-memory` | object get | `13.02ms` | `383,877` |
| `sqlite-memory` | event append | `21.47ms` | `232,883` |
| `sqlite-memory` | event list | `1.19ms` | `4,201,680` |
| `sqlite-memory` | queue push | `61.71ms` | `81,014` |
| `sqlite-memory` | queue claim+ack | `145.45ms` | `34,375` |
| `sqlite-file` | object put | `117.77ms` | `42,455` |
| `sqlite-file` | object get | `17.99ms` | `277,824` |
| `sqlite-file` | event append | `109.85ms` | `45,514` |
| `sqlite-file` | event list | `1.10ms` | `4,512,635` |
| `sqlite-file` | queue push | `192.48ms` | `25,976` |
| `sqlite-file` | queue claim+ack | `336.51ms` | `14,858` |

Current read: the durable file path is already fine for early local-app
workloads, but queue claim+ack now does real lease-expiry maintenance and
timestamp writes. Write-heavy object/event paths and queue loops will benefit
from explicit batching, fewer one-row transactions, and purpose-built claim
indexes as the API grows.
