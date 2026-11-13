import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, copyFileSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

const root = process.cwd();
const build = spawnSync("cargo", ["build", "-p", "thingdb-node"], { cwd: root, stdio: "inherit" });
if (build.status !== 0) {
  process.exit(build.status ?? 1);
}

const packageRoot = path.join(root, "packages/thingdb-node");
const extension = process.platform === "win32" ? "dll" : process.platform === "darwin" ? "dylib" : "so";
const prefix = process.platform === "win32" ? "" : "lib";
const source = path.join(root, `target/debug/${prefix}thingdb_node.${extension}`);
const dist = path.join(packageRoot, "dist");
mkdirSync(dist, { recursive: true });
copyFileSync(source, path.join(dist, "thingdb_node.node"));

const { ThingDB } = await import(pathToFileURL(path.join(packageRoot, "index.js")));
const temp = path.join(os.tmpdir(), `thingdb-node-smoke-${process.pid}`);
rmSync(temp, { recursive: true, force: true });
try {
  const db = new ThingDB(temp);
  const users = db.keyspace("users");
  await users.put(new Uint8Array([97, 108, 105, 99, 101]), new Uint8Array([111, 110, 101]));
  assert.equal(Buffer.from(await users.get(new Uint8Array([97, 108, 105, 99, 101]))).toString(), "one");
  await db.putMany([
    { keyspace: "users", key: Buffer.from("bob"), value: Buffer.from("two") },
    { keyspace: "users", key: Buffer.from("alice"), delete: true },
  ]);
  assert.equal(await users.get(Buffer.from("alice")), null);
  assert.equal(Buffer.from(await users.get(Buffer.from("bob"))).toString(), "two");
  const entries = await users.prefix(Buffer.from("b"));
  assert.equal(entries.length, 1);
  const snapshot = await db.snapshot();
  assert.equal(Buffer.from(snapshot.get("users", Buffer.from("bob"))).toString(), "two");
  await db.persist();
  await db.compact();
  db.close();
  assert.throws(() => db.keyspace("closed"), /closed/);
} finally {
  rmSync(temp, { recursive: true, force: true });
  rmSync(dist, { recursive: true, force: true });
}
console.log("Validated @thingd/thingdb async API against a local durable database");
