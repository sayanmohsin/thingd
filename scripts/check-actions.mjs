import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const roots = [".github/workflows", ".github/actions"];
const files = [];

function collectFiles(directory) {
  for (const entry of readdirSync(directory)) {
    const path = join(directory, entry);
    if (statSync(path).isDirectory()) {
      collectFiles(path);
    } else if (/\.(yaml|yml)$/.test(entry)) {
      files.push(path);
    }
  }
}

for (const root of roots) {
  collectFiles(root);
}

const deprecatedCacheAction = /uses:\s*actions\/cache(?:\/(?:restore|save))?@v[1-4](?:\s|$)/;
const deprecatedArtifactAction = /uses:\s*actions\/upload-artifact@v[1-6](?:\s|$)/;
const violations = [];

for (const file of files) {
  const lines = readFileSync(file, "utf8").split("\n");
  lines.forEach((line, index) => {
    if (deprecatedCacheAction.test(line)) {
      violations.push(`${relative(process.cwd(), file)}:${index + 1}: ${line.trim()}`);
    }
    if (deprecatedArtifactAction.test(line)) {
      violations.push(`${relative(process.cwd(), file)}:${index + 1}: ${line.trim()}`);
    }
  });
}

if (violations.length > 0) {
  console.error("Deprecated GitHub Actions runtime detected. Use cache v5 and upload-artifact v7 or newer:");
  console.error(violations.join("\n"));
  process.exit(1);
}

console.log(`GitHub Actions runtime check passed (${files.length} workflow files).`);
