import { mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const THINGD_DIR_NAME = ".thingd";
const THINGD_DB_NAME = "data.db";

export function defaultThingdDir(): string {
  return join(homedir(), THINGD_DIR_NAME);
}

export function defaultThingdDbPath(): string {
  return join(defaultThingdDir(), THINGD_DB_NAME);
}

export function ensureThingdDir(): void {
  mkdirSync(defaultThingdDir(), { recursive: true });
}
