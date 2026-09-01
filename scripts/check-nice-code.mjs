#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const format = process.env.NICE_CODE_FORMAT ?? "text";
const executable = join(
  repoRoot,
  "node_modules",
  ".bin",
  process.platform === "win32" ? "nice-code.cmd" : "nice-code",
);

if (!["text", "json", "sarif", "agent"].includes(format)) {
  console.error("NICE_CODE_FORMAT must be text, json, sarif, or agent.");
  process.exit(2);
}

const result = spawnSync(
  executable,
  ["--project", repoRoot, "--ci", "--all", "--format", format],
  {
    cwd: repoRoot,
    env: { ...process.env, NO_COLOR: "1" },
    stdio: "inherit",
  },
);

if (result.error) {
  console.error(`Could not run Nice Code: ${result.error.message}`);
  process.exit(2);
}

process.exit(result.status ?? 2);
