import type { ThingDDriver, ThingdMcpAuditOptions } from "@thingd/sdk";

export type ThingDStorageDriver = Exclude<ThingDDriver, "cloud">;

export type HttpRuntimeSafetyOptions = {
  host: string;
  authToken?: string;
  allowUnauthenticated?: boolean;
};

export function parseThingdDriver(value: string | undefined): ThingDStorageDriver | undefined {
  if (!value) {
    return undefined;
  }

  if (value === "memory" || value === "native") {
    return value;
  }

  throw new Error(`Unsupported thingd driver: ${value}`);
}

export function parsePort(value: string | undefined, fallback: number): number {
  if (!value) {
    return fallback;
  }

  const port = Number.parseInt(value, 10);

  if (!Number.isInteger(port) || port < 0 || port > 65_535) {
    throw new Error(`Invalid port: ${value}`);
  }

  return port;
}

export function parseBooleanFlag(value: string | undefined, name: string): boolean {
  if (!value) {
    return false;
  }

  const normalized = value.toLowerCase();
  if (["1", "true", "yes", "on"].includes(normalized)) {
    return true;
  }

  if (["0", "false", "no", "off"].includes(normalized)) {
    return false;
  }

  throw new Error(`Invalid ${name}: expected true or false`);
}

export function readMcpAuditOptionsFromEnv(
  env: Record<string, string | undefined>
): ThingdMcpAuditOptions | false {
  const enabled =
    env.THINGD_MCP_AUDIT === undefined
      ? undefined
      : parseBooleanFlag(env.THINGD_MCP_AUDIT, "THINGD_MCP_AUDIT");

  if (enabled === false) {
    return false;
  }

  return {
    enabled,
    actor: env.THINGD_MCP_ACTOR,
    source: env.THINGD_MCP_SOURCE,
    stream: env.THINGD_MCP_AUDIT_STREAM,
  };
}

export function readCliValue(args: string[], index: number, name: string): string {
  const value = args[index + 1];

  if (!value) {
    throw new Error(`${name} requires a value`);
  }

  return value;
}

export function ensureHttpRuntimeIsSafe(options: HttpRuntimeSafetyOptions): void {
  const authToken = options.authToken?.trim();

  if (authToken || options.allowUnauthenticated || isLoopbackHost(options.host)) {
    return;
  }

  throw new Error(
    "THINGD_AUTH_TOKEN is required when the HTTP MCP runtime binds to a non-loopback host. Set THINGD_AUTH_TOKEN or THINGD_ALLOW_UNAUTHENTICATED=true for local-only experiments."
  );
}

function isLoopbackHost(host: string): boolean {
  const normalized = host.toLowerCase();

  return (
    normalized === "localhost" ||
    normalized === "127.0.0.1" ||
    normalized === "::1" ||
    normalized === "[::1]"
  );
}
