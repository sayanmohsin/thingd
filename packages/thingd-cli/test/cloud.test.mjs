import assert from "node:assert/strict";
import { existsSync, unlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import test from "node:test";
import { runCli } from "../dist/index.js";

const CLOUD_CONFIG_PATH = join(homedir(), ".thingd", "cloud-config.json");

async function run(args, env = {}) {
  let stdout = "";
  let stderr = "";

  const code = await runCli(args, {
    env: { THINGD_PATH: ":memory:", ...env },
    stdout: { write(c) { stdout += c; } },
    stderr: { write(c) { stderr += c; } },
  });

  return { code, stdout, stderr };
}

function removeCloudConfig() {
  try { unlinkSync(CLOUD_CONFIG_PATH); } catch {}
}

function withCloudConfig(token = "test-token", email = "test@example.com") {
  writeFileSync(CLOUD_CONFIG_PATH, JSON.stringify({ token, email }), "utf-8");
}

test.beforeEach(() => removeCloudConfig());
test.afterEach(() => removeCloudConfig());

test("cloud status without config shows not logged in", async () => {
  const result = await run(["cloud", "status"]);
  assert.equal(result.code, 0);
  assert.match(result.stdout, /Not logged in/);
});

test("cloud logout removes config", async () => {
  withCloudConfig();
  const result = await run(["cloud", "logout"]);
  assert.equal(result.code, 0);
  assert.equal(existsSync(CLOUD_CONFIG_PATH), false);
});

test("cloud login with --code shows URL", async () => {
  const result = await run(["cloud", "login", "--code", "test-code"]);
  assert.equal(result.code, 0);
  assert.match(result.stdout, /thingd.cloud/);
});

test("cloud project list without login shows error", async () => {
  const result = await run(["cloud", "project", "list"]);
  assert.match(result.stderr, /Not logged in/);
});
