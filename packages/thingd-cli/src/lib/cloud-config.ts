import { existsSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { defaultThingdDir, ensureThingdDir } from "../paths.js";

const CONFIG_FILE = "cloud-config.json";

export type CloudConfig = {
  token: string;
  email?: string;
  url?: string;
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

export function removeCloudConfig(): void {
  try {
    unlinkSync(cloudConfigPath());
  } catch {
    // File may not exist
  }
}
