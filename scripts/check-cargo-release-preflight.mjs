import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const workflow = readFileSync(path.join(root, ".github/workflows/release.yml"), "utf8");
const preflight = workflow.split("  release-preflight:")[0];
const expectedPatch = `--config 'patch.crates-io.thingdb.path="crates/thingdb"'`;

assert.ok(
  preflight.includes(expectedPatch),
  "Thingd crate preflight must patch crates.io ThingDB to the checked-out local crate",
);

const fixture = mkdtempSync(path.join(os.tmpdir(), "thingd-cargo-release-preflight-"));
try {
  const thingdb = path.join(fixture, "thingdb");
  const thingd = path.join(fixture, "thingd");
  mkdirSync(path.join(thingdb, "src"), { recursive: true });
  mkdirSync(path.join(thingd, "src"), { recursive: true });
  writeFileSync(
    path.join(fixture, "Cargo.toml"),
    '[workspace]\nmembers = ["thingdb", "thingd"]\nresolver = "2"\n',
  );
  writeFileSync(
    path.join(thingdb, "Cargo.toml"),
    '[package]\nname = "thingdb"\nversion = "0.92.1"\nedition = "2024"\n',
  );
  writeFileSync(path.join(thingdb, "src/lib.rs"), "pub fn version() -> &'static str { \"local\" }\n");
  writeFileSync(
    path.join(thingd, "Cargo.toml"),
    '[package]\nname = "thingd-release-fixture"\nversion = "0.92.1"\nedition = "2024"\ndescription = "Cargo release preflight fixture"\nlicense = "Apache-2.0"\n\n[dependencies]\nthingdb = { version = "0.92.1" }\n',
  );
  writeFileSync(
    path.join(thingd, "src/main.rs"),
    'fn main() { assert_eq!(thingdb::version(), "local"); }\n',
  );

  const patch = 'patch.crates-io.thingdb.path="thingdb"';
  const check = spawnSync(
    "cargo",
    [
      "package",
      "--no-verify",
      "--offline",
      "--allow-dirty",
      "--manifest-path",
      path.join(thingd, "Cargo.toml"),
      "--config",
      patch,
    ],
    { cwd: fixture, encoding: "utf8", env: { ...process.env, CARGO_NET_OFFLINE: "true" } },
  );
  if (check.status !== 0) {
    throw new Error(check.stderr || check.stdout || "Cargo local ThingDB patch smoke test failed");
  }

  const archive = path.join(fixture, "target/package/thingd-release-fixture-0.92.1.crate");
  const packagedManifest = spawnSync(
    "tar",
    ["-xOf", archive, "thingd-release-fixture-0.92.1/Cargo.toml"],
    { encoding: "utf8" },
  );
  assert.equal(packagedManifest.status, 0, "Cargo should produce a package manifest for validation");
  const thingdbDependency = packagedManifest.stdout.match(
    /\[dependencies\.thingdb\]\n([\s\S]*?)(?=\n\[|$)/,
  )?.[1];
  assert.ok(thingdbDependency, "Cargo should retain ThingDB as a package dependency");
  assert.match(thingdbDependency, /version = "0\.92\.1"/);
  assert.doesNotMatch(thingdbDependency, /path\s*=/);
} finally {
  rmSync(fixture, { recursive: true, force: true });
}

console.log("Cargo release preflight correctly resolves ThingDB from the local checkout.");
