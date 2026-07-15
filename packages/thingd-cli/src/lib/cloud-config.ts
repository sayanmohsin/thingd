import { existsSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { defaultThingdDir, ensureThingdDir } from "../paths.js";

const CONFIG_FILE = "cloud-config.json";

export type CloudConfig = {
  token: string;
  /** API key for data plane operations (REST/MCP) — created after login. */
  apiKey?: string;
  email?: string;
  url?: string;
  /** Currently active organization context (set by `thingd cloud org use`). */
  organizationId?: string;
  /** Resolved MCP URL for the active cloud instance (auto-discovered or set by `thingd cloud instance use`). */
  instanceUrl?: string;
  /** Active project ID (set when instanceUrl is resolved). */
  projectId?: string;
  /** Active project slug (set when instanceUrl is resolved). */
  projectSlug?: string;
  /** Active instance slug (set when instanceUrl is resolved). */
  instanceSlug?: string;
};

export function cloudConfigPath(): string {
  return join(defaultThingdDir(), CONFIG_FILE);
}

export function readCloudConfig(): CloudConfig | null {
  const path = cloudConfigPath();
  if (!existsSync(path)) {
    return null;
  }
  try {
    return JSON.parse(readFileSync(path, "utf-8")) as CloudConfig;
  } catch {
    return null;
  }
}

export function writeCloudConfig(config: CloudConfig): void {
  ensureThingdDir();
  writeFileSync(cloudConfigPath(), JSON.stringify(config, null, 2), "utf-8");
}

/**
 * Returns the best available cloud MCP URL from saved config.
 * Only returns URL when a concrete instance endpoint is available
 * (instanceUrl) — the bare API url is not enough for MCP connections.
 */
export function resolveCloudUrl(config: CloudConfig): string | undefined {
  if (config.instanceUrl) {
    return config.instanceUrl;
  }
  return undefined;
}

export function removeCloudConfig(): void {
  try {
    unlinkSync(cloudConfigPath());
  } catch {
    // File may not exist
  }
}
