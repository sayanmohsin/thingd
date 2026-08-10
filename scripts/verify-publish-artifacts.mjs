import fs from "node:fs";
import { join, resolve } from "node:path";
import { cwd } from "node:process";
import { tmpdir } from "node:os";
import spawn from "cross-spawn";
import { publishPackages, validatePublishManifest } from "./publish-manifest.mjs";

const rootDir = resolve(cwd());
const requiredNativePrebuilds = Number.parseInt(process.env.REQUIRED_NATIVE_PREBUILDS ?? "1", 10);
const expectedVersion = process.env.RELEASE_VERSION ?? JSON.parse(fs.readFileSync(join(rootDir, "package.json"), "utf8")).version;

function pack(packageDir) {
  const result = spawn.sync("npm", ["pack", "--json", "--ignore-scripts"], {
    cwd: join(rootDir, packageDir),
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });

  if (result.status !== 0) {
    throw new Error(result.stderr || `pnpm pack failed for ${packageDir}`);
  }

  const jsonStarts = [result.stdout.indexOf("["), result.stdout.indexOf("{")].filter((index) => index >= 0);
  if (jsonStarts.length === 0) {
    throw new Error(`pnpm pack did not return JSON for ${packageDir}: ${result.stdout}`);
  }
  const output = JSON.parse(result.stdout.slice(Math.min(...jsonStarts)).trim());
  const metadata = Array.isArray(output) ? output[0] : output;
  const tarball = join(rootDir, packageDir, metadata.filename);
  const manifest = JSON.parse(
    spawn.sync("tar", ["-xOf", tarball, "package/package.json"], { encoding: "utf8" }).stdout,
  );
  return {
    files: new Set(metadata.files.map(({ path }) => path)),
    manifest,
    tarball,
  };
}

function assertFiles(packageName, files, requiredFiles) {
  for (const file of requiredFiles) {
    if (!files.has(file)) {
      throw new Error(`${packageName} tarball is missing ${file}`);
    }
  }
}

const packed = new Map(publishPackages.map((pkg) => [pkg.name, { ...pkg, ...pack(pkg.path) }]));

for (const pkg of publishPackages) {
  const artifact = packed.get(pkg.name);
  const errors = validatePublishManifest(artifact.manifest, expectedVersion);
  if (errors.length > 0) {
    throw new Error(`${pkg.name} publish manifest is invalid:\n- ${errors.join("\n- ")}`);
  }
}

const sdkFiles = packed.get("@thingd/sdk").files;
assertFiles("@thingd/sdk", sdkFiles, [
  "dist/index.js",
  "dist/index.d.ts",
  "dist/client/index.js",
  "dist/memory/index.js",
  "dist/types/index.js",
]);

const nativeFiles = packed.get("@thingd/native").files;
assertFiles("@thingd/native", nativeFiles, ["index.js", "index.d.ts"]);
const nativePrebuilds = [...nativeFiles].filter(
  (file) => file.startsWith("prebuilds/") && file.endsWith("/thingd_native.node"),
);
if (nativePrebuilds.length < requiredNativePrebuilds) {
  throw new Error(
    `@thingd/native tarball contains ${nativePrebuilds.length} prebuild(s); expected at least ${requiredNativePrebuilds}`,
  );
}

console.log(
  `Validated publish artifacts: @thingd/sdk (${sdkFiles.size} files), @thingd/native (${nativePrebuilds.length} prebuilds)`,
);

const smokeDir = fs.mkdtempSync(join(tmpdir(), "thingd-publish-smoke-"));
try {
  const install = spawn.sync(
    "npm",
    [
      "install",
      "--ignore-scripts",
      "--no-audit",
      "--no-fund",
      "--no-save",
      ...publishPackages.map((pkg) => packed.get(pkg.name).tarball),
    ],
    { cwd: smokeDir, encoding: "utf8", stdio: "inherit" },
  );
  if (install.status !== 0) {
    throw new Error("Clean package installation failed");
  }
  const importCheck = spawn.sync("node", ["--input-type=module", "-e", "await import('@thingd/cli')"], {
    cwd: smokeDir,
    encoding: "utf8",
    stdio: "inherit",
  });
  if (importCheck.status !== 0) {
    throw new Error("Clean @thingd/cli import failed");
  }
  console.log("Validated clean installation and import of @thingd/cli");
} finally {
  for (const artifact of packed.values()) {
    fs.rmSync(artifact.tarball, { force: true });
  }
  fs.rmSync(smokeDir, { recursive: true, force: true });
}
