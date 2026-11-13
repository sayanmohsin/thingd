import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const root = resolve(packageRoot, "../..");
const target = process.env.NATIVE_TARGET;
const targetRoot = resolve(root, target ? `target/${target}/release` : "target/release");
const platform = process.env.NATIVE_PLATFORM ?? process.platform;
const arch = process.env.NATIVE_ARCH ?? process.arch;
const source = resolve(
  targetRoot,
  `${platform === "win32" ? "" : "lib"}thingdb_node.${platform === "win32" ? "dll" : platform === "darwin" ? "dylib" : "so"}`,
);
const destination = resolve(packageRoot, `prebuilds/${platform}-${arch}/thingdb_node.node`);
if (!existsSync(source)) {
  throw new Error(`Native ThingDB library not found at ${source}`);
}
mkdirSync(dirname(destination), { recursive: true });
copyFileSync(source, destination);
