import { existsSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { defaultThingdDir, ensureThingdDir } from "../paths.js";

const CONFIG_FILE = "cloud-config.json";

export type CloudConfig = {
  token: string;
  email?: string;
  url?: string;
  /** Currently active organization context (set by `thingd cloud org use`). */
  organizationId?: string;
  /** Resolved MCP URL for the active cloud instance (auto-discovered or set by `thingd cloud instance use`). */
  instanceUrl?: string;
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
 * Priority: instanceUrl > url (with /mcp appended if bare).
 */
export function resolveCloudUrl(config: CloudConfig): string | undefined {
  if (config.instanceUrl) {
    return config.instanceUrl;
  }
  if (config.url) {
    const u = new URL(config.url);
    if (u.pathname === "" || u.pathname === "/") {
      u.pathname = "/mcp";
    }
    return u.toString();
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
