import { readFileSync } from "node:fs";

const reportPath = process.argv[2];
const expectedIterations = Number(process.env.DURABLE_BENCH_ITERATIONS ?? 0);
const expectedRepetitions = Number(process.env.DURABLE_BENCH_REPETITIONS ?? 0);

if (!reportPath) {
  throw new Error("usage: node scripts/check-durable-benchmark.mjs <report.json>");
}

const report = JSON.parse(readFileSync(reportPath, "utf8"));
const { metadata, results, comparisons, storage, wal } = report;

if (metadata?.backend !== "durable") {
  throw new Error(`expected a durable report, got ${metadata?.backend ?? "unknown"}`);
}
if (metadata.reliability_preflight !== true) {
  throw new Error("reliability preflight was not enabled");
}
if (metadata.qualification_preflight !== true) {
  throw new Error("qualification preflight was not enabled");
}
if (!metadata.qualification_status?.startsWith("complete:")) {
  throw new Error(`benchmark is not complete: ${metadata.qualification_status ?? "unknown"}`);
}
if (metadata.peak_rss_bytes == null || !metadata.peak_rss_status?.startsWith("measured:")) {
  throw new Error("benchmark did not provide measured RSS");
}
if (metadata.cpu_time_ns == null || !metadata.cpu_time_status?.startsWith("measured:")) {
  throw new Error("benchmark did not provide measured CPU time");
}
if (!["native-sync", "common-fsync"].includes(metadata.durability_profile)) {
  throw new Error(`unexpected durability profile: ${metadata.durability_profile}`);
}
if (expectedIterations > 0 && metadata.iterations !== expectedIterations) {
  throw new Error("iteration metadata does not match the requested run");
}
if (expectedRepetitions > 0 && metadata.repetitions !== expectedRepetitions) {
  throw new Error("repetition metadata does not match the requested run");
}
if (metadata.repetitions < 5 || metadata.queue_iterations != null) {
  throw new Error("reduced repetitions or queue workload cannot qualify");
}

const requiredOperations = [
  "object_put",
  "object_batch",
  "object_get",
  "event_append",
  "event_batch",
  "queue_push",
  "queue_batch",
  "queue_claim_ack",
  "queue_claim_ack2",
  "list_objects",
  "list_objects_limit100",
];
const thingDbOperations = [
  "wal-single-write",
  "wal-explicit-batch",
  "wal-recovery",
  "table-compaction",
  "table-recovery",
];
const requiredDrivers = ["persistent", "thingdb-experimental"];

for (const driver of requiredDrivers) {
  const driverResults = results.filter((result) => result.driver === driver);
  if (driverResults.length === 0) {
    throw new Error(`missing results for ${driver}`);
  }
  const repetitions = new Set(driverResults.map((result) => result.repetition));
  if (repetitions.size !== metadata.repetitions) {
    throw new Error(`${driver} has ${repetitions.size} repetitions, expected ${metadata.repetitions}`);
  }
  const operations = driver === "thingdb-experimental"
    ? [...requiredOperations, ...thingDbOperations]
    : requiredOperations;
  for (const operation of operations) {
    if (!driverResults.some((result) => result.operation === operation)) {
      throw new Error(`${driver} is missing required operation ${operation}`);
    }
  }
}

for (const result of results) {
  if (result.error_count !== 0) {
    throw new Error(`${result.driver}/${result.operation} reported errors`);
  }
}
if (!Array.isArray(comparisons) || comparisons.some((comparison) => !comparison.complete)) {
  throw new Error("durable backend comparisons are incomplete");
}
if (!Array.isArray(storage) || storage.length === 0 || !Array.isArray(wal) || wal.length === 0) {
  throw new Error("durable storage and WAL diagnostics are missing");
}

console.log(
  `Validated ${metadata.phase}: ${metadata.iterations} iterations x ${metadata.repetitions} repetitions, ${metadata.durability_profile}.`,
);
