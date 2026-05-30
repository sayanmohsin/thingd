import type { CliContext } from "./index.js";
import { readClusterOptionsFromEnv } from "./mcp/cluster.js";
import { parsePort, parseThingdDriver, readMcpAuditOptionsFromEnv } from "./mcp/config.js";
import { startThingdHttpServer } from "./mcp/http.js";
import { defaultThingdDbPath, ensureThingdDir } from "./paths.js";

export async function runMcpHttp(context: CliContext): Promise<void> {
  const parsed = context.parsed;

  // Resolve port, host, allowUnauthenticated, etc. from flags or env
  const portStr = parsed.flags.get("port")?.at(-1) ?? context.env.THINGD_PORT;
  const host = parsed.flags.get("host")?.at(-1) ?? context.env.THINGD_HOST ?? "127.0.0.1";
  const path = parsed.flags.get("path")?.at(-1) ?? context.env.THINGD_PATH ?? defaultThingdDbPath();
  const driverStr = parsed.flags.get("driver")?.at(-1) ?? context.env.THINGD_DRIVER;
  const authToken = parsed.flags.get("auth-token")?.at(-1) ?? context.env.THINGD_AUTH_TOKEN;
  const allowUnauthenticated =
    parsed.booleans.has("allow-unauthenticated") || parsed.flags.has("allow-unauthenticated");

  const port = parsePort(portStr, 8757);
  const driver = parseThingdDriver(driverStr);

  if (path === defaultThingdDbPath()) {
    ensureThingdDir();
  }

  if (!authToken) {
    console.error("thingd-mcp HTTP runtime is starting without THINGD_AUTH_TOKEN.");
  }

  const runtime = await startThingdHttpServer({
    path,
    driver,
    host,
    port,
    authToken,
    allowUnauthenticated,
    audit: readMcpAuditOptionsFromEnv(context.env),
    cluster: readClusterOptionsFromEnv(context.env),
  });

  console.error(`thingd MCP HTTP runtime listening at ${runtime.mcpUrl}`);

  for (const signal of ["SIGINT", "SIGTERM"] as const) {
    process.once(signal, () => {
      void runtime.close().finally(() => process.exit(0));
    });
  }

  // Keep the process alive
  return new Promise(() => {});
}
