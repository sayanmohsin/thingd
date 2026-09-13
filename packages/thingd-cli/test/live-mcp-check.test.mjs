import assert from "node:assert/strict";
import { connect } from "node:net";
import test from "node:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";

async function isPortOpen(host, port, timeout = 1000) {
  return new Promise((resolve) => {
    const socket = connect(port, host, () => {
      socket.end();
      resolve(true);
    });
    socket.on("error", () => resolve(false));
    socket.setTimeout(timeout, () => {
      socket.destroy();
      resolve(false);
    });
  });
}

const liveCheckEnabled = process.env.THINGD_LIVE_MCP_CHECK === "1";
const mcpUrl = process.env.THINGD_MCP_URL ?? "http://127.0.0.1:8757/mcp";
const parsed = new URL(mcpUrl);
const mcpReachable = liveCheckEnabled && (await isPortOpen(parsed.hostname, Number(parsed.port)));
test("connects to live thingd MCP and lists tools", { skip: !mcpReachable }, async () => {
  const authToken = process.env.THINGD_AUTH_TOKEN;
  const tenantId = process.env.THINGD_TENANT_ID;
  const headers = {
    ...(authToken ? { Authorization: `Bearer ${authToken}` } : {}),
    ...(tenantId ? { "X-Tenant-Id": tenantId } : {}),
  };
  const transport = new StreamableHTTPClientTransport(new URL(mcpUrl), {
    requestInit: Object.keys(headers).length > 0 ? { headers } : undefined,
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
