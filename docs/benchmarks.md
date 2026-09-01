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
  --iterations 1000 --seed 42 --backend all --reliability \
  --output target/storage-benchmark.json
```

Use `--output target/storage-benchmark.csv` for a flat CSV result. The output
includes commit, Rust, operating system, architecture, CPU model, filesystem,
seed, operation counts, throughput, and durable directory size. Sampled
workloads include p50/p95/p99/max latency; aggregate timing rows explicitly
mark latency as unsampled rather than presenting a synthetic percentile. Each
durable backend gets a fresh temporary directory, and correctness/reopen checks
run before the command succeeds. A correctness or recovery error is a failed
run, not a performance datapoint.

Every run is also appended automatically to
`target/storage-benchmark-history.jsonl`. Set `--history` or
`THINGD_BENCH_HISTORY` to retain history elsewhere, and set `--phase` or
`THINGD_BENCH_PHASE` to label a run, for example `reliability-baseline`,
`wal-group-commit`, or `table-indexes`. Each history record contains the date,
branch, commit, environment, selected backend, and all measured RocksDB,
ThingDB, or in-memory workload rows. This makes repeated runs comparable by
phase and date without committing machine-specific results.

The benchmark selects `--backend all|rocksdb|thingdb|memory|cache`; `all` is the
default and is the required comparison mode. It measures object, event, queue,
search, vector search, batch, count, delete, concurrent-read, and
lock-contention operations for the selected adapters. In comparison mode, `all`
includes the reference memory engine, ThingDB RAM mode, durable RocksDB, and
durable ThingDB. It also includes the standalone ThingDB RAM cache as
`thingdb-cache`.

ThingDB RAM runs also record internal pipeline diagnostics in the structured
output: keyspace lookup, lock wait/hold time, value cloning, mutation,
iteration, Thingd-layer deserialization, and search timing. These diagnostics
explain a result but are not themselves performance guarantees. A focused
repeatable smoke run is:

```bash
cargo run --release -p thingd --example storage_bench --features persistent,search -- \
  --iterations 100 --repetitions 5 --seed 42 --backend thingdb \
  --phase thingdb-ram-performance-smoke \
  --output target/thingdb-ram-performance.json \
  --history target/thingdb-ram-performance.jsonl
```

Use larger 50K and 100K runs only after the smoke run passes correctness and
filesystem isolation. Do not compare these results with Redis or treat them as
production claims.

Use `--backend cache` for a focused ThingDB cache run. This measures byte
key/value inserts, hot reads, mixed access, TTL/LRU bounds, latency
percentiles, and four-thread contention. The cache is a separate process-local
RAM primitive; its numbers must not be confused with Thingd semantic object
storage or durable ThingDB. It creates no WAL, table, manifest, or temporary
database files.
Use `--reliability` (or `THINGD_BENCH_RELIABILITY=1`) to run the deterministic
preflight before recording benchmark results. The preflight checks object
updates/deletes, events, queues, links, search cleanup, atomic batches,
concurrent readers/writers, repeated ThingDB RAM instances, and zero journal
usage. A failed preflight exits non-zero; its throughput results must not be
used as a passing performance datapoint.

Use `--repetitions 5` or `THINGD_BENCH_REPETITIONS=5` for phase comparisons.
Structured JSON output includes grouped median, minimum, maximum, and spread
throughput summaries in addition to each repetition's raw result. Resource
metadata reports peak RSS and process CPU time when the host permits `ps`
sampling; otherwise it records an explicit `unsupported: ...` status. This
avoids treating missing host instrumentation as zero usage or a passing scale
qualification.
For exploratory large-record runs whose queue transitions would otherwise
dominate local runtime, use `--queue-iterations <n>` or
`THINGD_BENCH_QUEUE_ITERS=<n>`. The limit is recorded in benchmark metadata and
does not reduce object, event, scan, or maintenance workloads. It is not valid
for a full qualification claim; qualification runs must omit it and exercise
the complete queue workload at the declared dataset size.
The WAL-hardening and group-commit phases additionally record
`wal-single-write`, `wal-explicit-batch`, `wal-concurrent-write`, and
`wal-recovery` rows. The immutable-table and recovery phases additionally
record `table-compaction` and `table-recovery` rows plus ThingDB WAL timing
diagnostics. Single writes remain
sync-before-ack; grouped writes reduce physical sync calls without trading
durability for throughput. Group-commit diagnostics include logical commits,
physical sync calls, average and maximum group size, and queue wait time.
Phase 1C adds table-layer and compaction measurements; compare WAL growth,
compaction duration, table count, total disk usage, restart recovery, and
compacted read correctness against the Phase 1B history. Phase 2 adds
interrupted-maintenance fault tests and compares recovery time and final
logical state after each deterministic fault boundary.
Phase 3 additionally records mutable-table bytes, automatic flush count, total
flush time, and whether the configured mutable-table bound was exceeded. A
bounded-memtable run must verify that acknowledged writes survive reopen and
that an injected post-WAL flush failure blocks further writes until recovery.
Use `--memtable-bytes <bytes>` (or `THINGD_BENCH_MEMTABLE_BYTES`) to make the
bound explicit and reproducible. For example:

```bash
cargo run --release -p thingd --example storage_bench --features persistent,search -- \
  --iterations 100000 --repetitions 5 --seed 42 --backend all \
  --memtable-bytes 8388608 --phase phase-3-bounded-memtables \
  --output target/phase-3-bounded-memtables.json \
  --history target/storage-benchmark-history.jsonl
```

Phase 4 adds table-layer point-read and restart measurements. Compare cold
startup, post-reopen point reads, ordered scans, and memory/disk usage against
the Phase 3 history. A passing result requires identical logical results after
flush, restart, tombstone application, and compaction; it is not a promotion
claim by itself.

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

For controlled durable qualification, add `--qualification` (or set
`THINGD_BENCH_QUALIFICATION=1`). This runs the existing unified benchmark's
deterministic reopen, compaction, cross-backend logical repack, source
preservation, and destination validation checks for RocksDB and ThingDB. The
checks use fresh temporary directories and are recorded in the structured
output under `qualification`; a failure stops the run. This is operational
evidence only and does not make ThingDB production-ready or change the
RocksDB default.

Use `--backend thingdb-memory` for a RAM-only ThingDB run without the durable
WAL workloads. This is useful for isolating process-local queue and object
performance; it does not qualify durable ThingDB.

For a dependency-refresh qualification run, use the same release binary and
dataset settings for every backend:

```bash
cargo run --release -p thingd --example storage_bench --features persistent,search -- \
  --iterations 100000 --repetitions 5 --seed 42 --backend all \
  --reliability --qualification --phase dependency-refresh-100k \
  --output target/dependency-refresh-100k.json \
  --history target/storage-benchmark-history.jsonl
```

This is a full all-backend run: it includes the complete queue workload and
durable reopen, compaction, repack, encryption, and validation checks. Record
the structured output as a local or CI artifact; do not commit machine-specific
results. A timeout, memory limit, failed reliability check, or incomplete
queue workload is blocked evidence, not a passing qualification result.

The current Phase 5 development smoke uses the same command shape at 1,000
iterations and one repetition. Its correctness, reliability, reopen,
compaction, encryption, and both logical repack directions passed locally.
This is a qualification harness check only; the five-repeat 10K and 100K
comparison gates remain pending.

The durable write-path smoke also passes at 100 iterations with the same
correctness and recovery preflight. It confirms that durable ThingDB remains
substantially slower for synchronous sequential writes; the 10K qualification
run is currently blocked by runtime, so no scale or promotion claim is made.

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

These are provisional promotion targets for the combined benchmark and
reliability gate, not current claims. The current ThingDB implementation is
expected to miss some durable targets. ThingDB RAM must first demonstrate
semantic parity, bounded memory behavior, and no filesystem artifacts before
it can be considered for the separate default-adoption phase.

## Benchmark status

The benchmark is intentionally methodology-first. Recent local smoke runs use
the same release binary, fixed seed, fresh directories, and correctness and
recovery preflight for every backend. They show that ThingDB RAM is a viable
filesystem-free process-local path, while durable ThingDB is relatively strong
on some reads and batches but remains substantially slower for synchronous
isolated writes than RocksDB.

These findings are development evidence, not universal throughput claims. The
full five-repeat 10K and 100K qualification gates remain separate and must be
run on the same machine, filesystem, dataset, and build before results can be
used for promotion decisions. Generated structured output is ignored by Git
and should be retained as a workflow artifact or private local evidence.

## Node.js SDK benchmark

```bash
pnpm bench:node
node packages/thingd/bench/node-bench.mjs 20000
```

This exercises the public SDK through ThingDB RAM when the native driver is
available and through the portable in-memory fallback otherwise. Existing
machine-specific historical results may refer to the previous temporary native
path and are not directly comparable to current ThingDB RAM results.

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
