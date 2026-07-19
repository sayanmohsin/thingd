import assert from "node:assert/strict";
import { existsSync, rmSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import { runCli } from "../dist/index.js";

const exportObjPath = resolve("test-objects.jsonl");
const exportEvPath = resolve("test-events.jsonl");
const snapshotPath = resolve("test-snapshot.json");

function makeEnv(label) {
  const dbPath = join(tmpdir(), `thingd-cli-c-${label}.db`);
  return {
    env: { THINGD_PATH: dbPath, THINGD_DRIVER: "native" },
    cleanup: () => {
      for (const file of [dbPath, `${dbPath}-wal`, `${dbPath}-shm`]) {
        if (existsSync(file)) {
          try { rmSync(file, { recursive: true, force: true }); } catch {}
        }
      }
    },
  };
}

test("thingd export, import and redact work correctly", { skip: "Fjall single-process: needs close() support" }, async () => {
  const { env, cleanup } = makeEnv("export-import");
  const run = (args) => runCli(args, {
    env, stdout: { write: (c) => {} }, stderr: { write: (c) => {} },
  });
  try { cleanup(); } catch {}
  
  // 1. Put object containing sensitive data
  const put1 = await run(["objects", "put", "credentials", "cred-1", "--data",
    '{"secretToken":"super-secret-123","apiKey":"sk-abcdefghijklmnopqrstuvwxyz12","publicName":"Acme","text":"Contact us at support@acme.com"}',
  ]);
  assert.equal(put1.code, 0);

  // 2. Export with default redaction
  const exp1 = await run(["export", "--collection", "credentials", "--out", exportObjPath, "--redact"]);
  assert.equal(exp1.code, 0);
  assert.ok(existsSync(exportObjPath));

  const content = readFileSync(exportObjPath, "utf8");
  const parsed = JSON.parse(content.trim());
  
  assert.equal(parsed.id, "cred-1");
  assert.equal(parsed.secretToken, "[REDACTED]");
  assert.equal(parsed.apiKey, "[REDACTED]");
  assert.equal(parsed.publicName, "Acme");
  assert.equal(parsed.text, "Contact us at [REDACTED_EMAIL]");

  // 3. Export without redaction
  const exp2 = await run(["export", "--collection", "credentials", "--out", exportObjPath]);
  assert.equal(exp2.code, 0);
  const contentUnredacted = readFileSync(exportObjPath, "utf8");
  const parsedUnredacted = JSON.parse(contentUnredacted.trim());
  assert.equal(parsedUnredacted.secretToken, "super-secret-123");
  assert.equal(parsedUnredacted.apiKey, "sk-abcdefghijklmnopqrstuvwxyz12");

  // 4. Import exported unredacted data to a new collection
  const imp = await run(["import", "--collection", "credentials_restored", "--in", exportObjPath]);
  assert.equal(imp.code, 0);

  const getRes = await run(["objects", "get", "credentials_restored", "cred-1"]);
  assert.equal(getRes.code, 0);
  const importedObj = JSON.parse(getRes.stdout);
  assert.equal(importedObj.id, "cred-1");
  assert.equal(importedObj.secretToken, "super-secret-123");
  cleanup();
});

test("thingd export events works", { skip: "Fjall single-process: needs close() support" }, async () => {
  const { env, cleanup } = makeEnv("export-events");
  const run = (args) => runCli(args, {
    env, stdout: { write: (c) => {} }, stderr: { write: (c) => {} },
  });
  try { cleanup(); } catch {}

  // 1. Append event
  const appendRes = await run(["events", "append", "audit-trail", "login", "--text",
    "User logged in from test@acme.com",
  ]);
  assert.equal(appendRes.code, 0);

  // 2. Export events
  const expRes = await run(["export", "--events", "--out", exportEvPath, "--redact"]);
  assert.equal(expRes.code, 0);
  assert.ok(existsSync(exportEvPath));

  const content = readFileSync(exportEvPath, "utf8");
  const lines = content.trim().split("\n");
  assert.ok(lines.length >= 1);
  const parsed = JSON.parse(lines[0]);
  assert.equal(parsed.stream, "audit-trail");
  assert.equal(parsed.type, "login");
  assert.equal(parsed.text, "User logged in from [REDACTED_EMAIL]");
  cleanup();
});

test("thingd snapshot create and restore works", { skip: "Fjall single-process: needs close() support" }, async () => {
  const { env, cleanup } = makeEnv("snapshot");
  const run = (args) => runCli(args, {
    env, stdout: { write: (c) => {} }, stderr: { write: (c) => {} },
  });
  try { cleanup(); } catch {}

  // 1. Put an object and append an event
  const put = await run(["objects", "put", "users", "user-1", "--text", "Original User"]);
  assert.equal(put.code, 0);

  const append = await run(["events", "append", "system-log", "init", "--text", "System Initialized"]);
  assert.equal(append.code, 0);

  // 2. Create snapshot
  const snapCreate = await run(["snapshot", "create", "--out", snapshotPath]);
  assert.equal(snapCreate.code, 0);
  assert.ok(existsSync(snapshotPath));

  const snapContent = JSON.parse(readFileSync(snapshotPath, "utf8"));
  assert.ok(snapContent.collections.users);
  assert.equal(snapContent.collections.users[0].id, "user-1");
  assert.ok(snapContent.events.some(e => e.stream === "system-log"));

  // 3. Mutate DB (delete user-1 and append new event)
  const del = await run(["objects", "delete", "users", "user-1"]);
  assert.equal(del.code, 0);

  // 4. Restore snapshot
  const snapRestore = await run(["snapshot", "restore", "--in", snapshotPath]);
  assert.equal(snapRestore.code, 0);

  // 5. Verify restored state
  const getRestored = await run(["objects", "get", "users", "user-1"]);
  assert.equal(getRestored.code, 0);
  const restoredObj = JSON.parse(getRestored.stdout);
  assert.equal(restoredObj.id, "user-1");
  assert.equal(restoredObj.text, "Original User");
  cleanup();
});
