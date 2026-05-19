import type { MemoryDDriver } from "@sayanmohsin/memoryd";
import type { MemorydMcpAuditOptions } from "./audit.js";

export type MemoryDStorageDriver = Exclude<MemoryDDriver, "remote">;

export type HttpRuntimeSafetyOptions = {
  host: string;
  authToken?: string;
  allowUnauthenticated?: boolean;
};

export function parseMemorydDriver(value: string | undefined): MemoryDStorageDriver | undefined {
  if (!value) {
    return undefined;
  }

  if (value === "memory" || value === "native") {
    return value;
  }

  throw new Error(`Unsupported memoryd driver: ${value}`);
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
  env: Record<string, string | undefined>,
): MemorydMcpAuditOptions | false {
  const enabled =
    env.MEMORYD_MCP_AUDIT === undefined
      ? undefined
      : parseBooleanFlag(env.MEMORYD_MCP_AUDIT, "MEMORYD_MCP_AUDIT");

  if (enabled === false) {
    return false;
  }

  return {
    enabled,
    actor: env.MEMORYD_MCP_ACTOR,
    source: env.MEMORYD_MCP_SOURCE,
    stream: env.MEMORYD_MCP_AUDIT_STREAM,
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
    "MEMORYD_AUTH_TOKEN is required when the HTTP MCP runtime binds to a non-loopback host. Set MEMORYD_AUTH_TOKEN or MEMORYD_ALLOW_UNAUTHENTICATED=true for local-only experiments.",
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
