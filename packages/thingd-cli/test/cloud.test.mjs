import assert from "node:assert/strict";
import { createServer } from "node:http";
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

function listen(server) {
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      server.off("error", reject);
      resolve();
    });
  });
}

function close(server) {
  return new Promise((resolve, reject) => {
    server.close((error) => {
      if (error) {
        reject(error);
        return;
      }
      resolve();
    });
  });
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

test("cloud login with --code and --token verifies against API", async () => {
  const server = createServer((req, res) => {
    res.setHeader("Content-Type", "application/json");
    if (req.url === "/users/me") {
      res.end(
        JSON.stringify({
          user: { id: "1", email: "mock@test.com", name: "Mock User", role: "admin" },
        })
      );
      return;
    }
    res.statusCode = 404;
    res.end(JSON.stringify({ error: "not_found" }));
  });
  await listen(server);

  try {
    const address = server.address();
    assert.ok(address);
    const port = address.port;

    const result = await run(
      ["cloud", "login", "--code", "x", "--token", "mock-token", "--url", `http://127.0.0.1:${port}`]
    );

    assert.equal(result.code, 0);
    assert.match(result.stdout, /mock@test.com/);
  } finally {
    await close(server);
  }
});

test("cloud project list without login shows error", async () => {
  const result = await run(["cloud", "project", "list"]);
  assert.match(result.stderr, /Not logged in/);
});
