import fs from "node:fs";
import path from "node:path";

const [version] = process.argv.slice(2);
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  throw new Error(`Expected a SemVer release version, received: ${version ?? ""}`);
}

const root = process.cwd();
const manifestPath = path.join(root, "crates/thingd/Cargo.toml");
const manifest = fs.readFileSync(manifestPath, "utf8");
const majorMinor = version.split(".").slice(0, 2).join(".");
const workspaceDependency = `thingdb = { path = "../thingdb", version = "${majorMinor}", optional = true }`;
const publishedDependency =
  `thingdb = { git = "https://github.com/sayanmohsin/thingd", tag = "thingd-v${version}", package = "thingdb", optional = true }`;

if (!manifest.includes(workspaceDependency)) {
  throw new Error(
    `Expected the workspace ThingDB dependency in ${path.relative(root, manifestPath)}`,
  );
}

fs.writeFileSync(manifestPath, manifest.replace(workspaceDependency, publishedDependency));
console.log(`Prepared crates/thingd/Cargo.toml with ThingDB tag thingd-v${version}`);
