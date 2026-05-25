import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { type CliContext, withDb } from "./index.js";
import { createThingdMcpServer } from "./mcp/index.js";

export async function runMcp(context: CliContext): Promise<void> {
  await withDb(context, async (db) => {
    // We pass empty options to createThingdMcpServer so it uses default audit behavior
    const server = createThingdMcpServer(db);
    const transport = new StdioServerTransport();

    await server.connect(transport);

    // Keep the process alive and the database connection open
    // so the MCP server can continue to receive messages over stdio.
    return new Promise(() => {});
  });
}
