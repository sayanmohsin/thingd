// Re-export core MCP logic from the SDK
export {
  appendMcpAuditEvent,
  createThingdMcpServer,
  jsonResult,
  parseCollectionAllowlist,
  parsePayloadSizeLimit,
  type RegisterThingdToolsOptions,
  readMcpHardeningOptionsFromEnv,
  registerThingdTools,
  resolveThingdMcpAuditOptions,
  type ThingdMcpAuditMetadata,
  type ThingdMcpAuditOptions,
  type ThingdMcpHardeningOptions,
  type ThingdMcpServerOptions,
} from "thingd";

// CLI-only: cluster + HTTP transport
export type {
  ResolvedThingdClusterOptions,
  ThingdClusterDiscovery,
  ThingdClusterMode,
  ThingdClusterOptions,
  ThingdClusterStatus,
} from "./cluster.js";
export type { RunningThingdHttpServer, ThingdHttpServerOptions } from "./http.js";
export { startThingdHttpServer } from "./http.js";
