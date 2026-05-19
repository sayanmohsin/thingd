import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type { MemoryD } from "@sayanmohsin/memoryd";
import { registerMemorydTools } from "./tools.js";

export function createMemorydMcpServer(db: MemoryD): McpServer {
  const server = new McpServer(
    {
      name: "memoryd",
      version: "0.1.0",
    },
    {
      instructions:
        "Use memoryd tools to search, read, write, and queue work in an object-shaped local memory store. Prefer searching before writing duplicate memory.",
    },
  );

  registerMemorydTools(server, db);
  return server;
}
