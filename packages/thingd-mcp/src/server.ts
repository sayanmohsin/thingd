import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type { ThingD } from "thingd";
import type { ThingdMcpAuditOptions } from "./audit.js";
import { registerThingdTools } from "./tools.js";

export type ThingdMcpServerOptions = {
  audit?: ThingdMcpAuditOptions | false;
};

export function createThingdMcpServer(db: ThingD, options: ThingdMcpServerOptions = {}): McpServer {
  const server = new McpServer(
    {
      name: "thingd",
      version: "0.1.0",
    },
    {
      instructions:
        "Use thingd tools to search, read, write, and queue work in an object-shaped local memory store. Prefer searching before writing duplicate memory.",
    },
  );

  registerThingdTools(server, db, options);
  return server;
}
