import { readFile, readdir, stat } from "node:fs/promises";
import path from "node:path";

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const allowedFiles = new Set([
  "Cargo.lock",
  "crates/thingd/Cargo.toml",
  "crates/thingd/src/error.rs",
  "crates/thingd/src/persistent.rs",
  // The isolated migration tool and its operator guide must name the legacy
  // format explicitly; production runtime and general docs stay neutral.
  "crates/thingd-migrate/Cargo.toml",
  "crates/thingd-migrate/src/main.rs",
  ".github/workflows/ci.yml",
  "docs/agent-implementation-guide.md",
  "docs/storage-backends.md",
]);
const scannedRoots = ["AGENTS.md", "README.md", "docs", "crates", "packages", ".github", "package.json"];
const textExtensions = new Set([".md", ".json", ".js", ".mjs", ".ts", ".rs", ".toml", ".yaml", ".yml"]);
const forbidden = /\bFjallEngine\b|\bFjall\b|\bfjall\b|feature\s*=\s*["']fjall["']/g;
const stalePublicClaims = [
  "The current native format is embedded RocksDB",
  "ThingDB is the default",
  "ThingDB is production-ready",
  "ThingDB replaces RocksDB",
];

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
const staleClaims = [];
for (const relativePath of [...new Set(files)]) {
  if (allowedFiles.has(relativePath)) {
    continue;
  }
  const source = await readFile(path.join(repoRoot, relativePath), "utf8");
  if (forbidden.test(source)) {
    violations.push(relativePath);
  }
  if (relativePath.endsWith(".md") || relativePath.endsWith(".yaml")) {
    for (const claim of stalePublicClaims) {
      if (source.includes(claim)) {
        staleClaims.push(`${relativePath}: ${claim}`);
      }
    }
  }
  forbidden.lastIndex = 0;
}

const storageGuide = (
  await readFile(path.join(repoRoot, "docs/storage-backends.md"), "utf8")
)
  .toLowerCase()
  .replace(/\s+/g, " ");
for (const required of [
  "thingd_storage_backend=thingdb",
  "experimental",
  "does not open rocksdb files directly",
  "logical repack",
]) {
  if (!storageGuide.includes(required)) {
    staleClaims.push(`docs/storage-backends.md is missing required guidance: ${required}`);
  }
}

if (violations.length > 0) {
  console.error(
    `Backend-specific storage names are public in:\n${violations.map((file) => `- ${file}`).join("\n")}`
  );
  process.exit(1);
}

if (staleClaims.length > 0) {
  console.error(`Stale or unsafe public storage claims found:\n${staleClaims.map((claim) => `- ${claim}`).join("\n")}`);
  process.exit(1);
}

console.log("Public storage naming check passed: PersistentEngine is backend-neutral.");
