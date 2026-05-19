import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(packageRoot, "../..");
const targetRoot = resolve(repoRoot, "target/release");
const outputPath = resolve(packageRoot, "dist/memoryd_native.node");

const candidates = [
  "libmemoryd_native.dylib",
  "libmemoryd_native.so",
  "memoryd_native.dll",
].map((fileName) => resolve(targetRoot, fileName));

const inputPath = candidates.find((candidate) => existsSync(candidate));

if (!inputPath) {
  throw new Error(`Could not find native memoryd library in ${targetRoot}`);
}

mkdirSync(dirname(outputPath), { recursive: true });
copyFileSync(inputPath, outputPath);
console.log(`Copied ${inputPath} -> ${outputPath}`);
