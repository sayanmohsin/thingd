import assert from "node:assert/strict";
import test from "node:test";
import { MemoryD } from "../dist/index.js";

test("stores, reads, updates, and deletes objects", async () => {
  const db = await MemoryD.open(":memory:");

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

test("appends and lists events by stream", async () => {
  const db = await MemoryD.open(":memory:");

  await db.events.append("project:memoryd", {
    type: "decision.made",
    text: "memoryd should be MCP-native.",
  });

  await db.events.append("customer:cus_123", {
    type: "plan.changed",
    text: "Customer upgraded to pro.",
  });

  const projectEvents = await db.events.list("project:memoryd");

  assert.equal(projectEvents.length, 1);
  assert.equal(projectEvents[0].type, "decision.made");
});

test("queues jobs with idempotency and FIFO claiming", async () => {
  const db = await MemoryD.open(":memory:");
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

test("searches objects and events", async () => {
  const db = await MemoryD.open(":memory:");

  await db.put("decisions", {
    id: "rust-core",
    text: "Use Rust for the core engine.",
  });

  await db.events.append("project:memoryd", {
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
