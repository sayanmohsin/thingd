import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const __dirname = dirname(fileURLToPath(import.meta.url));

function resolveBinding() {
  // 1. Try local dev build first (dist/thingd_native.node)
  const devPath = join(__dirname, "dist", "thingd_native.node");
  if (existsSync(devPath)) {
    return { binding: require(devPath), path: devPath };
  }

  // 2. Try prebuilt binary matching current platform and arch
  const platform = process.platform;
  const arch = process.arch;
  const prebuiltPath = join(__dirname, "prebuilds", `${platform}-${arch}`, "thingd_native.node");
  if (existsSync(prebuiltPath)) {
    return { binding: require(prebuiltPath), path: prebuiltPath };
  }

  // A source build is intentionally not attempted at runtime. Published
  // packages carry prebuilds; falling back to a native compilation here would
  // produce opaque libclang/RocksDB failures for consumers.
  const message =
    `No prebuilt @thingd/native binary is available for ${platform}-${arch}. ` +
    "Install a supported package target or build the native addon with the Thingd native toolchain.";
  const error = new Error(message);
  error.code = "THINGD_NATIVE_PREBUILD_MISSING";
  throw error;
}

const { binding, path } = resolveBinding();

export const { NativeThingStore, parseSchema } = binding;
export const loadedPath = path;
