import { existsSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const requiredFiles = ["index.js", "index.d.ts"];

for (const file of requiredFiles) {
  if (!existsSync(join(packageRoot, file))) {
    throw new Error(`Native package is missing ${file}`);
  }
}

const prebuildRoot = join(packageRoot, "prebuilds");
const prebuilds = existsSync(prebuildRoot)
  ? readdirSync(prebuildRoot, { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .flatMap((entry) => {
        const binary = join(prebuildRoot, entry.name, "thingd_native.node");
        return existsSync(binary) ? [binary] : [];
      })
  : [];

if (prebuilds.length === 0) {
  throw new Error("Native package is missing prebuilds/*/thingd_native.node");
}

console.error(`Validated native package: ${prebuilds.length} prebuild(s)`);
