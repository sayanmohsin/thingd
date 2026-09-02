import fs from "node:fs";
import path from "node:path";

const [version] = process.argv.slice(2);
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  throw new Error(`Expected a SemVer release version, received: ${version ?? ""}`);
}

const root = process.cwd();
const manifestPath = path.join(root, "crates/thingd/Cargo.toml");
let manifest = fs.readFileSync(manifestPath, "utf8");
const sourceRoot = path.join(root, "crates/thingdb/src");
const publishRoot = path.join(root, "crates/thingd/src");
const workspaceDependency = manifest.match(
  /thingdb = \{ path = "\.\.\/thingdb", version = "[^"]+", optional = true \}/,
)?.[0];
const persistentFeature = manifest.match(/^persistent = \[([^\n]*)\]$/m);
const thingdbFeature = manifest.match(/^thingdb-backend = \[[^\n]*\]\n?/m)?.[0];

if (!workspaceDependency || !persistentFeature || !thingdbFeature) {
  throw new Error("Expected the ThingDB dependency and backend feature entries");
}

const persistentEntries = persistentFeature[1]
  .split(",")
  .map((entry) => entry.trim())
  .filter((entry) => entry !== '"thingdb-backend"');
if (persistentEntries.length === 0) {
  throw new Error("The publish manifest must retain at least one persistent backend");
}

manifest = manifest
  .replace(persistentFeature[0], `persistent = [${persistentEntries.join(", ")}]`)
  .replace(`\n${thingdbFeature}`, "\n")
  .replace(`\n${workspaceDependency}`, "");
if (!manifest.includes('crc32fast = "1.4"')) {
  manifest = manifest.replace(
    "[dependencies]\n",
    '[dependencies]\ncrc32fast = "1.4"\n',
  );
}
fs.writeFileSync(manifestPath, manifest);

const thingdLibPath = path.join(publishRoot, "lib.rs");
const thingdLib = fs.readFileSync(thingdLibPath, "utf8");
if (!thingdLib.includes('#[path = "thingdb.rs"]')) {
  const storageModule = '#[cfg(feature = "persistent-engine")]\nmod storage_backend;';
  if (!thingdLib.includes(storageModule)) {
    throw new Error("Expected the persistent storage module declaration");
  }
  fs.writeFileSync(
    thingdLibPath,
    thingdLib.replace(
      storageModule,
      `${storageModule}\n#[cfg(feature = "persistent-engine")]\n#[path = "thingdb.rs"]\nmod thingdb;`,
    ),
  );
}

for (const file of ["persistent.rs", "storage_backend.rs"]) {
  const filePath = path.join(publishRoot, file);
  const source = fs.readFileSync(filePath, "utf8");
  fs.writeFileSync(filePath, source.replaceAll("thingdb::", "crate::thingdb::"));
}

const thingdbLib = fs
  .readFileSync(path.join(sourceRoot, "lib.rs"), "utf8")
  .replace("mod cache;", '#[path = "thingdb_cache.rs"]\nmod cache;');
const thingdbCache = fs
  .readFileSync(path.join(sourceRoot, "cache.rs"), "utf8")
  .replace("use crate::{Error, Result};", "use super::{Error, Result};");
fs.writeFileSync(path.join(publishRoot, "thingdb.rs"), thingdbLib);
fs.writeFileSync(path.join(publishRoot, "thingdb_cache.rs"), thingdbCache);

console.log(`Prepared crates/thingd with private ThingDB sources for ${version}`);
