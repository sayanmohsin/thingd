import { existsSync } from "node:fs";
import { performance } from "node:perf_hooks";
import { fileURLToPath } from "node:url";
import { ThingD } from "../dist/index.js";

const DEFAULT_ITERATIONS = 5_000;
const COLLECTION = "bench_objects";
const STREAM = "bench:events";
const QUEUE = "bench_queue";
const OBJECT_BODY = { text: "benchmark object", project: "thingd", confidence: 0.95 };
const EVENT_BODY = { text: "benchmark event", project: "thingd", actor: "benchmark" };

const iterations = parseInt(process.argv[2] || process.env.THINGD_BENCH_ITERS || DEFAULT_ITERATIONS, 10);

const nativeBinaryPath = fileURLToPath(
  new URL("../../thingd-native/dist/thingd_native.node", import.meta.url),
);
const nativeAvailable = existsSync(nativeBinaryPath);

async function benchDriver(name, openFn) {
  const store = await openFn();
  const results = [];

  results.push(await timeAsync(name, "object_put", iterations, async () => {
    for (let i = 0; i < iterations; i++) {
      await store.put(COLLECTION, { ...OBJECT_BODY, id: `object-${i}` });
    }
  }));

  results.push(await timeAsync(name, "object_get", iterations, async () => {
    for (let i = 0; i < iterations; i++) {
      await store.get(COLLECTION, `object-${i}`);
    }
  }));

  results.push(await timeAsync(name, "event_append", iterations, async () => {
    for (let i = 0; i < iterations; i++) {
      await store.events.append(STREAM, { ...EVENT_BODY, id: `event-${i}` });
    }
  }));

  results.push(await timeAsync(name, "event_list", iterations, async () => {
    for (let i = 0; i < iterations; i++) {
      await store.events.list(STREAM, { limit: 100 });
    }
  }));

  results.push(await timeAsync(name, "queue_push", iterations, async () => {
    for (let i = 0; i < iterations; i++) {
      await store.queue(QUEUE).push({ payload: `payload-${i}` });
    }
  }));

  results.push(await timeAsync(name, "queue_claim", iterations, async () => {
    for (let i = 0; i < iterations; i++) {
      const job = await store.queue(QUEUE).claim();
      if (job) {
        await store.queue(QUEUE).ack(job.id);
      }
    }
  }));

  await store.close();
  return results;
}

async function timeAsync(store, operation, count, fn) {
  const start = performance.now();
  await fn();
  const elapsed = performance.now() - start;
  return { store, operation, count, elapsed, opsPerSec: Math.round((count / elapsed) * 1000) };
}

function fmtDuration(ms) {
  if (ms < 1) return `${Math.round(ms * 1000)}µs`;
  if (ms < 1000) return `${ms.toFixed(2)}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

function fmtOps(n) {
  return n.toLocaleString("en-US");
}

async function main() {
  console.log("thingd node benchmark");
  console.log(`iterations: ${iterations}`);
  console.log();

  const all = [];

  all.push(...await benchDriver("memory", () => ThingD.open(":memory:")));

  if (nativeAvailable) {
    all.push(...await benchDriver("native", () =>
      ThingD.open({ path: ":memory:", driver: "native" }),
    ));
  }

  console.log(
    `${"store".padStart(10)} | ${"operation".padEnd(15)} | ${"ops".padStart(7)} | ${"elapsed".padStart(12)} | ${"ops/s".padStart(10)}`,
  );
  console.log("-".repeat(70));
  for (const r of all) {
    console.log(
      `${r.store.padStart(10)} | ${r.operation.padEnd(15)} | ${String(r.count).padStart(7)} | ${fmtDuration(r.elapsed).padStart(12)} | ${fmtOps(r.opsPerSec).padStart(10)}`,
    );
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
