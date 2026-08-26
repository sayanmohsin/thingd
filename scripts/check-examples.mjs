import { readdir, readFile } from "node:fs/promises";
import { join, relative } from "node:path";
import { execFileSync } from "node:child_process";

const root = join(process.cwd(), "examples");
const sourceExtensions = new Set([".js", ".mjs"]);
const forbiddenPatterns = [
  [/md_(?:test|live)_[A-Za-z0-9_-]+/, "credential-like token"],
  [/api\.thingd\.cloud\/mcp\/proj_[A-Za-z0-9_-]+/, "hardcoded Cloud project URL"],
  [/(?:\/Users|\/home)\/[A-Za-z0-9._-]+/, "machine-specific absolute path"],
];

async function collectFiles(directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.name === "node_modules" || entry.name === "dist") {
      continue;
    }
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await collectFiles(path)));
    } else {
      files.push(path);
    }
  }
  return files;
}

const failures = [];
for (const file of await collectFiles(root)) {
  const text = await readFile(file, "utf8");
  const label = relative(process.cwd(), file);

  for (const [pattern, description] of forbiddenPatterns) {
    if (pattern.test(text)) {
      failures.push(`${label}: contains ${description}`);
    }
  }

  const extension = file.slice(file.lastIndexOf("."));
  if (sourceExtensions.has(extension)) {
    try {
      execFileSync(process.execPath, ["--check", file], { stdio: "pipe" });
    } catch (error) {
      failures.push(`${label}: JavaScript syntax check failed\n${error.stderr?.toString() ?? ""}`);
    }
  }

  if (file.endsWith(".sh")) {
    try {
      execFileSync("bash", ["-n", file], { stdio: "pipe" });
    } catch (error) {
      failures.push(`${label}: shell syntax check failed\n${error.stderr?.toString() ?? ""}`);
    }
  }
}

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exit(1);
}

console.log("Example safety and JavaScript syntax checks passed.");
