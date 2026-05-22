import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(packageRoot, "../..");
const targetRoot = resolve(repoRoot, "target/release");
const outputPath = resolve(packageRoot, "dist/thingd_native.node");

const candidates = [
  "libthingd_native.dylib",
  "libthingd_native.so",
  "thingd_native.dll",
].map((fileName) => resolve(targetRoot, fileName));

const inputPath = candidates.find((candidate) => existsSync(candidate));

if (!inputPath) {
  throw new Error(`Could not find native thingd library in ${targetRoot}`);
}

mkdirSync(dirname(outputPath), { recursive: true });
copyFileSync(inputPath, outputPath);
console.log(`Copied ${inputPath} -> ${outputPath}`);
