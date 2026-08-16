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
pnpm bench:rust:structured
```

The default is 5,000 iterations. Override it with either
`THINGD_BENCH_ITERS=20000 pnpm bench:rust` or by passing the count directly to
the Rust example. The smoke command uses 10 deterministic iterations and is
intended only to verify that the benchmark remains buildable and runnable.

The Rust storage benchmark is one unified harness: WAL, object, event, queue,
search, vector, batch, delete, concurrency, encryption, and lifecycle workloads
are all measured by the same executable against the selected backends. WAL
measurements are included in the normal structured run rather than maintained
as a separate benchmark. Use `--repetitions 5 --phase wal-hardening` when
recording a Phase 1A comparison; generated JSON and JSONL files must not be
committed.

For a reproducible RocksDB-vs-ThingDB run with structured output:

```bash
cargo run --release -p thingd --example storage_bench --features persistent,search -- \
  --iterations 1000 --seed 42 --backend all --output target/storage-benchmark.json
```

Use `--output target/storage-benchmark.csv` for a flat CSV result. The output
includes commit, Rust, operating system, architecture, seed, operation counts,
throughput, p50/p95/p99/max latency, and durable directory size. Each durable
backend gets a fresh temporary directory, and correctness/reopen checks run
before the command succeeds. A correctness or recovery error is a failed run,
not a performance datapoint.

Every run is also appended automatically to
`target/storage-benchmark-history.jsonl`. Set `--history` or
`THINGD_BENCH_HISTORY` to retain history elsewhere, and set `--phase` or
`THINGD_BENCH_PHASE` to label a run, for example `reliability-baseline`,
`wal-group-commit`, or `table-indexes`. Each history record contains the date,
branch, commit, environment, selected backend, and all measured RocksDB,
ThingDB, or in-memory workload rows. This makes repeated runs comparable by
phase and date without committing machine-specific results.

The benchmark selects `--backend all|rocksdb|thingdb|memory`; `all` is the
default and is the required comparison mode. It measures object, event, queue,
search, vector search, batch, count, delete, concurrent-read, and
lock-contention operations for the selected adapters.

Use `--repetitions 5` or `THINGD_BENCH_REPETITIONS=5` for phase comparisons.
Structured JSON output includes grouped median, minimum, maximum, and spread
throughput summaries in addition to each repetition's raw result.
The WAL-hardening and group-commit phases additionally record
`wal-single-write`, `wal-explicit-batch`, `wal-concurrent-write`, and
`wal-recovery` rows plus ThingDB WAL timing diagnostics. Single writes remain
sync-before-ack; grouped writes reduce physical sync calls without trading
durability for throughput. Group-commit diagnostics include logical commits,
physical sync calls, average and maximum group size, and queue wait time.

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

## ThingDB benchmark plan

ThingDB benchmarking is a promotion gate, not a marketing benchmark. Every
ThingDB result must include the matching RocksDB result from the same binary,
machine, filesystem, dataset, and workload configuration.

### Workload matrix

Run each workload against both durable backends at 10k, 100k, and 1M logical
records. Add 10M records only after the 1M run is stable.

| Area | Workloads |
| --- | --- |
| Ingest and updates | sequential puts, random puts, updates, mixed put/update |
| Reads | point gets, random reads, cold restart reads, concurrent reads |
| Ordered access | prefix scans, bounded range scans, full ordered scans |
| Deletes | random deletes, delete-heavy mixed workload, tombstone cleanup |
| Atomicity | single-key writes, multi-key batches, cross-keyspace batches |
| Durability | sync writes, restart after each workload, WAL truncation/replay |
| Maintenance | flush, compaction, interrupted compaction, repeated reopen |
| Search integration | durable writes plus derived Tantivy rebuild and catch-up |
| Operations | repack, backup/restore, encryption reopen, disk usage and rollback |

### Measurements

Record throughput and p50/p95/p99 latency for each operation, plus total
ingest time, restart time, recovery time, compaction time, peak RSS, CPU time,
WAL bytes, table bytes, total disk usage, write amplification, and read/write
stall time. Record failures, checksum errors, lost records, duplicate records,
and search lag separately from normal latency.

Run at least five repetitions per workload and report the median plus the
spread. Use release builds, pinned Rust and dependency versions, a dedicated
filesystem, and the same logical input for both backends. Separate cold-cache
and warm-cache runs, and report whether Tantivy is disabled, synchronous, or
asynchronous. Never compare numbers from different machines as a regression
claim.

### Reliability and promotion gates

Before moving beyond the experimental phase:

- differential tests must produce identical logical records, ordering,
  versions, deletes, queues, links, schemas, vectors, and replication state;
- every WAL, flush, manifest, and compaction fault-injection point must recover
  without acknowledged data loss or silent corruption;
- fuzz/property tests must cover key/value encoding, WAL framing, manifests,
  table records, truncation, and recovery decisions;
- 1M-record runs must complete within the documented memory budget and report
  bounded restart/recovery time;
- ThingDB must meet provisional targets of at least 80% of RocksDB throughput,
  no more than 2x RocksDB p99 latency for the defined core workloads, and no
  more than 1.5x total disk usage, or the gap must be explicitly accepted;
- repack, encryption, backup/restore, rollback, and derived-search rebuilds
  must pass with the source database preserved.

These are provisional promotion targets for Phase 3/4, not current claims.
The current ThingDB implementation is expected to miss some of them.

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
exploratory until its large-store and crash-recovery gates pass. Do not compare
this smoke table with results from a different machine or iteration count. The
generated structured output is ignored by Git and should be retained as a
workflow artifact or local evidence.

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
