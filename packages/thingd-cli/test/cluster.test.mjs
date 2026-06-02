import assert from "node:assert/strict";
import { existsSync, rmSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { ThingD } from "thingd";
import { startThingdHttpServer } from "../dist/mcp/index.js";

function getDbPath(id) {
  return resolve(`test-cluster-follower-${id}.db`);
}

function cleanupDb(dbPath) {
  for (const file of [dbPath, `${dbPath}-wal`, `${dbPath}-shm`]) {
    if (existsSync(file)) {
      try {
        rmSync(file, { force: true });
      } catch {
        // Ignore cleanup errors
      }
    }
  }
}

async function callJsonTool(client, name, args) {
  const result = await client.callTool({
    name,
    arguments: args,
  });

  const text = result.content.find((part) => part.type === "text")?.text;
  assert.equal(typeof text, "string");

  return JSON.parse(text);
}

test("cluster replication sync replicates leader writes to follower", async () => {
  const dbPath = getDbPath("1");
  cleanupDb(dbPath);

  const leader = await startThingdHttpServer({
    path: ":memory:",
    driver: "native",
    port: 0,
    cluster: {
      mode: "leader",
    },
  });

  const follower = await startThingdHttpServer({
    path: dbPath,
    driver: "native",
    port: 0,
    cluster: {
      mode: "follower",
      leaderUrl: leader.url,
    },
  });

  const client = new Client({
    name: "sync-test-client",
    version: "0.1.0",
  });
  const transport = new StreamableHTTPClientTransport(new URL(leader.mcpUrl));
  await client.connect(transport);

  const put = await callJsonTool(client, "thing_put", {
    collection: "items",
    object: {
      id: "sync-obj",
      text: "replicated!",
    },
  });
  assert.equal(put.id, "sync-obj");

  // Wait for follower runner to pull
  await new Promise((resolve) => setTimeout(resolve, 2500));

  await client.close();
  await follower.close();
  await leader.close();

  // Inspect follower db file
  const followerDb = await ThingD.open({
    path: dbPath,
    driver: "native",
  });
  const obj = await followerDb.get("items", "sync-obj");
  assert.ok(obj);
  assert.equal(obj.text, "replicated!");

  await followerDb.close();
  cleanupDb(dbPath);
});

test("cluster forwarding routes follower writes to leader and replicates back", async () => {
  const dbPath = getDbPath("2");
  cleanupDb(dbPath);

  const leader = await startThingdHttpServer({
    path: ":memory:",
    driver: "native",
    port: 0,
    cluster: {
      mode: "leader",
    },
  });

  const follower = await startThingdHttpServer({
    path: dbPath,
    driver: "native",
    port: 0,
    cluster: {
      mode: "follower",
      leaderUrl: leader.url,
    },
  });

  const client = new Client({
    name: "forward-test-client",
    version: "0.1.0",
  });
  const transport = new StreamableHTTPClientTransport(new URL(follower.mcpUrl));
  await client.connect(transport);

  const put = await callJsonTool(client, "thing_put", {
    collection: "items",
    object: {
      id: "forward-obj",
      text: "routed!",
    },
  });
  assert.equal(put.id, "forward-obj");

  // Wait for follower runner to pull
  await new Promise((resolve) => setTimeout(resolve, 2500));

  await client.close();
  await follower.close();
  await leader.close();

  // Inspect follower db file
  const followerDb = await ThingD.open({
    path: dbPath,
    driver: "native",
  });
  const obj = await followerDb.get("items", "forward-obj");
  assert.ok(obj);
  assert.equal(obj.text, "routed!");

  await followerDb.close();
  cleanupDb(dbPath);
});

test("cluster status returns replication details and computed lag", async () => {
  const dbPath = getDbPath("3");
  cleanupDb(dbPath);

  const leader = await startThingdHttpServer({
    path: ":memory:",
    driver: "native",
    port: 0,
    cluster: {
      mode: "leader",
      advertiseUrl: "http://127.0.0.1:8757",
      peers: ["http://127.0.0.1:8757", "http://127.0.0.1:8758"],
      discovery: "static",
    },
  });

  const follower = await startThingdHttpServer({
    path: dbPath,
    driver: "native",
    port: 0,
    cluster: {
      mode: "follower",
      leaderUrl: leader.url,
    },
  });

  // Write an object to leader via direct client
  const client = new Client({
    name: "status-test-client",
    version: "0.1.0",
  });
  const transport = new StreamableHTTPClientTransport(new URL(leader.mcpUrl));
  await client.connect(transport);

  await callJsonTool(client, "thing_put", {
    collection: "items",
    object: {
      id: "status-obj",
      text: "metrics",
    },
  });

  // Wait for replication to sync up
  await new Promise((resolve) => setTimeout(resolve, 2500));

  // Query follower cluster status
  const followerStatusRes = await fetch(`${follower.url}/cluster/status`);
  const followerStatus = await followerStatusRes.json();

  assert.equal(followerStatusRes.status, 200);
  assert.equal(followerStatus.mode, "follower");
  assert.equal(followerStatus.writable, false);
  assert.equal(followerStatus.forwarding, true);
  assert.ok(followerStatus.replication);
  assert.equal(followerStatus.replication.status, "syncing");
  assert.ok(followerStatus.replication.lastReplicatedSequence > 0);
  assert.equal(followerStatus.replication.lag, 0);

  // Query leader cluster status
  const leaderStatusRes = await fetch(`${leader.url}/cluster/status`);
  const leaderStatus = await leaderStatusRes.json();

  assert.equal(leaderStatusRes.status, 200);
  assert.equal(leaderStatus.mode, "leader");
  assert.equal(leaderStatus.writable, true);
  assert.ok(leaderStatus.replication);
  assert.equal(leaderStatus.replication.status, "active");
  assert.ok(leaderStatus.replication.lastReplicatedSequence > 0);

  await client.close();
  await follower.close();
  await leader.close();
  cleanupDb(dbPath);
});
