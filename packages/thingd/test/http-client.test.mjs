import assert from "node:assert/strict";
import { once } from "node:events";
import { createServer, request } from "node:http";
import test from "node:test";
import { runHttpClientErrorContract } from "../../../scripts/http-client-error-contract.mjs";
import { HttpThingStore } from "../dist/client/http-thing-store.js";
import { handleRestRequest, ThingD } from "../dist/index.js";

runHttpClientErrorContract("HttpThingStore", (url) => new HttpThingStore({ url }));

test("HTTP batch reads use POST and preserve the legacy GET contract", async (t) => {
  const db = await ThingD.open({ driver: "memory" });
  const methods = [];
  const server = createServer((req, res) => {
    methods.push(req.method);
    void handleRestRequest(db, req, res, new URL(req.url, "http://localhost").pathname);
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  t.after(async () => {
    await new Promise((resolve, reject) => {
      server.close((error) => error ? reject(error) : resolve());
      server.closeAllConnections();
    });
    await db.close();
  });
  const url = `http://127.0.0.1:${server.address().port}`;
  const client = new HttpThingStore({ url });
  await db.put("notes", { id: "first", text: "First" });
  await db.put("notes", { id: "second", text: "Second" });
  const ids = ["second", "missing", "first", "second"];
  const rows = await client.getBatch("notes", ids);
  assert.equal(methods[0], "POST");
  assert.deepEqual(rows.map((row) => row?.id ?? null), ["second", null, "first", "second"]);
  assert.deepEqual(await client.getBatch("notes", []), []);

  const body = JSON.stringify({ ids });
  const legacy = await new Promise((resolve, reject) => {
    const req = request(`${url}/v1/objects/batch?collection=notes`, {
      method: "GET",
      headers: { "content-type": "application/json", "content-length": Buffer.byteLength(body) },
    }, (res) => {
      let text = "";
      res.setEncoding("utf8");
      res.on("data", (chunk) => { text += chunk; });
      res.on("end", () => resolve({ status: res.statusCode, body: JSON.parse(text) }));
      res.on("error", reject);
    });
    req.on("error", reject);
    req.end(body);
  });
  assert.equal(legacy.status, 200);
  assert.deepEqual(legacy.body.data, rows);
  const oversized = await fetch(`${url}/v1/objects/batch?collection=notes`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(Array.from({ length: 1001 }, () => "first")),
  });
  assert.equal(oversized.status, 400);
});

test("REST batch errors do not expose internal exception details", async (t) => {
  const db = { queue: () => ({ ackMany: async () => { throw new Error("secret stack detail"); } }) };
  const server = createServer((req, res) => {
    void handleRestRequest(db, req, res, new URL(req.url, "http://localhost").pathname);
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  t.after(() => new Promise((resolve) => server.close(resolve)));
  const response = await fetch(`http://127.0.0.1:${server.address().port}/v1/queues/test/ack-batch`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jobIds: ["one"] }),
  });
  assert.equal(response.status, 409);
  assert.deepEqual(await response.json(), { error: { code: "conflict", message: "Queue batch ack failed" } });
});
