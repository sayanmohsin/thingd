import { readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";

const checkOnly = process.argv.includes("--check");
const root = process.cwd();
const workspaceCargo = await readFile(path.join(root, "Cargo.toml"), "utf8");
const workspaceVersion = workspaceCargo.match(/^version\s*=\s*"(\d+\.\d+\.\d+)"/m)?.[1];
if (!workspaceVersion) {
  throw new Error("Could not determine workspace package version from Cargo.toml");
}
const dependencyVersion = workspaceVersion.split(".").slice(0, 2).join(".");

async function cargoManifests(directory) {
  const entries = await readdir(path.join(root, directory), { withFileTypes: true });
  const manifests = [];
  for (const entry of entries) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      manifests.push(...(await cargoManifests(entryPath)));
    } else if (entry.isFile() && entry.name === "Cargo.toml") {
      manifests.push(entryPath);
    }
  }
  return manifests;
}

const manifests = [
  ...(await cargoManifests("crates")),
  ...(await cargoManifests("packages")),
];
const stale = [];

for (const manifest of manifests) {
  const absolutePath = path.join(root, manifest);
  const source = await readFile(absolutePath, "utf8");
  const updated = source.replace(
    /^(\s*[^#\n]+\{[^\n]*\bpath\s*=\s*"[^"]+"[^\n]*\bversion\s*=\s*")(\d+\.\d+)("[^\n]*\}\s*(?:#.*)?)$/gm,
    (line, prefix, version, suffix) => {
      if (version === dependencyVersion) {
        return line;
      }
      stale.push(`${manifest}: ${version} -> ${dependencyVersion}`);
      return `${prefix}${dependencyVersion}${suffix}`;
    }
  );
  if (!checkOnly && updated !== source) {
    await writeFile(absolutePath, updated);
  }
}

if (stale.length && checkOnly) {
  console.error(
    `Local Cargo dependency versions do not match workspace ${workspaceVersion}:\n${stale
      .map((entry) => `- ${entry}`)
      .join("\n")}`
  );
  process.exit(1);
}

console.log(
  checkOnly
    ? `Local Cargo dependency versions match workspace ${workspaceVersion}.`
    : `Synchronized local Cargo dependency versions to ${dependencyVersion}.`
);
