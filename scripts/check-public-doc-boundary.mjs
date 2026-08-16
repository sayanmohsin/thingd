import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";

const trackedFiles = execFileSync("git", ["ls-files"], { encoding: "utf8" })
  .trim()
  .split("\n")
  .filter((file) => Boolean(file) && existsSync(file));

const forbiddenPaths = [
  /^docs\/(roadmap|handoff|sidecar-cluster)\.md$/,
  /^docs\/thingd(?:\/|$)/,
  /^\.opencode\/plans\/(active|blocked)\/.*\.md$/,
];

const forbiddenContent = [
  /github\.com\/sayanmohsin\/thingd-cloud\/blob/i,
  /docs\/thingd\/(?:roadmap|handoff|sidecar-cluster)/i,
  /BEGIN (?:RSA |OPENSSH |EC )?PRIVATE KEY/,
  /(?:sk-|ghp_|github_pat_|xox[baprs]-)(?!abcdefghijklmnopqrstuvwxyz12)[A-Za-z0-9_-]{20,}/,
];

const violations = [];

for (const file of trackedFiles) {
  if (forbiddenPaths.some((pattern) => pattern.test(file))) {
    violations.push(`${file}: private planning path is not allowed in public thingd`);
    continue;
  }

  if (!/\.(?:md|mdx|json|ya?ml|toml|ts|js|mjs|tsx)$/.test(file)) {
    continue;
  }

  const contents = readFileSync(file, "utf8");
  for (const pattern of forbiddenContent) {
    if (pattern.test(contents)) {
      violations.push(`${file}: matches forbidden public-boundary pattern ${pattern}`);
    }
  }
}

if (violations.length > 0) {
  console.error("Public documentation boundary check failed:");
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  process.exitCode = 1;
} else {
  console.log(`Public documentation boundary check passed (${trackedFiles.length} tracked files).`);
}
