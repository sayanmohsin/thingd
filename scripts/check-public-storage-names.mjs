import { readFile, readdir, stat } from "node:fs/promises";
import path from "node:path";

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const allowedFiles = new Set([
  "Cargo.lock",
  "crates/thingd/Cargo.toml",
  "crates/thingd/src/error.rs",
  "crates/thingd/src/persistent.rs",
]);
const scannedRoots = ["AGENTS.md", "README.md", "docs", "crates", "packages", ".github", "package.json"];
const textExtensions = new Set([".md", ".json", ".js", ".mjs", ".ts", ".rs", ".toml", ".yaml", ".yml"]);
const forbidden = /\bFjallEngine\b|\bFjall\b|\bfjall\b|feature\s*=\s*["']fjall["']/g;

async function collect(relativePath) {
  const absolutePath = path.join(repoRoot, relativePath);
  const entries = await readdir(absolutePath, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    if (entry.isDirectory() && [".git", "dist", "node_modules", "target"].includes(entry.name)) {
      continue;
    }
    const child = path.join(relativePath, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await collect(child)));
    } else if (textExtensions.has(path.extname(entry.name)) || entry.name === "AGENTS.md") {
      files.push(child);
    }
  }
  return files;
}

const files = [];
for (const root of scannedRoots) {
  const absoluteRoot = path.join(repoRoot, root);
  if ((await stat(absoluteRoot)).isDirectory()) {
    files.push(...(await collect(root)));
  } else if (textExtensions.has(path.extname(root)) || root === "AGENTS.md") {
    files.push(root);
  }
}

const violations = [];
for (const relativePath of [...new Set(files)]) {
  if (allowedFiles.has(relativePath)) {
    continue;
  }
  const source = await readFile(path.join(repoRoot, relativePath), "utf8");
  if (forbidden.test(source)) {
    violations.push(relativePath);
  }
  forbidden.lastIndex = 0;
}

if (violations.length > 0) {
  console.error(
    `Backend-specific storage names are public in:\n${violations.map((file) => `- ${file}`).join("\n")}`
  );
  process.exit(1);
}

console.log("Public storage naming check passed: PersistentEngine is backend-neutral.");
