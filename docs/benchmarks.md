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

Run date: 2026-07-30
Commit: `677aba8`
Environment: macOS arm64, Rust 1.96.0, release build
Iterations: 10

This is intentionally a smoke baseline, not a performance regression gate.
For meaningful comparisons, run the same command on the same machine with the
same iteration count and commit the generated output only when it is a
deliberate baseline update.

| Driver | Representative operation | Ops/sec |
| --- | --- | ---: |
| in-memory | object_put | 86,206 |
| in-memory | object_get | 555,555 |
| in-memory | event_append | 1,666,666 |
| in-memory | queue_claim_ack | 3,333,333 |
| persistent | object_put | 10 |
| persistent | object_get | 147,058 |
| persistent | event_append | 6 |
| persistent | queue_claim_ack | 26,178 |

The complete output is available from the command above. Do not compare this
smoke table with results from a different machine or iteration count.

## Node.js SDK benchmark

```bash
pnpm bench:node
node packages/thingd/bench/node-bench.mjs 20000
```

This exercises the public SDK through the native driver when available and
through the in-memory fallback. Node benchmark numbers are also machine-local
and should be recorded with their date, commit, Node version, and iteration
count.

## Documentation policy

Benchmark output is not updated automatically because performance numbers are
environment-specific. Code changes that alter benchmark operations must update
this page and the benchmark smoke check. CI should validate that the command
still builds and runs, but should not enforce ops/sec thresholds on shared
runners.
