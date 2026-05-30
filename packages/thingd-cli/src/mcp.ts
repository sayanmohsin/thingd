import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { type CliContext, resolveConnection, withDb } from "./index.js";
import { createThingdMcpServer } from "./mcp/index.js";

export async function runMcp(context: CliContext): Promise<void> {
  const connection = resolveConnection(context);
  await withDb(context, async (db) => {
    // We pass empty options to createThingdMcpServer so it uses default audit behavior
    const server = createThingdMcpServer(db);
    const transport = new StdioServerTransport();

    context.stderr.write(`\nthingd stdio MCP server started successfully.\n`);
    context.stderr.write(`  ✓ Database: ${connection.path}\n`);
    context.stderr.write(`  ✓ Driver:   ${connection.driver ?? "memory"}\n`);
    context.stderr.write(`  ✓ Transport: Stdio (listening silently on stdin)\n\n`);

    await server.connect(transport);

    // Keep the process alive and the database connection open
    // so the MCP server can continue to receive messages over stdio.
    return new Promise(() => {});
  });
}
