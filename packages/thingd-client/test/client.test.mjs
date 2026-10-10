import assert from "node:assert/strict";
import test from "node:test";
import { ThingdClient } from "../dist/client.js";

test("queue batch methods call the atomic REST endpoints", async (t) => {
  const requests = [];
  const originalFetch = globalThis.fetch;
  t.after(() => {
    globalThis.fetch = originalFetch;
  });
  globalThis.fetch = async (url, init) => {
    requests.push({ url: String(url), init });
    return new Response(JSON.stringify({ data: [] }), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  };

  const client = new ThingdClient({ url: "https://thingd.example" });
  await client.queue("task queue").claimMany(64, { leaseMs: 5000 });
  await client.queue("task queue").ackMany(["job-1", "job-2"]);

  assert.equal(requests[0]?.url, "https://thingd.example/v1/queues/task%20queue/claim-batch");
  assert.deepEqual(JSON.parse(requests[0]?.init.body), { leaseMs: 5000, limit: 64 });
  assert.equal(requests[1]?.url, "https://thingd.example/v1/queues/task%20queue/ack-batch");
  assert.deepEqual(JSON.parse(requests[1]?.init.body), { jobIds: ["job-1", "job-2"] });
});

test("queue batch methods reject invalid limits and duplicate ack IDs", async () => {
  const client = new ThingdClient({ url: "https://thingd.example" });
  await assert.rejects(client.claimJobs("tasks", 1_001), /between 0 and 1000/);
  await assert.rejects(client.ackJobs("tasks", ["job-1", "job-1"]), /unique job IDs/);
});
