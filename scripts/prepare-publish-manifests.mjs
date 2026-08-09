import fs from "node:fs";
import path from "node:path";
import {
  normalizeManifest,
  publishPackages,
  readManifest,
  validatePublishManifest,
} from "./publish-manifest.mjs";

const [versionArgument] = process.argv.slice(2);
const root = process.cwd();
const version = versionArgument ?? JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8")).version;

if (!/^\d+\.\d+\.\d+$/.test(version)) {
  throw new Error(`Expected a SemVer release version, received: ${version}`);
}

const majorMinor = version.split(".").slice(0, 2).join(".");
const cargoFiles = ["crates/thingd-server/Cargo.toml", "packages/thingd-native/Cargo.toml"];
const workspaceCargo = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8");
const workspaceVersion = workspaceCargo.match(/^version = "([^"]+)"/m)?.[1];
if (workspaceVersion !== version) {
  throw new Error(`Cargo workspace is ${workspaceVersion}; expected release version ${version}`);
}
for (const cargoFile of cargoFiles) {
  const cargo = fs.readFileSync(path.join(root, cargoFile), "utf8");
  const versions = [...cargo.matchAll(/path\s*=\s*"[^"]+"[^\n]*version\s*=\s*"([^"]+)"/g)].map(
    ([, dependencyVersion]) => dependencyVersion,
  );
  if (versions.some((dependencyVersion) => dependencyVersion !== majorMinor)) {
    throw new Error(`${cargoFile} has local dependency versions ${versions.join(", ")}; expected ${majorMinor}`);
  }
}

for (const pkg of publishPackages) {
  const manifest = readManifest(root, pkg.path);
  if (manifest.version !== version) {
    throw new Error(`${pkg.name} is ${manifest.version}; expected release version ${version}`);
  }
  const normalized = normalizeManifest(manifest, version);
  fs.writeFileSync(path.join(root, pkg.path, "package.json"), `${JSON.stringify(normalized, null, 2)}\n`);
  const errors = validatePublishManifest(normalized, version);
  if (errors.length > 0) {
    throw new Error(`${pkg.name} cannot be published:\n- ${errors.join("\n- ")}`);
  }
}

console.log(`Prepared ${publishPackages.length} npm manifests for ${version}`);
