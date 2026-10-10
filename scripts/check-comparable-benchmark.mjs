import { readFileSync } from "node:fs";

const reportPath = process.argv[2];
const expectedIterations = Number(process.env.COMPARABLE_BENCH_ITERATIONS ?? 0);
const expectedRepetitions = Number(process.env.COMPARABLE_BENCH_REPETITIONS ?? 0);
const expectedSeed = Number(process.env.COMPARABLE_BENCH_SEED ?? 42);

if (!reportPath) {
  throw new Error("usage: node scripts/check-comparable-benchmark.mjs <report.json>");
}

const report = JSON.parse(readFileSync(reportPath, "utf8"));
const { metadata, results, raw_comparisons: snapshots } = report;

if (metadata?.backend !== "comparable") {
  throw new Error(`expected comparable benchmark output, got ${metadata?.backend ?? "unknown"}`);
}
if (expectedIterations > 0 && metadata.iterations !== expectedIterations) {
  throw new Error("iteration metadata does not match the requested run");
}
if (expectedRepetitions > 0 && metadata.repetitions !== expectedRepetitions) {
  throw new Error("repetition metadata does not match the requested run");
}
if (metadata.repetitions < 1 || metadata.queue_iterations != null) {
  throw new Error("invalid repetitions or unexpected reduced semantic queue workload");
}
if (metadata.seed !== expectedSeed || metadata.os !== "linux") {
  throw new Error("comparison must use the expected deterministic seed on Linux");
}
if (!metadata.commit || !metadata.branch || !metadata.filesystem || !metadata.cpu_model) {
  throw new Error("commit, branch, filesystem, and CPU metadata are required");
}
if (
  metadata.peak_rss_bytes == null ||
  !metadata.peak_rss_status?.startsWith("measured:") ||
  metadata.cpu_time_ns == null ||
  !metadata.cpu_time_status?.startsWith("measured:")
) {
  throw new Error("process RSS and CPU measurements must be available for comparison evidence");
}

const drivers = [
  "thingdb-ram-kv",
  "sqlite-memory-kv",
  "redis-memory-kv",
  "thingdb-durable-kv",
  "rocksdb-sync-kv",
  "sqlite-wal-full-kv",
  "fjall-syncall-kv",
  "redis-aof-always-kv",
];
const operations = [
  "raw_sequential_sync_put",
  "raw_random_sync_update",
  "raw_point_hit",
  "raw_point_miss",
  "raw_atomic_batch_put",
  "raw_prefix_scan",
  "raw_bounded_range_probe",
  "raw_sync_delete",
  "raw_concurrent_sync_put_4w",
];

if (!Array.isArray(snapshots) || snapshots.length !== drivers.length * metadata.repetitions) {
  throw new Error("raw comparison snapshots are missing or incomplete");
}
if (!Array.isArray(results)) {
  throw new Error("benchmark results are missing");
}

for (const result of results) {
  if (result.error_count !== 0) {
    throw new Error(`${result.driver}/${result.operation} reported errors`);
  }
}

for (let repetition = 0; repetition < metadata.repetitions; repetition += 1) {
  const repeatedSnapshots = snapshots.filter((snapshot) => snapshot.repetition === repetition);
  if (repeatedSnapshots.length !== drivers.length) {
    throw new Error(`repetition ${repetition} has ${repeatedSnapshots.length} drivers`);
  }
  const reference = repeatedSnapshots[0];
  if (!reference.logical_digest || reference.logical_entries < 1) {
    throw new Error(`repetition ${repetition} has no valid logical digest`);
  }

  for (const driver of drivers) {
    const snapshot = repeatedSnapshots.find((item) => item.driver === driver);
    if (!snapshot) {
      throw new Error(`repetition ${repetition} is missing ${driver}`);
    }
    if (
      snapshot.logical_entries !== reference.logical_entries ||
      snapshot.logical_digest !== reference.logical_digest
    ) {
      throw new Error(`${driver} logical digest differs in repetition ${repetition}`);
    }
    if (!snapshot.database_version || !snapshot.durability || !snapshot.client_topology) {
      throw new Error(`${driver} is missing version, durability, or topology metadata`);
    }
    if (!snapshot.resource_status) {
      throw new Error(`${driver} is missing explicit resource measurement status`);
    }
    if (["thingdb-ram-kv", "sqlite-memory-kv"].includes(driver)) {
      if (snapshot.bytes_on_disk !== 0 || snapshot.filesystem_artifacts !== 0) {
        throw new Error(`${driver} RAM mode created filesystem state`);
      }
    }
    if (snapshot.mode === "durable" && !driver.startsWith("redis-") &&
      (snapshot.bytes_on_disk == null || snapshot.filesystem_artifacts == null ||
        snapshot.filesystem_artifacts < 1)) {
      throw new Error(`${driver} is missing durable disk usage/artifact measurements`);
    }
    if (driver === "redis-memory-kv" && !snapshot.redis_used_memory_bytes) {
      throw new Error("Redis RAM server memory measurement is missing");
    }
    if (driver === "redis-aof-always-kv") {
      if (!snapshot.redis_server_version || !snapshot.redis_persistence_bytes) {
        throw new Error("Redis AOF server version or persistence size is missing");
      }
      if (!snapshot.restart_recovery?.startsWith("not-tested:")) {
        throw new Error("Redis server-restart limitation must be reported explicitly");
      }
    } else if (snapshot.mode === "durable" && !snapshot.restart_recovery?.startsWith("passed:")) {
      throw new Error(`${driver} durable restart/digest check did not pass`);
    }

    for (const operation of operations) {
      const matches = results.filter(
        (result) =>
          result.repetition === repetition &&
          result.driver === driver &&
          result.operation === operation,
      );
      if (matches.length !== 1) {
        throw new Error(`${driver}/${operation} has ${matches.length} results for repetition ${repetition}`);
      }
      const [result] = matches;
      if (
        !result.latency_sampled ||
        result.latency_sample_count < 1 ||
        result.p50_ns == null ||
        result.p95_ns == null ||
        result.p99_ns == null ||
        result.max_ns == null
      ) {
        throw new Error(`${driver}/${operation} is missing latency percentiles`);
      }
    }

    if (snapshot.mode === "durable" && driver !== "redis-aof-always-kv") {
      const restart = results.filter(
        (result) =>
          result.repetition === repetition &&
          result.driver === driver &&
          result.operation === "raw_restart_digest_check",
      );
      if (restart.length !== 1 || restart[0].error_count !== 0) {
        throw new Error(`${driver} restart result is missing or failed`);
      }
    }
  }
}

console.log(
  `Validated ${metadata.phase}: ${metadata.iterations} operations x ${metadata.repetitions} repetitions across ${drivers.length} raw key/value modes.`,
);
