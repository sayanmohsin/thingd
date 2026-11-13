import { spawnSync } from "node:child_process";
import { mkdirSync, copyFileSync, existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const root = resolve(packageRoot, "../..");
const build = spawnSync("cargo", ["build", "-p", "thingdb-node", "--release"], {
  cwd: root,
  stdio: "inherit",
});
if (build.status !== 0) {
  process.exit(build.status ?? 1);
}
const extension = process.platform === "win32" ? "dll" : process.platform === "darwin" ? "dylib" : "so";
const prefix = process.platform === "win32" ? "" : "lib";
const source = resolve(root, `target/release/${prefix}thingdb_node.${extension}`);
const destination = resolve(packageRoot, "dist/thingdb_node.node");
if (!existsSync(source)) {
  throw new Error(`Native ThingDB library not found at ${source}`);
}
mkdirSync(dirname(destination), { recursive: true });
copyFileSync(source, destination);
console.log(`Built @thingd/thingdb native addon at ${destination}`);
