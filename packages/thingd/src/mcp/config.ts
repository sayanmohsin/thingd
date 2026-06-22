export type ThingdMcpHardeningOptions = {
  /** Comma-separated collection allowlist from THINGD_MCP_COLLECTIONS. Empty = all allowed. */
  collectionAllowlist?: Set<string>;
  /** When true, all write tools are rejected. Set via THINGD_MCP_READ_ONLY=true. */
  readOnly?: boolean;
  /** Maximum HTTP request body in bytes. Set via THINGD_MCP_MAX_PAYLOAD_BYTES. Default 512 KB. */
  maxPayloadBytes?: number;
};

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

function parseBooleanFlag(value: string | undefined, name: string): boolean {
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

/**
 * Read all MCP hardening options from the environment.
 */
export function readMcpHardeningOptionsFromEnv(
  env: Record<string, string | undefined>
): ThingdMcpHardeningOptions {
  return {
    collectionAllowlist: parseCollectionAllowlist(env.THINGD_MCP_COLLECTIONS),
    readOnly: env.THINGD_MCP_READ_ONLY
      ? parseBooleanFlag(env.THINGD_MCP_READ_ONLY, "THINGD_MCP_READ_ONLY")
      : undefined,
    maxPayloadBytes: parsePayloadSizeLimit(env.THINGD_MCP_MAX_PAYLOAD_BYTES),
  };
}
