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

  test("native: backupTo creates a valid backup file", async () => {
    const directory = await mkdtemp(join(tmpdir(), "thingd-native-"));
    const path = join(directory, "source.db");
    const backupPath = join(directory, "backup.db");

    const db = await ThingD.open({ path, driver: "native" });
    await db.put("test", { id: "backup-obj", text: "backup test" });
    db.backupTo(backupPath);
    assert.ok(existsSync(backupPath));

    // Verify backup is readable
    const restored = await ThingD.open({ path: backupPath, driver: "native" });
    const obj = await restored.get("test", "backup-obj");
    assert.equal(obj?.text, "backup test");
    await restored.close();
    await db.close();
  });

  test("native: walCheckpoint returns frame counts", async () => {
    const db = await ThingD.open({ path: ":memory:", driver: "native" });
    const result = db.walCheckpoint();
    assert.ok(typeof result.framesBefore === "number");
    assert.ok(typeof result.framesAfter === "number");
    await db.close();
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

  test(`${label}: persists lastError in the store after nack`, async () => {
    const db = await openDb();
    const queue = db.queue("embed");

    const pushed = await queue.push({ object: "docs/doc_123" });
    await queue.claim();
    await queue.nack(pushed.id, {
      error: "persistent error message",
    });

    const jobs = await queue.list();
    const nackedJob = jobs.find((j) => j.id === pushed.id);
    assert.ok(nackedJob, "nacked job should appear in list");
    assert.equal(nackedJob?.lastError, "persistent error message");
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

  test(`${label}: counts objects, events, and jobs correctly`, async () => {
    const db = await openDb();

    assert.equal(await db.countObjects(), 0);
    assert.equal(await db.countEvents(), 0);
    assert.equal(await db.countActiveJobs(), 0);
    assert.equal(await db.countDeadJobs(), 0);

    await db.put("col-a", { id: "obj-1" });
    await db.put("col-a", { id: "obj-2" });
    await db.put("col-b", { id: "obj-3" });
    assert.equal(await db.countObjects(), 3);

    await db.events.append("stream-1", { type: "test" });
    await db.events.append("stream-1", { type: "test" });
    await db.events.append("stream-2", { type: "test" });
    assert.equal(await db.countEvents(), 3);

    const q = db.queue("work");
    await q.push({ task: "a" });
    await q.push({ task: "b" });
    await q.push({ task: "c" });
    assert.equal(await db.countActiveJobs(), 3);

    // push a doomed job to a separate queue with maxAttempts=1
    const deadQ = db.queue("dead-letter-test");
    const doomed = await deadQ.push({ task: "d" }, { maxAttempts: 1 });
    await deadQ.claim();
    await deadQ.nack(doomed.id, { error: "fail" });
    assert.equal(await db.countDeadJobs(), 1);
    assert.equal(await db.countActiveJobs(), 3);
  });

  test(`${label}: lists collections, streams, and queues`, async () => {
    const db = await openDb();

    assert.deepEqual(await db.listCollections(), []);
    assert.deepEqual(await db.listStreams(), []);
    assert.deepEqual(await db.listQueues(), []);

    await db.put("col-a", { id: "x" });
    await db.put("col-b", { id: "y" });
    await db.put("col-a", { id: "z" });
    assert.deepEqual(await db.listCollections(), ["col-a", "col-b"]);

    await db.events.append("stream-1", { type: "t" });
    await db.events.append("stream-2", { type: "t" });
    assert.deepEqual(await db.listStreams(), ["stream-1", "stream-2"]);

    await db.queue("work").push({ task: "x" });
    await db.queue("jobs").push({ task: "y" });
    assert.deepEqual(await db.listQueues(), ["jobs", "work"]);
  });

  test(`${label}: searches with filter and limit options`, async () => {
    const db = await openDb();

    await db.put("docs", { id: "a", text: "hello world" });
    await db.put("docs", { id: "b", text: "hello there" });
    await db.put("docs", { id: "c", text: "goodbye world" });

    const all = await db.search("world");
    assert.equal(all.length, 2);

    const limited = await db.search("world", { limit: 1 });
    assert.equal(limited.length, 1);

    const byCollection = await db.search("hello", { collections: ["docs"] });
    assert.equal(byCollection.length, 2);

    const noMatch = await db.search("hello", { collections: ["nonexistent"] });
    assert.equal(noMatch.length, 0);
  });

  test(`${label}: searchObjects returns flat object results`, async () => {
    const db = await openDb();

    await db.put("docs", { id: "a", text: "hello world" });
    await db.put("docs", { id: "b", text: "hello there" });
    await db.events.append("log", { type: "test", text: "hello event" });

    const objects = await db.searchObjects("hello", { collections: ["docs"] });
    assert.equal(objects.length, 2);
    assert.ok(objects.every((o) => o.kind === undefined), "should not have kind wrapper");
    assert.ok(objects.every((o) => typeof o.id === "string"));
    assert.ok(objects.every((o) => o.collection === "docs"));

    const noResults = await db.searchObjects("nonexistent");
    assert.deepEqual(noResults, []);

    await db.close();
  });

  test(`${label}: listObjects with filter, limit, and offset`, async () => {
    const db = await openDb();

    await db.put("widgets", { id: "a", color: "red", size: 1 });
    await db.put("widgets", { id: "b", color: "blue", size: 2 });
    await db.put("widgets", { id: "c", color: "red", size: 3 });
    await db.put("widgets", { id: "d", color: "green", size: 4 });
    await db.put("widgets", { id: "e", color: "red", size: 5 });

    // No options — returns all
    const all = await db.listObjects("widgets");
    assert.equal(all.length, 5);

    // Filter by color
    const reds = await db.listObjects("widgets", { filter: { color: "red" } });
    assert.equal(reds.length, 3);
    assert.ok(reds.every((o) => o.color === "red"));

    // Limit
    const limited = await db.listObjects("widgets", { limit: 2 });
    assert.equal(limited.length, 2);

    // Offset
    const offset = await db.listObjects("widgets", { offset: 3 });
    assert.equal(offset.length, 2);

    // Filter + limit
    const filtered = await db.listObjects("widgets", { filter: { color: "red" }, limit: 1 });
    assert.equal(filtered.length, 1);

    // Empty collection
    const empty = await db.listObjects("nonexistent");
    assert.deepEqual(empty, []);

    await db.close();
  });

  test(`${label}: returns errors for invalid ack/nack operations`, async () => {
    const db = await openDb();
    const q = db.queue("test");

    // ack/nack non-existent job
    const missingAck = await q.ack("no-such-job");
    assert.equal(missingAck.ok, false);
    assert.equal(missingAck.ok ? null : missingAck.reason, "not_found");

    const missingNack = await q.nack("no-such-job");
    assert.equal(missingNack.ok, false);
    assert.equal(missingNack.ok ? null : missingNack.reason, "not_found");

    // ack/nack a completed job
    const pushed = await q.push({ task: "x" });
    await q.claim();
    await q.ack(pushed.id);

    const doubleAck = await q.ack(pushed.id);
    assert.equal(doubleAck.ok, false);
    assert.equal(doubleAck.ok ? null : doubleAck.reason, "terminal");

    const ackAfterComplete = await q.nack(pushed.id);
    assert.equal(ackAfterComplete.ok, false);
    assert.equal(ackAfterComplete.ok ? null : ackAfterComplete.reason, "terminal");
  });

  test(`${label}: ThingD facade convenience accessors`, async () => {
    const db = await openDb();

    // events.append / events.list via facade
    await db.events.append("facade-stream", { type: "test", text: "via events" });
    const evts = await db.events.list("facade-stream");
    assert.equal(evts.length, 1);
    assert.equal(evts[0].type, "test");

    // queue() returns a MemoryQueue
    const mq = db.queue("facade-queue");
    const pushed = await mq.push({ task: "via queue" });
    const claimed = await mq.claim();
    assert.equal(claimed?.id, pushed.id);

    // listObjects via facade
    await db.put("facade-col", { id: "obj-1" });
    const objects = await db.listObjects("facade-col");
    assert.equal(objects.length, 1);

    // close does not throw
    await db.close();
  });

  test(`${label}: creates, gets, and deletes graph links`, async () => {
    const db = await openDb();

    const link = await db.links.create("users/alice", "authored", "docs/readme");
    assert.ok(link.id);
    assert.equal(link.fromRef, "users/alice");
    assert.equal(link.linkType, "authored");
    assert.equal(link.toRef, "docs/readme");
    assert.ok(link.createdAt);

    const fetched = await db.links.get(link.id);
    assert.deepEqual(fetched, link);

    const deleted = await db.links.delete(link.id);
    assert.equal(deleted, true);

    const gone = await db.links.get(link.id);
    assert.equal(gone, null);

    await db.close();
  });

  test(`${label}: gets link neighbors by direction`, async () => {
    const db = await openDb();

    await db.links.create("users/alice", "authored", "docs/readme");
    await db.links.create("users/alice", "authored", "docs/api");
    await db.links.create("docs/readme", "references", "docs/api");

    const outgoing = await db.links.neighbors("users/alice", "Outgoing");
    assert.equal(outgoing.length, 2);
    assert.ok(outgoing.every((l) => l.fromRef === "users/alice"));

    const incoming = await db.links.neighbors("docs/api", "Incoming");
    assert.equal(incoming.length, 2);
    assert.ok(incoming.every((l) => l.toRef === "docs/api"));

    const both = await db.links.neighbors("docs/readme", "Both");
    assert.equal(both.length, 2);

    const authored = await db.links.neighbors("users/alice", "Outgoing", { linkType: "authored" });
    assert.equal(authored.length, 2);

    const refs = await db.links.neighbors("users/alice", "Outgoing", { linkType: "references" });
    assert.equal(refs.length, 0);

    const limited = await db.links.neighbors("users/alice", "Outgoing", { limit: 1 });
    assert.equal(limited.length, 1);

    await db.close();
  });

  test(`${label}: counts graph links`, async () => {
    const db = await openDb();

    assert.equal(await db.countLinks(), 0);

    await db.links.create("a", "rel", "b");
    assert.equal(await db.countLinks(), 1);

    await db.links.create("b", "rel", "c");
    assert.equal(await db.countLinks(), 2);

    await db.links.delete((await db.links.neighbors("a", "Outgoing"))[0].id);
    assert.equal(await db.countLinks(), 1);

    await db.close();
  });

  test(`${label}: putBatch creates multiple objects`, async () => {
    const db = await openDb();

    const results = await db.putBatch("widgets", [
      { id: "w1", color: "red" },
      { id: "w2", color: "blue" },
      { id: "w3", color: "green" },
    ]);

    assert.equal(results.length, 3);
    assert.equal(results[0].collection, "widgets");
    assert.equal(results[0].version, 1);

    const all = await db.listObjects("widgets");
    assert.equal(all.length, 3);

    await db.close();
  });

  test(`${label}: deleteBatch removes multiple objects`, async () => {
    const db = await openDb();

    await db.put("items", { id: "a", x: 1 });
    await db.put("items", { id: "b", x: 2 });
    await db.put("items", { id: "c", x: 3 });

    const deleted = await db.deleteBatch("items", ["a", "c"]);
    assert.equal(deleted, 2);

    const remaining = await db.listObjects("items");
    assert.equal(remaining.length, 1);
    assert.equal(remaining[0].id, "b");

    await db.close();
  });

  test(`${label}: listObjects with filter`, async () => {
    const db = await openDb();

    await db.put("things", { id: "t1", status: "active" });
    await db.put("things", { id: "t2", status: "inactive" });
    await db.put("things", { id: "t3", status: "active" });

    const active = await db.listObjects("things", { filter: { status: "active" } });
    assert.equal(active.length, 2);
    assert.ok(active.every((o) => o.status === "active"));

    const inactive = await db.listObjects("things", { filter: { status: "inactive" } });
    assert.equal(inactive.length, 1);
    assert.equal(inactive[0].id, "t2");

    await db.close();
  });

  test(`${label}: listObjects with sortBy`, async () => {
    const db = await openDb();

    await db.put("sorted", { id: "c", val: 3 });
    await db.put("sorted", { id: "a", val: 1 });
    await db.put("sorted", { id: "b", val: 2 });

    const asc = await db.listObjects("sorted", { sortBy: { field: "id", direction: "asc" } });
    assert.deepEqual(asc.map((o) => o.id), ["a", "b", "c"]);

    const desc = await db.listObjects("sorted", { sortBy: { field: "id", direction: "desc" } });
    assert.deepEqual(desc.map((o) => o.id), ["c", "b", "a"]);

    await db.close();
  });

  test(`${label}: listObjects with limit and offset`, async () => {
    const db = await openDb();

    for (let i = 0; i < 10; i++) {
      await db.put("paged", { id: `item-${i}`, idx: i });
    }

    const page1 = await db.listObjects("paged", { limit: 3, offset: 0 });
    assert.equal(page1.length, 3);

    const page2 = await db.listObjects("paged", { limit: 3, offset: 3 });
    assert.equal(page2.length, 3);
    assert.notEqual(page1[0].id, page2[0].id);

    const lastPage = await db.listObjects("paged", { limit: 3, offset: 9 });
    assert.equal(lastPage.length, 1);

    await db.close();
  });
}
