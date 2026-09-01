import { existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { env, platform, arch } from "node:process";

const command = (name, args = []) => {
  try {
    return execFileSync(name, args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  } catch {
    return null;
  }
};

const firstLine = (value) => value?.split("\n", 1)[0] ?? null;
const rustVersion = command("rustc", ["-vV"]);
const rustLlvm = rustVersion?.match(/^LLVM version:\s*(.+)$/m)?.[1] ?? null;
const clangVersion = firstLine(command("clang", ["--version"]));
const llvmConfigVersion = command("llvm-config", ["--version"]);
const llvmConfigPrefix = command("llvm-config", ["--prefix"]);
const libclangPath = env.LIBCLANG_PATH ?? null;
const llvmConfigPath = env.LLVM_CONFIG_PATH ?? null;

const report = {
  platform,
  arch,
  rust: firstLine(rustVersion),
  rustLlvm,
  clang: clangVersion,
  llvmConfig: llvmConfigVersion,
  llvmConfigPrefix,
  libclangPath,
  llvmConfigPath,
  rocksdbBuild: {
    requiredFor: "rocksdb-backend and the compatibility persistent feature",
    nativeCxx: true,
    bindgen: true,
  },
  environment: {
    dyldLibraryPathSet: Boolean(env.DYLD_LIBRARY_PATH),
    libraryPathSet: Boolean(env.LD_LIBRARY_PATH),
  },
};

const warnings = [];
if (!rustVersion) warnings.push("rustc was not found on PATH");
if (!clangVersion) warnings.push("clang was not found on PATH");
if (!llvmConfigVersion) warnings.push("llvm-config was not found on PATH");
if (libclangPath && !existsSync(libclangPath)) {
  warnings.push(`LIBCLANG_PATH does not exist: ${libclangPath}`);
}
if (llvmConfigPath && !existsSync(llvmConfigPath)) {
  warnings.push(`LLVM_CONFIG_PATH does not exist: ${llvmConfigPath}`);
}
if (env.DYLD_LIBRARY_PATH) {
  warnings.push("DYLD_LIBRARY_PATH is set; remove it while invoking rustc to avoid libLLVM conflicts");
}
if (llvmConfigVersion && llvmConfigPrefix && libclangPath && !libclangPath.startsWith(llvmConfigPrefix)) {
  warnings.push("LIBCLANG_PATH and llvm-config resolve to different LLVM installations");
}

if (process.argv.includes("--json")) {
  console.log(JSON.stringify({ ...report, warnings }, null, 2));
} else {
  console.log("Thingd native toolchain diagnostics");
  console.log(JSON.stringify(report, null, 2));
  if (warnings.length) {
    console.warn("Warnings:");
    for (const warning of warnings) console.warn(`- ${warning}`);
    console.warn("These diagnostics are advisory; Rust builds using RocksDB still require a compatible native toolchain.");
  } else {
    console.log("No native toolchain conflicts detected.");
  }
}

if (process.argv.includes("--strict") && warnings.length) process.exitCode = 1;
