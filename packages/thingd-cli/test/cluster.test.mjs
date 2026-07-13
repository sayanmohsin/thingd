import assert from "node:assert/strict";
import { existsSync, rmSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { ThingD } from "@thingd/sdk";
import { startThingdHttpServer } from "../dist/mcp/index.js";

// Speed up replication for tests: poll every 50ms (default is 500ms).
process.env.THINGD_CLUSTER_REPLICATION_INTERVAL_MS = "50";

/**
 * Adaptive retry: poll `pollFn()` every `intervalMs` until it returns true
 * or `timeoutMs` elapses. Throws on timeout.
 */
async function waitFor(pollFn, { timeoutMs = 8_000, intervalMs = 80 } = {}) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const ok = await pollFn();
    if (ok) return;
    await new Promise((r) => setTimeout(r, intervalMs));
  }
  throw new Error(
    `waitFor timed out after ${timeoutMs}ms`
  );
}

/** Pick an ephemeral port that should be free-ish for testing. */
let _nextPort = 18_757;

function nextPort() {
  return _nextPort++;
}

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

/**
 * Start a server and return { runtime, port, url } with a fixed port.
 */
async function startFixed(httpOptions) {
  const runtime = await startThingdHttpServer({
    ...httpOptions,
    port: httpOptions.port ?? nextPort(),
    host: httpOptions.host ?? "127.0.0.1",
  });
  return runtime;
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

  // Wait for follower to replicate the object
  await waitFor(async () => {
    const db = await ThingD.open({ path: dbPath, driver: "native" }).catch(
      () => null
    );
    if (!db) return false;
    const obj = await db.get("items", "sync-obj").catch(() => null);
    await db.close();
    return obj !== null;
  });

  await client.close();
  await follower.close();
  await leader.close();

  // Verify via follower db
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

  // Wait for follower to replicate the object back from leader
  await waitFor(async () => {
    const db = await ThingD.open({ path: dbPath, driver: "native" }).catch(
      () => null
    );
    if (!db) return false;
    const obj = await db.get("items", "forward-obj").catch(() => null);
    await db.close();
    return obj !== null;
  });

  await client.close();
  await follower.close();
  await leader.close();

  // Verify via follower db
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

  // Wait for replication to sync (poll status endpoint)
  await waitFor(async () => {
    try {
      const res = await fetch(`${follower.url}/cluster/status`);
      const data = await res.json();
      return (
        data.replication?.lastReplicatedSequence > 0 &&
        data.replication?.status === "syncing" &&
        data.replication?.lag === 0
      );
    } catch {
      return false;
    }
  });

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

// ── Phase 8: Leader failover ─────────────────────────────────────────────────

test("leader failover promotes next peer when leader is unreachable", async () => {
  const dbPath = getDbPath("fo-1");
  cleanupDb(dbPath);

  const port1 = nextPort();
  const port2 = nextPort();
  const url1 = `http://127.0.0.1:${port1}`;
  const url2 = `http://127.0.0.1:${port2}`;

  const leader = await startThingdHttpServer({
    path: ":memory:",
    driver: "native",
    port: port1,
    cluster: {
      mode: "leader",
      advertiseUrl: url1,
      peers: [url1, url2],
      discovery: "static",
    },
  });

  const follower = await startThingdHttpServer({
    path: dbPath,
    driver: "native",
    port: port2,
    cluster: {
      mode: "follower",
      advertiseUrl: url2,
      peers: [url1, url2],
      discovery: "static",
      leaderElection: true,
      electionMaxFailures: 2,
    },
  });

  // Write an object via leader and wait for replication.
  const client = new Client({
    name: "failover-test-client",
    version: "0.1.0",
  });
  const transport = new StreamableHTTPClientTransport(new URL(leader.mcpUrl));
  await client.connect(transport);

  await callJsonTool(client, "thing_put", {
    collection: "items",
    object: { id: "pre-failover", text: "before" },
  });

  // Wait for replication to sync (poll status)
  await waitFor(async () => {
    try {
      const res = await fetch(`${follower.url}/cluster/status`);
      const data = await res.json();
      return data.replication?.lastReplicatedSequence > 0;
    } catch {
      return false;
    }
  });

  // Kill the leader.
  await leader.close();
  await client.close();

  // Wait for failover to trigger (2 failures × 50ms interval + buffer).
  // Adaptive: poll until follower becomes leader.
  await waitFor(async () => {
    try {
      const res = await fetch(`${follower.url}/cluster/status`);
      const data = await res.json();
      return data.mode === "leader";
    } catch {
      return false;
    }
  });

  // Follower should now be leader.
  const statusRes = await fetch(`${follower.url}/cluster/status`);
  const status = await statusRes.json();

  assert.equal(statusRes.status, 200);
  assert.equal(status.mode, "leader", "Follower should have promoted to leader");
  assert.equal(status.writable, true, "Promoted leader should be writable");
  assert.equal(status.forwarding, false, "Promoted leader should not forward");
  assert.equal(
    status.replication.status,
    "active",
    "Promoted leader should have active replication status"
  );

  // Follower should serve MCP writes directly after promotion.
  const client2 = new Client({
    name: "failover-test-client2",
    version: "0.1.0",
  });
  const transport2 = new StreamableHTTPClientTransport(new URL(follower.mcpUrl));
  await client2.connect(transport2);

  const put = await callJsonTool(client2, "thing_put", {
    collection: "items",
    object: { id: "post-failover", text: "after" },
  });
  assert.equal(put.id, "post-failover");

  // Verify the write was persisted to the follower's local db.
  const get = await callJsonTool(client2, "thing_get", {
    collection: "items",
    id: "post-failover",
  });
  assert.equal(get.text, "after");

  await client2.close();
  await follower.close();
  cleanupDb(dbPath);
});

test("leader failover does not trigger without leaderElection enabled", async () => {
  const dbPath = getDbPath("fo-2");
  cleanupDb(dbPath);

  const port1 = nextPort();
  const port2 = nextPort();
  const url1 = `http://127.0.0.1:${port1}`;

  const leader = await startThingdHttpServer({
    path: ":memory:",
    driver: "native",
    port: port1,
    cluster: { mode: "leader" },
  });

  const follower = await startThingdHttpServer({
    path: dbPath,
    driver: "native",
    port: port2,
    cluster: {
      mode: "follower",
      leaderUrl: url1,
    },
  });

  // Sync some data.
  const client = new Client({
    name: "no-election-client",
    version: "0.1.0",
  });
  const transport = new StreamableHTTPClientTransport(new URL(leader.mcpUrl));
  await client.connect(transport);

  await callJsonTool(client, "thing_put", {
    collection: "items",
    object: { id: "no-election-obj", text: "sync" },
  });

  // Wait for replication
  await waitFor(async () => {
    try {
      const res = await fetch(`${follower.url}/cluster/status`);
      const data = await res.json();
      return data.replication?.lastReplicatedSequence > 0;
    } catch {
      return false;
    }
  });

  // Kill the leader.
  await leader.close();
  await client.close();

  // Wait for replication failures to accumulate (leader is dead)
  await waitFor(async () => {
    try {
      const res = await fetch(`${follower.url}/cluster/status`);
      const data = await res.json();
      return data.consecutiveFailures >= 4;
    } catch {
      return false;
    }
  });

  // Follower should still be follower (no election enabled).
  const statusRes = await fetch(`${follower.url}/cluster/status`);
  const status = await statusRes.json();

  assert.equal(status.mode, "follower", "Follower should remain in follower mode");
  assert.equal(status.writable, false);
  assert.equal(status.forwarding, true);

  await follower.close();
  cleanupDb(dbPath);
});
