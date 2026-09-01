import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("./sync-cargo-local-dependency-versions.mjs", import.meta.url));

test("synchronizes path dependency versions with trailing fields", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "thingd-cargo-version-sync-"));
  try {
    await writeFile(path.join(root, "Cargo.toml"), 'version = "0.86.0"\n');
    await Promise.all([
      mkdir(path.join(root, "crates", "thingd"), { recursive: true }),
      mkdir(path.join(root, "packages", "native"), { recursive: true }),
    ]);
    await writeFile(
      path.join(root, "crates", "thingd", "Cargo.toml"),
      '[dependencies]\nthingdb = { path = "../thingdb", version = "0.85", optional = true }\n',
    );
    await writeFile(
      path.join(root, "packages", "native", "Cargo.toml"),
      '[dependencies]\nthingd = { path = "../../crates/thingd", version = "0.85", features = ["persistent"] }\n',
    );

    execFileSync(process.execPath, [script], { cwd: root, stdio: "pipe" });
    const thingd = await readFile(path.join(root, "crates", "thingd", "Cargo.toml"), "utf8");
    const native = await readFile(path.join(root, "packages", "native", "Cargo.toml"), "utf8");
    assert.match(thingd, /version = "0\.86"/);
    assert.match(native, /version = "0\.86"/);
    execFileSync(process.execPath, [script, "--check"], { cwd: root, stdio: "pipe" });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
