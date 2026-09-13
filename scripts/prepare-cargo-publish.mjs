import fs from "node:fs";
import path from "node:path";

const [version] = process.argv.slice(2);
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  throw new Error(`Expected a SemVer release version, received: ${version ?? ""}`);
}

const root = process.cwd();
const manifestPath = path.join(root, "crates/thingd/Cargo.toml");
let manifest = fs.readFileSync(manifestPath, "utf8");
const workspaceDependency = manifest.match(
  /thingdb = \{ path = "\.\.\/thingdb", version = "[^"]+", optional = true \}/,
)?.[0];
const persistentFeature = manifest.match(/^persistent = \[([^\n]*)\]$/m);
const thingdbFeature = manifest.match(/^thingdb-backend = \[[^\n]*\]\n?/m)?.[0];

if (!workspaceDependency || !persistentFeature || !thingdbFeature) {
  throw new Error("Expected the ThingDB workspace dependency and backend feature entries");
}

manifest = manifest.replace(
  workspaceDependency,
  `thingdb = { version = "${version}", optional = true }`,
);
fs.writeFileSync(manifestPath, manifest);
console.log(`Prepared crates/thingd with published ThingDB dependency ${version}`);
