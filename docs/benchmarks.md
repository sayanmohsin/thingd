# Benchmarks

The storage benchmark is a local development signal, not a portable product
claim. Results vary with CPU, filesystem, thermal state, Rust version, and
iteration count. The benchmark currently compares the in-memory and persistent
adapters; it does not benchmark REST, MCP, or sidecar throughput.

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
count, delete, concurrent-read, and lock-contention operations for `in-memory`
and `persistent`.
It uses temporary databases and does not leave benchmark data in the repo.

## Latest smoke run

Run date: 2026-08-06
Commit: `6abfa73`
Environment: macOS 26.6 arm64, Rust 1.97.1, Node.js 24.18.0, release build
Iterations: 10

This is intentionally a smoke baseline, not a performance regression gate.
For meaningful comparisons, run the same command on the same machine with the
same iteration count and commit the generated output only when it is a
deliberate baseline update.

| Driver | Representative operation | Ops/sec |
| --- | --- | ---: |
| in-memory | object_put | 51,546 |
| in-memory | object_get | 500,000 |
| in-memory | event_append | 1,250,000 |
| in-memory | queue_claim_ack | 500,000 |
| persistent | object_put | 5 |
| persistent | object_get | 102,040 |
| persistent | event_append | 3 |
| persistent | queue_claim_ack | 10,050 |

The complete output is available from the command above. Do not compare this
smoke table with results from a different machine or iteration count.

## Node.js SDK benchmark

```bash
pnpm bench:node
node packages/thingd/bench/node-bench.mjs 20000
```

This exercises the public SDK through the native driver when available and
through the in-memory fallback. Latest bounded run: 2026-08-06, commit
`6abfa73`, macOS 26.6 arm64, Node.js 24.18.0, 10 iterations. The native
driver is intentionally measured through the existing `:memory:` native path,
which uses the persistent implementation; larger runs are consequently slow
on this workload.

| Driver | Representative operation | Ops/sec |
| --- | --- | ---: |
| memory | object_put | 8,506 |
| memory | object_get | 277,454 |
| memory | event_append | 33,062 |
| memory | queue_claim | 29,766 |
| native | object_put | 5 |
| native | object_get | 76,923 |
| native | event_append | 4 |
| native | queue_claim | 913 |

Node benchmark numbers are also machine-local and should be recorded with
their date, commit, Node version, and iteration count.

## Documentation policy

Benchmark output is not updated automatically because performance numbers are
environment-specific. Code changes that alter benchmark operations must update
this page and the benchmark smoke check. CI should validate that the command
still builds and runs, but should not enforce ops/sec thresholds on shared
runners.
