import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type { MemoryD } from "@sayanmohsin/memoryd";
import type { MemorydMcpAuditOptions } from "./audit.js";
import { registerMemorydTools } from "./tools.js";

export type MemorydMcpServerOptions = {
  audit?: MemorydMcpAuditOptions | false;
};

export function createMemorydMcpServer(
  db: MemoryD,
  options: MemorydMcpServerOptions = {},
): McpServer {
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

  registerMemorydTools(server, db, options);
  return server;
}
