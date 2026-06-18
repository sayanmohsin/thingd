import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type { ThingD } from "thingd";
import { SDK_VERSION } from "thingd";
import type { ThingdMcpAuditOptions } from "./audit.js";
import type { ThingdMcpHardeningOptions } from "./config.js";
import { registerThingdTools } from "./tools.js";

export type ThingdMcpServerOptions = {
  audit?: ThingdMcpAuditOptions | false;
  hardening?: ThingdMcpHardeningOptions;
};

/**
 * Create a new McpServer with all thingd tools registered.
 *
 * Note: a new instance must be created per HTTP request because
 * @modelcontextprotocol/sdk's underlying Server.connect() throws if called
 * on an already-connected instance (stateless HTTP mode: one transport per
 * request lifecycle). Tool registration is cheap (Map insertions) so this
 * is not a meaningful overhead in practice.
 */
export function createThingdMcpServer(db: ThingD, options: ThingdMcpServerOptions = {}): McpServer {
  const server = new McpServer(
    {
      name: "thingd",
      version: SDK_VERSION,
    },
    {
      instructions:
        "Use thingd tools to search, read, write, and queue work in an object-shaped local memory store. Prefer searching before writing duplicate memory.",
    }
  );

  registerThingdTools(server, db, {
    audit: options.audit,
    hardening: options.hardening,
  });

  // MCP resource: thingd://collections — list known collection names.
  // Registered as a static URI so it appears in resources/list.
  // Honours the collection allowlist: only allowed collections are surfaced.
  server.registerResource(
    "thingd-collections",
    "thingd://collections",
    {
      title: "Collections",
      description:
        "Lists all known thingd object collections. If a collection allowlist is configured, only allowed collections appear.",
      mimeType: "application/json",
    },
    async (_uri) => {
      const allCollections: string[] = (await db.listCollections?.()) ?? [];
      const allowlist = options.hardening?.collectionAllowlist;
      const collections = allowlist
        ? allCollections.filter((c) => allowlist.has(c))
        : allCollections;

      return {
        contents: [
          {
            uri: "thingd://collections",
            text: JSON.stringify(
              collections.map((name) => ({
                name,
                uri: `thingd://collections/${name}`,
              })),
              null,
              2
            ),
            mimeType: "application/json",
          },
        ],
      };
    }
  );

  return server;
}
