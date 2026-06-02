import type { ThingDDriver } from "thingd";
import type { ThingdMcpAuditOptions } from "./audit.js";

export type ThingDStorageDriver = Exclude<ThingDDriver, "remote">;

export type HttpRuntimeSafetyOptions = {
  host: string;
  authToken?: string;
  allowUnauthenticated?: boolean;
};

export type ThingdMcpHardeningOptions = {
  /** Comma-separated collection allowlist from THINGD_MCP_COLLECTIONS. Empty = all allowed. */
  collectionAllowlist?: Set<string>;
  /** When true, all write tools are rejected. Set via THINGD_MCP_READ_ONLY=true. */
  readOnly?: boolean;
  /** Maximum HTTP request body in bytes. Set via THINGD_MCP_MAX_PAYLOAD_BYTES. Default 512 KB. */
  maxPayloadBytes?: number;
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
  env: Record<string, string | undefined>,
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
    "THINGD_AUTH_TOKEN is required when the HTTP MCP runtime binds to a non-loopback host. Set THINGD_AUTH_TOKEN or THINGD_ALLOW_UNAUTHENTICATED=true for local-only experiments.",
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

/**
 * Parse THINGD_MCP_COLLECTIONS into a Set.
 * An empty string or missing env var means all collections are allowed.
 */
export function parseCollectionAllowlist(value: string | undefined): Set<string> | undefined {
  if (!value?.trim()) {
    return undefined;
  }

  const names = value
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);

  return names.length > 0 ? new Set(names) : undefined;
}

/**
 * Parse THINGD_MCP_MAX_PAYLOAD_BYTES. Defaults to 512 KB if unset or zero.
 */
export function parsePayloadSizeLimit(value: string | undefined, defaultBytes = 524_288): number {
  if (!value) {
    return defaultBytes;
  }

  const n = Number.parseInt(value, 10);
  if (!Number.isInteger(n) || n <= 0) {
    throw new Error(`Invalid THINGD_MCP_MAX_PAYLOAD_BYTES: ${value}`);
  }

  return n;
}

/**
 * Read all Phase-6 MCP hardening options from the environment.
 */
export function readMcpHardeningOptionsFromEnv(
  env: Record<string, string | undefined>,
): ThingdMcpHardeningOptions {
  return {
    collectionAllowlist: parseCollectionAllowlist(env.THINGD_MCP_COLLECTIONS),
    readOnly: env.THINGD_MCP_READ_ONLY
      ? parseBooleanFlag(env.THINGD_MCP_READ_ONLY, "THINGD_MCP_READ_ONLY")
      : undefined,
    maxPayloadBytes: parsePayloadSizeLimit(env.THINGD_MCP_MAX_PAYLOAD_BYTES),
  };
}
