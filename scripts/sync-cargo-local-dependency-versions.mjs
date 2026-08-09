import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const checkOnly = process.argv.includes("--check");
const workspace = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8");
const version = workspace.match(/^version = "(\d+\.\d+)\.\d+"/m)?.[1];
if (!version) {
  throw new Error("Could not determine the Cargo workspace major/minor version");
}

const files = ["crates/thingd-server/Cargo.toml", "packages/thingd-native/Cargo.toml"];
const stale = [];
for (const file of files) {
  const absolute = path.join(root, file);
  const source = fs.readFileSync(absolute, "utf8");
  const updated = source.replace(
    /(path\s*=\s*"[^"]+"\s*,\s*version\s*=\s*")[^"]+("[^\n]*)/g,
    `$1${version}$2`,
  );
  if (source !== updated) {
    stale.push(file);
    if (!checkOnly) {
      fs.writeFileSync(absolute, updated);
    }
  }
}

if (checkOnly && stale.length > 0) {
  throw new Error(`Stale local Cargo dependency versions: ${stale.join(", ")}`);
}
console.log(checkOnly ? `Cargo dependency versions match ${version}` : `Synchronized Cargo dependencies to ${version}`);
