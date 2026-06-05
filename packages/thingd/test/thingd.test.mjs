import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { ThingD } from "../dist/index.js";

const nativeBinaryPath = fileURLToPath(
  new URL("../../thingd-native/dist/thingd_native.node", import.meta.url),
);
const nativeAvailable = existsSync(nativeBinaryPath);

runThingDBehaviorSuite("memory", () => ThingD.open(":memory:"));

if (nativeAvailable) {
  runThingDBehaviorSuite("native", () =>
    ThingD.open({
      path: ":memory:",
      driver: "native",
    }),
  );

  test("native: persists objects across reopen", async () => {
    const directory = await mkdtemp(join(tmpdir(), "thingd-native-"));
    const path = join(directory, "thingd.db");

    const first = await ThingD.open({
      path,
      driver: "native",
    });
    await first.put("decisions", {
      id: "native-persistence",
      text: "Native thingd writes to SQLite.",
    });

    const second = await ThingD.open({
      path,
      driver: "native",
    });
    const stored = await second.get("decisions", "native-persistence");

    assert.equal(stored?.text, "Native thingd writes to SQLite.");
    assert.equal(stored?.version, 1);
  });
} else {
  test("native driver behavior suite", { skip: "native binary has not been built yet" }, () => {});
}

function runThingDBehaviorSuite(label, openDb) {
  test(`${label}: stores, reads, updates, and deletes objects`, async () => {
    const db = await openDb();

    const created = await db.put("decisions", {
      id: "rust-core",
      text: "Use Rust for the core engine.",
    });

    assert.equal(created.collection, "decisions");
    assert.equal(created.version, 1);

    const updated = await db.put("decisions", {
      id: "rust-core",
      text: "Use Rust for the core engine and TypeScript for the SDK.",
    });

    assert.equal(updated.version, 2);
    assert.equal((await db.get("decisions", "rust-core"))?.text, updated.text);
    assert.deepEqual(await db.delete("decisions", "rust-core"), { deleted: true });
    assert.equal(await db.get("decisions", "rust-core"), null);
  });

  test(`${label}: appends and lists events by stream`, async () => {
    const db = await openDb();

    await db.events.append("project:thingd", {
      type: "decision.made",
      text: "thingd should be MCP-native.",
    });

    await db.events.append("customer:cus_123", {
      type: "plan.changed",
      text: "Customer upgraded to pro.",
    });

    const projectEvents = await db.events.list("project:thingd");

    assert.equal(projectEvents.length, 1);
    assert.equal(projectEvents[0].type, "decision.made");
  });

  test(`${label}: queues jobs with idempotency and FIFO claiming`, async () => {
    const db = await openDb();
    const queue = db.queue("embed");

    const first = await queue.push(
      { object: "docs/doc_123" },
      {
        idempotencyKey: "embed:docs/doc_123:v1",
      },
    );

    const duplicate = await queue.push(
      { object: "docs/doc_123" },
      {
        idempotencyKey: "embed:docs/doc_123:v1",
      },
    );

    const second = await queue.push({ object: "docs/doc_456" });
    const claimedFirst = await queue.claim();
    const claimedSecond = await queue.claim();

    assert.equal(first.id, duplicate.id);
    assert.equal((await queue.list()).length, 2);
    assert.equal(claimedFirst?.id, first.id);
    assert.equal(claimedFirst?.status, "leased");
    assert.equal(claimedSecond?.id, second.id);
  });

  test(`${label}: acks leased jobs as completed`, async () => {
    const db = await openDb();
    const queue = db.queue("embed");

    const pushed = await queue.push({ object: "docs/doc_123" });
    const claimed = await queue.claim();
    const acked = await queue.ack(pushed.id);

    assert.equal(claimed?.id, pushed.id);
    assert.equal(acked.ok, true);
    assert.equal(acked.ok ? acked.job.status : null, "completed");
    assert.equal(await queue.claim(), null);
  });

  test(`${label}: nacks leased jobs back to ready with optional delay`, async () => {
    const db = await openDb();
    const queue = db.queue("embed");

    const pushed = await queue.push({ object: "docs/doc_123" });
    const claimed = await queue.claim();
    const nacked = await queue.nack(pushed.id, {
      error: "temporary embedding failure",
    });
    const reclaimed = await queue.claim();

    assert.equal(claimed?.id, pushed.id);
    assert.equal(nacked.ok, true);
    assert.equal(nacked.ok ? nacked.job.status : null, "ready");
    assert.equal(nacked.ok ? nacked.job.lastError : null, "temporary embedding failure");
    assert.equal(reclaimed?.id, pushed.id);
    assert.equal(reclaimed?.attempts, 2);
  });

  test(`${label}: does not claim delayed jobs before they are available`, async () => {
    const db = await openDb();
    const queue = db.queue("embed");

    await queue.push(
      { object: "docs/doc_123" },
      {
        delayMs: 60_000,
      },
    );

    assert.equal(await queue.claim(), null);
  });

  test(`${label}: moves jobs to the dead-letter list after max attempts`, async () => {
    const db = await openDb();
    const queue = db.queue("embed");

    const pushed = await queue.push(
      { object: "docs/doc_123" },
      {
        maxAttempts: 1,
      },
    );

    await queue.claim();
    const nacked = await queue.nack(pushed.id, {
      error: "permanent failure",
    });
    const dead = await queue.dead();

    assert.equal(nacked.ok, true);
    assert.equal(nacked.ok ? nacked.job.status : null, "dead");
    assert.equal(dead.length, 1);
    assert.equal(dead[0].id, pushed.id);
    assert.equal(await queue.claim(), null);
  });

  test(`${label}: reclaims jobs after lease expiration`, async () => {
    const db = await openDb();
    const queue = db.queue("embed");

    const pushed = await queue.push({ object: "docs/doc_123" });
    const firstClaim = await queue.claim({
      leaseMs: 0,
    });
    const secondClaim = await queue.claim();

    assert.equal(firstClaim?.id, pushed.id);
    assert.equal(secondClaim?.id, pushed.id);
    assert.equal(secondClaim?.attempts, 2);
  });

  test(`${label}: searches objects and events`, async () => {
    const db = await openDb();

    await db.put("decisions", {
      id: "rust-core",
      text: "Use Rust for the core engine.",
    });

    await db.events.append("project:thingd", {
      type: "decision.made",
      text: "The SDK should feel object-shaped.",
    });

    const objectHits = await db.search("rust", {
      collections: ["decisions"],
    });

    const eventHits = await db.search("object-shaped");

    assert.equal(objectHits[0].kind, "object");
    assert.equal(eventHits[0].kind, "event");
  });

  test(`${label}: preserves createdAt and updatedAt timestamps`, async () => {
    const db = await openDb();

    const created = await db.put("decisions", {
      id: "timestamp-test",
      text: "Check timestamp preservation.",
    });

    assert.ok(created.createdAt, "createdAt should be set");
    assert.ok(created.updatedAt, "updatedAt should be set");
    assert.equal(created.createdAt, created.updatedAt, "createdAt and updatedAt should match on creation");

    // Read back and verify timestamps persist
    const stored = await db.get("decisions", "timestamp-test");
    assert.ok(stored, "object should exist");
    assert.equal(stored.createdAt, created.createdAt, "createdAt should persist across read");
    assert.equal(stored.updatedAt, created.updatedAt, "updatedAt should persist across read");

    // Update the object and verify updatedAt changes
    const updated = await db.put("decisions", {
      id: "timestamp-test",
      text: "Updated content.",
    });

    assert.equal(updated.createdAt, created.createdAt, "createdAt should not change on update");
    assert.ok(new Date(updated.updatedAt) >= new Date(created.updatedAt), "updatedAt should not go backwards");

    // Events
    const event = await db.events.append("timestamp-test-stream", {
      type: "test.event",
      text: "Check event timestamp.",
    });

    assert.ok(event.createdAt, "event createdAt should be set");
  });
}
