import assert from "node:assert/strict";
import test from "node:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { MemoryD } from "@sayanmohsin/memoryd";
import { createMemorydMcpServer } from "../dist/index.js";

test("lists memoryd MCP tools", async () => {
  const { client, server } = await connectTestClient();

  const tools = await client.listTools();
  const toolNames = tools.tools.map((tool) => tool.name);

  assert.ok(toolNames.includes("memory.search"));
  assert.ok(toolNames.includes("memory.objects.put"));
  assert.ok(toolNames.includes("memory.queue.push"));

  await client.close();
  await server.close();
});

test("stores and searches objects through MCP tools", async () => {
  const { client, server } = await connectTestClient();

  const putResult = await callJsonTool(client, "memory.objects.put", {
    collection: "decisions",
    object: {
      id: "mcp-server",
      text: "Expose memoryd through MCP.",
    },
  });
  const searchResult = await callJsonTool(client, "memory.search", {
    query: "MCP",
    collections: ["decisions"],
  });

  assert.equal(putResult.collection, "decisions");
  assert.equal(putResult.version, 1);
  assert.equal(searchResult[0].kind, "object");
  assert.equal(searchResult[0].id, "mcp-server");

  await client.close();
  await server.close();
});

test("writes audit events for MCP mutations", async () => {
  const { client, server } = await connectTestClient();

  await callJsonTool(client, "memory.objects.put", {
    collection: "decisions",
    object: {
      id: "audit-events",
      text: "MCP writes append audit events.",
    },
    actor: "test-agent",
    source: "unit-test",
  });
  const events = await callJsonTool(client, "memory.events.list", {
    stream: "__memoryd:mcp:audit",
  });

  assert.equal(events.length, 1);
  assert.equal(events[0].type, "mcp.objects.put");
  assert.equal(events[0].actor, "test-agent");
  assert.equal(events[0].source, "unit-test");
  assert.deepEqual(events[0].target, {
    collection: "decisions",
    id: "audit-events",
  });

  await client.close();
  await server.close();
});

test("pushes, claims, and acks queue jobs through MCP tools", async () => {
  const { client, server } = await connectTestClient();

  const pushed = await callJsonTool(client, "memory.queue.push", {
    queue: "embed",
    payload: {
      object: "decisions/mcp-server",
    },
    idempotencyKey: "embed:decisions/mcp-server:v1",
  });
  const claimed = await callJsonTool(client, "memory.queue.claim", {
    queue: "embed",
  });
  const acked = await callJsonTool(client, "memory.queue.ack", {
    queue: "embed",
    id: pushed.id,
  });

  assert.equal(claimed.id, pushed.id);
  assert.equal(claimed.status, "leased");
  assert.equal(acked.ok, true);
  assert.equal(acked.job.status, "completed");

  await client.close();
  await server.close();
});

async function connectTestClient() {
  const db = await MemoryD.open(":memory:");
  const server = createMemorydMcpServer(db);
  const client = new Client({
    name: "memoryd-mcp-test",
    version: "0.1.0",
  });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();

  await Promise.all([server.connect(serverTransport), client.connect(clientTransport)]);

  return {
    client,
    server,
  };
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
