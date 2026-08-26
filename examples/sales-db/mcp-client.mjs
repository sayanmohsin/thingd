import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";

export function createSalesClient(name) {
  const mcpUrl = process.env.THINGD_MCP_URL;
  const authToken = process.env.THINGD_AUTH_TOKEN;

  if (!mcpUrl || !authToken) {
    throw new Error("THINGD_MCP_URL and THINGD_AUTH_TOKEN are required");
  }

  const client = new Client({ name, version: "0.1.0" });
  const transport = new StreamableHTTPClientTransport(new URL(mcpUrl), {
    requestInit: { headers: { Authorization: `Bearer ${authToken}` } },
  });

  return { client, transport };
}
