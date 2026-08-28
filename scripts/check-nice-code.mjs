#!/usr/bin/env node

import { existsSync } from "node:fs";
import { execFileSync, spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const pinnedCommit = "f3c15e364919969c9c1b9e2f11b837ca4d8f4fb3";
const niceCodeDirectory = resolve(process.env.NICE_CODE_DIR ?? join(repoRoot, ".nice-code"));
const checker = join(niceCodeDirectory, "scripts", "check.mjs");
const format = process.env.NICE_CODE_FORMAT ?? "text";

if (!existsSync(checker)) {
  console.error(
    `Nice Code is not available at ${niceCodeDirectory}. Clone nice-code and set NICE_CODE_DIR.`,
  );
  process.exit(2);
}

let actualCommit;
try {
  actualCommit = execFileSync("git", ["-C", niceCodeDirectory, "rev-parse", "HEAD"], {
    encoding: "utf8",
  }).trim();
} catch (error) {
  console.error(`Could not read the Nice Code revision: ${error.message}`);
  process.exit(2);
}

if (actualCommit !== pinnedCommit) {
  console.error(`Nice Code must be pinned to ${pinnedCommit}; found ${actualCommit}.`);
  process.exit(2);
}

if (!["text", "json", "sarif", "agent"].includes(format)) {
  console.error("NICE_CODE_FORMAT must be text, json, sarif, or agent.");
  process.exit(2);
}

const result = spawnSync(
  process.execPath,
  [checker, "--project", repoRoot, "--ci", "--all", "--format", format],
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
