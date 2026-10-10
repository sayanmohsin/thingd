import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptPath = fileURLToPath(import.meta.url);
const packageRoot = resolve(dirname(scriptPath), "..");
const repositoryRoot = resolve(packageRoot, "../..");

function macOSMajorVersion(version) {
  const major = Number.parseInt(version.split(".")[0] ?? "", 10);
  return Number.isInteger(major) ? major : undefined;
}

export function nativeBuildEnvironment({ platform, version, environment }) {
  const majorVersion = macOSMajorVersion(version);
  if (
    platform !== "darwin" ||
    majorVersion === undefined ||
    majorVersion < 26 ||
    environment.MACOSX_DEPLOYMENT_TARGET !== undefined
  ) {
    return environment;
  }

  // Rust's current macOS strip pipeline emits a misaligned LINKEDIT string
  // pool for macOS 26+; dyld rejects the addon. Targeting 11.0 uses the
  // compatible Mach-O layout until rust-lang/rust#157750 is fixed.
  return { ...environment, MACOSX_DEPLOYMENT_TARGET: "11.0" };
}

function detectMacOSVersion() {
  const result = spawnSync("sw_vers", ["-productVersion"], { encoding: "utf8" });
  return result.status === 0 ? result.stdout.trim() : undefined;
}

function run(command, args, options) {
  const result = spawnSync(command, args, { ...options, stdio: "inherit" });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function build() {
  const version = process.platform === "darwin" ? detectMacOSVersion() : undefined;
  const environment = nativeBuildEnvironment({
    platform: process.platform,
    version: version ?? "",
    environment: process.env,
  });

  if (environment !== process.env) {
    process.stdout.write(
      "Using MACOSX_DEPLOYMENT_TARGET=11.0 to avoid the macOS 26+ native-linker alignment issue.\n"
    );
  }

  run("cargo", ["build", "-p", "thingd-native", "--release"], {
    cwd: repositoryRoot,
    env: environment,
  });
  run(process.execPath, [resolve(packageRoot, "scripts/copy-native.mjs")], {
    cwd: repositoryRoot,
    env: environment,
  });
}

if (process.argv[1] && resolve(process.argv[1]) === scriptPath) {
  build();
}
