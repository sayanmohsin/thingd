# Benchmarks

The storage benchmark is a local development signal, not a portable product
claim. Results vary with CPU, filesystem, thermal state, Rust version, and
iteration count. The benchmark compares the in-memory adapter, the default
RocksDB-backed persistent adapter, and the experimental ThingDB adapter; it
does not benchmark REST, MCP, or sidecar throughput.

## Run

```bash
pnpm bench:rust
pnpm bench:rust:smoke
```

The default is 5,000 iterations. Override it with either
`THINGD_BENCH_ITERS=20000 pnpm bench:rust` or by passing the count directly to
the Rust example. The smoke command uses 10 iterations and is intended only to
verify that the benchmark remains buildable and runnable.

The benchmark measures object, event, queue, search, vector search, batch,
count, delete, concurrent-read, and lock-contention operations for `in-memory`,
`persistent` (RocksDB), and `thingdb-experimental`.

Each persistent run also reports a reopen/startup duration and first-search
latency. To exercise the low-memory path on Linux/macOS, run the benchmark in
an externally constrained process and record the platform's peak RSS:

```bash
ulimit -v 1048576
THINGD_BENCH_ITERS=100 cargo run --release -p thingd --example storage_bench --features persistent,search
```

Repeat with `ulimit -v 2097152`. The limit is an operator-controlled benchmark
constraint rather than a production setting; a process that exceeds it must
fail clearly, not be treated as a passing performance result.

Use `THINGD_BENCH_SEARCH_MODE=disabled` to measure the low-memory fallback scan
without opening a persistent search index. Use `persistent` for the normal
Tantivy path and `persistent-no-rebuild` for the no-startup-rebuild path.
It uses temporary databases and does not leave benchmark data in the repo.

## Latest smoke run

Run date: 2026-08-06
Commit: `a8ab14c`
Environment: macOS 26.6 arm64, Rust 1.97.1, Node.js 24.18.0, release build
Iterations: 10

This is intentionally a smoke baseline, not a performance regression gate.
For meaningful comparisons, run the same command on the same machine with the
same iteration count and commit the generated output only when it is a
deliberate baseline update.

| Driver | Representative operation | Ops/sec |
| --- | --- | ---: |
| in-memory | object_put | 26,385 |
| in-memory | object_get | 222,222 |
| in-memory | event_append | 454,545 |
| in-memory | queue_claim_ack | 238,095 |
| persistent | object_put | 5 |
| persistent | object_get | 222,222 |
| persistent | event_append | 3 |
| persistent | queue_claim_ack | 2,316 |

The complete output is available from the command above. ThingDB numbers are
exploratory until its large-store and crash-recovery gates pass. Do not compare this
smoke table with results from a different machine or iteration count.

## Node.js SDK benchmark

```bash
pnpm bench:node
node packages/thingd/bench/node-bench.mjs 20000
```

This exercises the public SDK through the native driver when available and
through the in-memory fallback. Latest bounded run: 2026-08-06, commit
`a8ab14c`, macOS 26.6 arm64, Node.js 24.18.0, 10 iterations. The native
driver is intentionally measured through the existing `:memory:` native path,
which uses the persistent implementation; larger runs are consequently slow
on this workload.

| Driver | Representative operation | Ops/sec |
| --- | --- | ---: |
| memory | object_put | 9,613 |
| memory | object_get | 289,503 |
| memory | event_append | 33,389 |
| memory | queue_claim | 35,268 |
| native | object_put | 5 |
| native | object_get | 33,708 |
| native | event_append | 3 |
| native | queue_claim | 551 |

Node benchmark numbers are also machine-local and should be recorded with
their date, commit, Node version, and iteration count.

## Documentation policy

Benchmark output is not updated automatically because performance numbers are
environment-specific. Code changes that alter benchmark operations must update
this page and the benchmark smoke check. CI should validate that the command
still builds and runs, but should not enforce ops/sec thresholds on shared
runners.
