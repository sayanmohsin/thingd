import assert from "node:assert/strict";
import test from "node:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { ThingD } from "thingd";
import { startThingdHttpServer } from "../dist/index.js";

test("serves health checks", async () => {
  const runtime = await startThingdHttpServer({
    path: ":memory:",
    port: 0,
  });

  const response = await fetch(`${runtime.url}/healthz`);
  const body = await response.json();

  assert.equal(response.status, 200);
  assert.equal(body.ok, true);
  assert.equal(body.service, "thingd-mcp");
  assert.equal(body.cluster.mode, "single");

  await runtime.close();
});

test("rejects unauthenticated MCP requests when auth token is configured", async () => {
  const runtime = await startThingdHttpServer({
    path: ":memory:",
    port: 0,
    authToken: "test-token",
  });

  const response = await fetch(runtime.mcpUrl, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "tools/list",
    }),
  });

  assert.equal(response.status, 401);

  await runtime.close();
});

test("requires auth token when binding HTTP MCP to non-loopback hosts", async () => {
  await assert.rejects(
    () =>
      startThingdHttpServer({
        path: ":memory:",
        host: "0.0.0.0",
        port: 0,
      }),
    /THINGD_AUTH_TOKEN is required/,
  );
});

test("calls thingd MCP tools over Streamable HTTP", async () => {
  const runtime = await startThingdHttpServer({
    path: ":memory:",
    port: 0,
    authToken: "test-token",
  });
  const client = new Client({
    name: "thingd-http-test",
    version: "0.1.0",
  });
  const transport = new StreamableHTTPClientTransport(new URL(runtime.mcpUrl), {
    requestInit: {
      headers: {
        Authorization: "Bearer test-token",
      },
    },
  });

  await client.connect(transport);

  const tools = await client.listTools();
  const put = await callJsonTool(client, "thing.put", {
    collection: "decisions",
    object: {
      id: "remote-mcp",
      text: "Expose thingd over Streamable HTTP.",
    },
  });
  const search = await callJsonTool(client, "thing.search", {
    query: "Streamable HTTP",
    collections: ["decisions"],
  });

  assert.ok(tools.tools.some((tool) => tool.name === "thing.put"));
  assert.equal(put.id, "remote-mcp");
  assert.equal(search[0].id, "remote-mcp");

  await client.close();
  await runtime.close();
});

test("serves cluster status and peers", async () => {
  const runtime = await startThingdHttpServer({
    path: ":memory:",
    port: 0,
    cluster: {
      mode: "leader",
      advertiseUrl: "http://thingd-0:8757",
      peers: ["http://thingd-0:8757", "http://thingd-1:8757"],
      discovery: "static",
    },
  });

  const statusResponse = await fetch(`${runtime.url}/cluster/status`);
  const peersResponse = await fetch(`${runtime.url}/cluster/peers`);
  const status = await statusResponse.json();
  const peers = await peersResponse.json();

  assert.equal(statusResponse.status, 200);
  assert.equal(status.mode, "leader");
  assert.equal(status.writable, true);
  assert.equal(status.replication, "not-implemented");
  assert.deepEqual(peers.peers, ["http://thingd-0:8757", "http://thingd-1:8757"]);

  await runtime.close();
});

test("forwards follower MCP traffic to the leader", async () => {
  const leader = await startThingdHttpServer({
    path: ":memory:",
    port: 0,
    authToken: "leader-token",
    cluster: {
      mode: "leader",
    },
  });
  const follower = await startThingdHttpServer({
    path: ":memory:",
    port: 0,
    authToken: "follower-token",
    cluster: {
      mode: "follower",
      leaderUrl: leader.url,
      forwardAuthToken: "leader-token",
    },
  });
  const client = new Client({
    name: "thingd-follower-test",
    version: "0.1.0",
  });
  const transport = new StreamableHTTPClientTransport(new URL(follower.mcpUrl), {
    requestInit: {
      headers: {
        Authorization: "Bearer follower-token",
      },
    },
  });

  await client.connect(transport);

  const put = await callJsonTool(client, "thing.put", {
    collection: "decisions",
    object: {
      id: "forwarded",
      text: "Follower forwarded this write.",
    },
  });
  const search = await callJsonTool(client, "thing.search", {
    query: "forwarded",
    collections: ["decisions"],
  });

  assert.equal(put.id, "forwarded");
  assert.equal(search[0].id, "forwarded");

  await client.close();
  await follower.close();
  await leader.close();
});

test("supports the Node SDK remote driver over Streamable HTTP", async () => {
  const runtime = await startThingdHttpServer({
    path: ":memory:",
    port: 0,
    authToken: "test-token",
  });
  const db = await ThingD.open({
    url: runtime.mcpUrl,
    driver: "remote",
    authToken: "test-token",
  });

  const stored = await db.put("decisions", {
    id: "remote-sdk",
    text: "Node apps can use the thingd sidecar through the SDK.",
  });
  const found = await db.get("decisions", "remote-sdk");
  const hits = await db.search("sidecar", {
    collections: ["decisions"],
  });

  assert.equal(stored.collection, "decisions");
  assert.equal(found?.text, "Node apps can use the thingd sidecar through the SDK.");
  assert.equal(hits[0].id, "remote-sdk");

  await db.close();
  await runtime.close();
});

async function callJsonTool(client, name, args) {
  const result = await client.callTool({
    name,
    arguments: args,
  });

  const text = result.content.find((part) => part.type === "text")?.text;
  assert.equal(typeof text, "string");

  return JSON.parse(text);
}
