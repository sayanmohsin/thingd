import assert from "node:assert/strict";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";

const mcpUrl = process.env.MEMORYD_MCP_URL ?? "http://127.0.0.1:8757/mcp";
const authToken = process.env.MEMORYD_AUTH_TOKEN;

const client = new Client({
  name: "memoryd-http-smoke",
  version: "0.1.0",
});
const transport = new StreamableHTTPClientTransport(new URL(mcpUrl), {
  requestInit: authToken
    ? {
        headers: {
          Authorization: `Bearer ${authToken}`,
        },
      }
    : undefined,
});

try {
  await client.connect(transport);
  const tools = await client.listTools();
  const toolNames = tools.tools.map((tool) => tool.name);

  assert.ok(toolNames.includes("memory.search"));
  assert.ok(toolNames.includes("memory.objects.put"));

  console.log(`memoryd MCP smoke passed for ${mcpUrl}`);
} finally {
  await client.close();
}
