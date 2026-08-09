import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";

export type SyncRole = "source" | "replica";

export type SyncConfig = {
  localUrl: string;
  remoteUrl: string;
  localToken?: string;
  remoteToken?: string;
  role?: SyncRole;
  cursor: number;
  paused?: boolean;
  provider?: "self-hosted" | "thingd.cloud" | string;
  projectId?: string;
  instanceSlug?: string;
  sourceId?: string;
  allowCloudTarget?: boolean;
  targetConfirmed?: boolean;
  configHash?: string;
};

export function syncConfigPath(): string {
  return join(homedir(), ".thingd", "sync.json");
}

export function readSyncConfig(): SyncConfig | null {
  try {
    return JSON.parse(readFileSync(syncConfigPath(), "utf8")) as SyncConfig;
  } catch {
    return null;
  }
}

export function writeSyncConfig(config: SyncConfig): void {
  const path = syncConfigPath();
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  writeFileSync(path, `${JSON.stringify(config, null, 2)}\n`, { mode: 0o600 });
}

export function removeSyncConfig(): void {
  const config = readSyncConfig();
  if (config) {
    writeSyncConfig({ ...config, cursor: 0, paused: false });
  }
}
