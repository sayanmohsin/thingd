import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(packageRoot, "../..");
const nativeTarget = process.env.NATIVE_TARGET;
const targetRoot = resolve(repoRoot, nativeTarget ? `target/${nativeTarget}/release` : "target/release");

const platform = process.env.NATIVE_PLATFORM ?? process.platform;
const arch = process.env.NATIVE_ARCH ?? process.arch;
const outputPath = resolve(packageRoot, `prebuilds/${platform}-${arch}/thingd_native.node`);

const candidates = [
  "libthingd_native.dylib",
  "libthingd_native.so",
  "thingd_native.dll",
].map((fileName) => resolve(targetRoot, fileName));

const inputPath = candidates.find((candidate) => existsSync(candidate));

if (!inputPath) {
  throw new Error(`Could not find native thingd library in ${targetRoot}. Did you run "cargo build --release"?`);
}

mkdirSync(dirname(outputPath), { recursive: true });
copyFileSync(inputPath, outputPath);
console.log(`Copied prebuild: ${inputPath} -> ${outputPath}`);

if (platform === "darwin") {
  import("node:child_process").then(({ execSync }) => {
    try {
      execSync(`codesign -s - "${outputPath}"`);
      console.log(`Ad-hoc signed prebuild ${outputPath}`);
    } catch (err) {
      console.warn("Failed to ad-hoc sign the prebuild binary:", err.message);
    }
  });
}
