import assert from "node:assert/strict";
import test from "node:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { startMemorydHttpServer } from "../dist/index.js";

test("serves health checks", async () => {
  const runtime = await startMemorydHttpServer({
    path: ":memory:",
    port: 0,
  });

  const response = await fetch(`${runtime.url}/healthz`);
  const body = await response.json();

  assert.equal(response.status, 200);
  assert.equal(body.ok, true);
  assert.equal(body.service, "memoryd-mcp");

  await runtime.close();
});

test("rejects unauthenticated MCP requests when auth token is configured", async () => {
  const runtime = await startMemorydHttpServer({
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

test("calls memoryd MCP tools over Streamable HTTP", async () => {
  const runtime = await startMemorydHttpServer({
    path: ":memory:",
    port: 0,
    authToken: "test-token",
  });
  const client = new Client({
    name: "memoryd-http-test",
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
  const put = await callJsonTool(client, "memory.objects.put", {
    collection: "decisions",
    object: {
      id: "remote-mcp",
      text: "Expose memoryd over Streamable HTTP.",
    },
  });
  const search = await callJsonTool(client, "memory.search", {
    query: "Streamable HTTP",
    collections: ["decisions"],
  });

  assert.ok(tools.tools.some((tool) => tool.name === "memory.objects.put"));
  assert.equal(put.id, "remote-mcp");
  assert.equal(search[0].id, "remote-mcp");

  await client.close();
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
