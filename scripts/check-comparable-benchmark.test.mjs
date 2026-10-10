import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const scriptPath = join(dirname(fileURLToPath(import.meta.url)), "check-comparable-benchmark.mjs");

function validReport() {
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
  const raw_comparisons = drivers.map((driver) => ({
    driver,
    mode: ["thingdb-ram-kv", "sqlite-memory-kv", "redis-memory-kv"].includes(driver)
      ? "ram"
      : "durable",
    repetition: 0,
    database_version: `${driver} test version`,
    durability: "test policy",
    client_topology: "test topology",
    restart_recovery: driver === "redis-aof-always-kv"
      ? "not-tested: server restart is external"
      : driver.includes("ram") || driver === "sqlite-memory-kv"
        ? "not-applicable: RAM-only mode"
        : "passed: digest matches",
    logical_entries: 10,
    logical_digest: "fnv1a64:0123456789abcdef",
    bytes_on_disk: driver.includes("redis") ? null : 1024,
    filesystem_artifacts: driver.includes("redis") ? null : 3,
    resource_status: "measured by test fixture",
    redis_server_version: driver.includes("redis") ? "8.test" : null,
    redis_used_memory_bytes: driver === "redis-memory-kv" ? 2048 : null,
    redis_persistence_bytes: driver === "redis-aof-always-kv" ? 4096 : null,
  }));

  for (const snapshot of raw_comparisons) {
    if (snapshot.mode === "ram") {
      snapshot.bytes_on_disk = 0;
      snapshot.filesystem_artifacts = 0;
    }
  }

  const results = drivers.flatMap((driver) => {
    const reportable = operations.map((operation) => ({
      driver,
      operation,
      repetition: 0,
      error_count: 0,
      latency_sampled: true,
      latency_sample_count: 1,
      p50_ns: 1,
      p95_ns: 1,
      p99_ns: 1,
      max_ns: 1,
    }));
    if (
      driver.endsWith("durable-kv") ||
      ["rocksdb-sync-kv", "sqlite-wal-full-kv", "fjall-syncall-kv"].includes(driver)
    ) {
      reportable.push({
        driver,
        operation: "raw_restart_digest_check",
        repetition: 0,
        error_count: 0,
      });
    }
    return reportable;
  });

  return {
    metadata: {
      backend: "comparable",
      iterations: 1000,
      repetitions: 1,
      seed: 42,
      os: "linux",
      branch: "development",
      commit: "0123456789abcdef",
      filesystem: "ext4",
      cpu_model: "test-cpu",
      peak_rss_bytes: 1024,
      peak_rss_status: "measured: test",
      cpu_time_ns: 1024,
      cpu_time_status: "measured: test",
      queue_iterations: null,
      phase: "comparable-test",
    },
    results,
    raw_comparisons,
  };
}

function runChecker(report) {
  const directory = mkdtempSync(join(tmpdir(), "thingd-comparable-check-"));
  const path = join(directory, "report.json");
  writeFileSync(path, JSON.stringify(report));
  const result = spawnSync(process.execPath, [scriptPath, path], { encoding: "utf8" });
  rmSync(directory, { recursive: true, force: true });
  return result;
}

test("accepts a complete matching Linux comparison report", () => {
  const result = runChecker(validReport());
  assert.equal(result.status, 0, result.stderr);
});

test("rejects a cross-engine logical digest mismatch", () => {
  const report = validReport();
  report.raw_comparisons[1].logical_digest = "fnv1a64:ffffffffffffffff";
  const result = runChecker(report);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /logical digest differs/);
});

test("rejects a missing repetition result", () => {
  const report = validReport();
  report.results = report.results.filter((result) => result.operation !== "raw_point_hit");
  const result = runChecker(report);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /has 0 results/);
});
