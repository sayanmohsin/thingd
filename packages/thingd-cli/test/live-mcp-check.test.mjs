import assert from "node:assert/strict";
import test from "node:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";

test("connects to live thingd MCP and lists tools", async () => {
  const mcpUrl = process.env.THINGD_MCP_URL ?? "http://127.0.0.1:8757/mcp";
  const authToken = process.env.THINGD_AUTH_TOKEN;
  const transport = new StreamableHTTPClientTransport(new URL(mcpUrl), {
    requestInit: authToken
      ? {
          headers: {
            Authorization: `Bearer ${authToken}`,
          },
        }
      : undefined,
  });
  const client = new Client({ name: "thingd-live-mcp-check", version: "0.1.0" });

  try {
    await client.connect(transport);
    const tools = await client.listTools();
    assert.ok(Array.isArray(tools.tools), "tools.tools should be an array");
    assert.ok(tools.tools.length > 0, "should list at least one MCP tool");
  } finally {
    await client.close();
  }
});
