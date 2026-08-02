import fs from "node:fs";
import path from "node:path";

const [version] = process.argv.slice(2);
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  throw new Error(`Expected a SemVer release version, received: ${version ?? "(missing)"}`);
}

const root = process.cwd();
const majorMinor = version.split(".").slice(0, 2).join(".");

function replaceRequired(file, pattern, replacement) {
  const absolute = path.join(root, file);
  const source = fs.readFileSync(absolute, "utf8");
  const updated = source.replace(pattern, replacement);
  if (source === updated) {
    throw new Error(`Expected release version pattern was not found in ${file}`);
  }
  fs.writeFileSync(absolute, updated);
}

function replaceIfPresent(file, pattern, replacement) {
  const absolute = path.join(root, file);
  const source = fs.readFileSync(absolute, "utf8");
  const updated = source.replace(pattern, replacement);
  if (source !== updated) {
    fs.writeFileSync(absolute, updated);
  }
}

function updateThingdDependencyVersion(file) {
  const absolute = path.join(root, file);
  const source = fs.readFileSync(absolute, "utf8");
  const lines = source.split("\n");
  const start = lines.findIndex((line) => /^\s*thingd\s*=/.test(line));
  if (start === -1) {
    return;
  }

  let end = start;
  while (end < lines.length && !lines[end].includes("}")) {
    end += 1;
  }

  const block = lines.slice(start, end + 1).join("\n");
  const updatedBlock = block.replace(/(\bversion\s*=\s*)"[^"]+"/, `$1"${majorMinor}"`);
  if (block !== updatedBlock) {
    lines.splice(start, end - start + 1, ...updatedBlock.split("\n"));
    fs.writeFileSync(absolute, lines.join("\n"));
  }
}

replaceRequired("Cargo.toml", /^version = ".*"$/m, `version = "${version}"`);
replaceRequired(
  "packages/thingd/src/version.ts",
  /SDK_VERSION = ".*"/,
  `SDK_VERSION = "${version}"`,
);

for (const file of ["README.md", "crates/thingd/README.md"]) {
  replaceIfPresent(file, /version = "\d+\.\d+"/g, `version = "${majorMinor}"`);
}

for (const file of ["crates/thingd-server/Cargo.toml", "packages/thingd-native/Cargo.toml"]) {
  updateThingdDependencyVersion(file);
}

for (const file of [
  "packages/thingd/package.json",
  "packages/thingd-cli/package.json",
  "packages/thingd-native/package.json",
  "packages/thingd-client/package.json",
]) {
  const absolute = path.join(root, file);
  const packageJson = JSON.parse(fs.readFileSync(absolute, "utf8"));
  packageJson.version = version;
  fs.writeFileSync(absolute, `${JSON.stringify(packageJson, null, 2)}\n`);
}

for (const [file, dependency] of [
  ["packages/thingd/package.json", "@thingd/native"],
  ["packages/thingd-cli/package.json", "@thingd/sdk"],
]) {
  replaceRequired(
    file,
    new RegExp(`("${dependency}"\\s*:\\s*)"[^"]+"`),
    `$1"^${version}"`,
  );
}
