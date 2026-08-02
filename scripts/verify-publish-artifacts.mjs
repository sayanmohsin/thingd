import { join, resolve } from "node:path";
import { cwd } from "node:process";
import spawn from "cross-spawn";

const rootDir = resolve(cwd());
const requiredNativePrebuilds = Number.parseInt(process.env.REQUIRED_NATIVE_PREBUILDS ?? "1", 10);

function pack(packageDir) {
  const result = spawn.sync("pnpm", ["pack", "--dry-run", "--json"], {
    cwd: join(rootDir, packageDir),
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });

  if (result.status !== 0) {
    throw new Error(result.stderr || `pnpm pack failed for ${packageDir}`);
  }

  const jsonStart = result.stdout.indexOf("{");
  if (jsonStart === -1) {
    throw new Error(`pnpm pack did not return JSON for ${packageDir}: ${result.stdout}`);
  }
  const output = JSON.parse(result.stdout.slice(jsonStart));
  return new Set((Array.isArray(output) ? output[0] : output).files.map(({ path }) => path));
}

function assertFiles(packageName, files, requiredFiles) {
  for (const file of requiredFiles) {
    if (!files.has(file)) {
      throw new Error(`${packageName} tarball is missing ${file}`);
    }
  }
}

const sdkFiles = pack("packages/thingd");
assertFiles("@thingd/sdk", sdkFiles, [
  "dist/index.js",
  "dist/index.d.ts",
  "dist/client/index.js",
  "dist/memory/index.js",
  "dist/types/index.js",
]);

const nativeFiles = pack("packages/thingd-native");
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
