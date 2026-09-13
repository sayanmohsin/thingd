import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const root = process.cwd();
const metadataResult = spawnSync("cargo", ["metadata", "--no-deps", "--format-version", "1"], {
  cwd: root,
  encoding: "utf8",
});
if (metadataResult.status !== 0) {
  throw new Error(metadataResult.stderr || "cargo metadata failed");
}
const metadata = JSON.parse(metadataResult.stdout);
const thingdbPackage = metadata.packages.find((entry) => entry.name === "thingdb");
if (!thingdbPackage) {
  throw new Error("Could not find the thingdb package metadata");
}

const packageResult = spawnSync("cargo", ["package", "-p", "thingdb", "--allow-dirty"], {
  cwd: root,
  encoding: "utf8",
  stdio: "inherit",
});
if (packageResult.status !== 0) {
  throw new Error("cargo package -p thingdb failed");
}

const tarball = path.join(root, "target", "package", `thingdb-${thingdbPackage.version}.crate`);
if (!fs.existsSync(tarball)) {
  throw new Error(`Expected package archive ${tarball}`);
}

const smokeRoot = fs.mkdtempSync(path.join(os.tmpdir(), "thingdb-package-smoke-"));
const sourceRoot = path.join(smokeRoot, "thingdb");
const consumerRoot = path.join(smokeRoot, "consumer");
fs.mkdirSync(sourceRoot, { recursive: true });
fs.mkdirSync(path.join(consumerRoot, "src"), { recursive: true });

const extract = spawnSync("tar", ["-xzf", tarball, "-C", sourceRoot, "--strip-components=1"], {
  encoding: "utf8",
});
if (extract.status !== 0) {
  throw new Error(extract.stderr || "Could not extract the ThingDB package archive");
}

fs.writeFileSync(
  path.join(consumerRoot, "Cargo.toml"),
  `[package]\nname = "thingdb-package-consumer"\nversion = "0.0.0"\nedition = "2024"\n\n[dependencies]\nthingdb = { path = "../thingdb" }\n`,
);
fs.writeFileSync(
  path.join(consumerRoot, "src", "main.rs"),
  `use std::time::Duration;

use thingdb::{CacheOptions, Database, KeyspaceCreateOptions, PersistMode, MemoryCache};

fn main() -> thingdb::Result<()> {
    let memory = Database::open(":memory:")?;
    let users = memory.keyspace("users", KeyspaceCreateOptions::default)?;
    users.insert(b"alice", b"one")?;
    memory.batch().put(&users, b"bob", b"two").delete(&users, b"alice").commit()?;
    assert_eq!(users.get(b"bob")?, Some(b"two".to_vec()));
    assert!(users.prefix(b"b").next().is_some());
    assert_eq!(memory.snapshot()?.keyspace("users").get(b"bob"), Some(b"two".to_vec()));

    let path = std::env::temp_dir().join(format!("thingdb-package-consumer-{}", std::process::id()));
    let durable = Database::open(&path)?;
    let records = durable.keyspace("records", KeyspaceCreateOptions::default)?;
    records.insert(b"one", b"value")?;
    durable.persist(PersistMode::SyncAll)?;
    drop(records);
    drop(durable);
    let _ = std::fs::remove_dir_all(&path);

    let cache = MemoryCache::new(CacheOptions::default())?;
    cache.insert_with_ttl(b"key", b"value", Duration::from_secs(1))?;
    assert_eq!(cache.get(b"key")?, Some(b"value".to_vec()));
    Ok(())
}
`,
);

const tree = spawnSync("cargo", ["tree", "--manifest-path", path.join(consumerRoot, "Cargo.toml")], {
  cwd: consumerRoot,
  encoding: "utf8",
});
if (tree.status !== 0) {
  throw new Error(tree.stderr || "cargo tree failed for the standalone consumer");
}
for (const forbidden of ["rocksdb", "librocksdb-sys", "bindgen", "libclang"]) {
  if (tree.stdout.toLowerCase().includes(forbidden)) {
    throw new Error(`Standalone ThingDB package unexpectedly contains ${forbidden}`);
  }
}

const run = spawnSync("cargo", ["run", "--manifest-path", path.join(consumerRoot, "Cargo.toml"), "--quiet"], {
  cwd: consumerRoot,
  encoding: "utf8",
  stdio: "inherit",
});
if (run.status !== 0) {
  throw new Error("Standalone ThingDB consumer smoke test failed");
}

fs.rmSync(smokeRoot, { recursive: true, force: true });
console.log(`Validated standalone thingdb ${thingdbPackage.version} package and consumer`);
