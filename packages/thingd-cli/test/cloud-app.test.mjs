import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { runCli } from "../dist/index.js";

const CLOUD_CONFIG_PATH = join(homedir(), ".thingd", "cloud-config.json");

function removeCloudConfig() {
  try {
    unlinkSync(CLOUD_CONFIG_PATH);
  } catch {
    // Ignore absent test configuration.
  }
}

function withCloudConfig(url) {
  mkdirSync(join(homedir(), ".thingd"), { recursive: true });
  writeFileSync(
    CLOUD_CONFIG_PATH,
    JSON.stringify({ url, userToken: "operator-token", email: "operator@example.com" }),
    "utf8"
  );
}

async function run(args) {
  let stdout = "";
  let stderr = "";
  const code = await runCli(args, {
    env: { THINGD_PATH: ":memory:" },
    stdout: { write: (chunk) => { stdout += chunk; } },
    stderr: { write: (chunk) => { stderr += chunk; } },
  });
  return { code, stdout, stderr };
}

function listen(server) {
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      server.off("error", reject);
      resolve(server.address().port);
    });
  });
}

function close(server) {
  return new Promise((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
}

function json(res, value, status = 200) {
  res.statusCode = status;
  res.setHeader("content-type", "application/json");
  res.end(JSON.stringify(value));
}

test.beforeEach(() => removeCloudConfig());
test.afterEach(() => removeCloudConfig());

test("cloud app init creates a thingd.app/v1 definition without overwriting", async () => {
  const file = join(tmpdir(), `nice-rep-${Date.now()}.app.json`);
  try {
    const result = await run(["cloud", "app", "init", "--file", file, "--name", "Nice Rep", "--slug", "nice-rep"]);
    assert.equal(result.code, 0);
    const definition = JSON.parse(readFileSync(file, "utf8"));
    assert.equal(definition.schemaVersion, "thingd.app/v1");
    assert.equal(definition.slug, "nice-rep");

    const second = await run(["cloud", "app", "init", "--file", file]);
    assert.equal(second.code, 1);
    assert.match(second.stderr, /Refusing to overwrite/);
  } finally {
    try { unlinkSync(file); } catch {}
  }
});

test("cloud app bootstrap dry-run performs no mutation", async () => {
  const calls = [];
  const server = createServer((req, res) => {
    calls.push(`${req.method} ${req.url}`);
    if (req.url === "/projects") {
      json(res, { projects: [{ id: "p1", slug: "nice-rep", name: "Nice Rep" }] });
      return;
    }
    if (req.url === "/projects/p1/instances") {
      json(res, { instances: [{ id: "i1", slug: "nice-rep", name: "Nice Rep", mcpUrl: "https://cloud/mcp" }] });
      return;
    }
    if (req.url === "/projects/p1/publish/apps") {
      json(res, { apps: [] });
      return;
    }
    json(res, { error: "unexpected" }, 404);
  });
  const port = await listen(server);
  const file = join(tmpdir(), `nice-rep-${Date.now()}.app.json`);
  const schema = join(tmpdir(), `nice-rep-${Date.now()}.thingd`);
  try {
    withCloudConfig(`http://127.0.0.1:${port}`);
    const init = await run(["cloud", "app", "init", "--file", file, "--slug", "nice-rep"]);
    assert.equal(init.code, 0);
    writeFileSync(schema, "version 1\ncollection users {\n  id: string @id\n}\n", "utf8");
    const schemaCheck = await run(["schema", "check", schema]);
    assert.equal(schemaCheck.code, 0, schemaCheck.stderr);

    const result = await run([
      "cloud", "app", "bootstrap", "--project", "nice-rep", "--instance", "nice-rep",
      "--file", file, "--schema", schema, "--dry-run", "--publish",
    ]);
    assert.equal(result.code, 0);
    const plan = JSON.parse(result.stdout);
    assert.equal(plan.dryRun, true);
    assert.equal(plan.action, "create");
    assert.deepEqual(plan.mutations, []);
    assert.equal(plan.schema.local, "valid");
    assert.ok(calls.every((call) => !call.startsWith("POST") && !call.startsWith("PUT")));
  } finally {
    await close(server);
    try { unlinkSync(file); } catch {}
    try { unlinkSync(schema); } catch {}
  }
});

test("cloud app config returns only mobile-safe configuration", async () => {
  const server = createServer((req, res) => {
    if (req.url === "/projects") {
      json(res, { projects: [{ id: "p1", slug: "nice-rep", name: "Nice Rep" }] });
      return;
    }
    if (req.url === "/projects/p1/app-config") {
      json(res, { app: { projectId: "p1", publishableKey: "pk_nice_rep" } });
      return;
    }
    json(res, { error: "unexpected" }, 404);
  });
  const port = await listen(server);
  try {
    withCloudConfig(`http://127.0.0.1:${port}`);
    const result = await run(["cloud", "app", "config", "--project", "nice-rep"]);
    assert.equal(result.code, 0);
    const config = JSON.parse(result.stdout);
    assert.equal(config.publishableKey, "pk_nice_rep");
    assert.equal(config.appEndpoint, `http://127.0.0.1:${port}/v1/app`);
    assert.equal(config.credentialType, "publishable_key");
    assert.doesNotMatch(result.stdout, /operator-token/);
  } finally {
    await close(server);
  }
});

test("cloud app errors preserve structured Cloud messages", async () => {
  const server = createServer((req, res) => {
    if (req.url === "/projects") {
      json(res, { error: "project_access_denied" }, 403);
      return;
    }
    json(res, { error: "unexpected" }, 404);
  });
  const port = await listen(server);
  try {
    withCloudConfig(`http://127.0.0.1:${port}`);
    const result = await run(["cloud", "app", "list", "--project", "nice-rep"]);
    assert.equal(result.code, 1);
    assert.match(result.stderr, /project_access_denied/);
  } finally {
    await close(server);
  }
});

test("cloud app smoke uses the app client contract", async () => {
  const server = createServer((req, res) => {
    if (req.url === "/projects") {
      json(res, { projects: [{ id: "p1", slug: "nice-rep", name: "Nice Rep" }] });
      return;
    }
    if (req.url === "/projects/p1/app-config") {
      json(res, { app: { projectId: "p1", publishableKey: "pk_nice_rep" } });
      return;
    }
    if (req.url === "/v1/app/manifest") {
      json(res, { data: { version: "thingd.app/v1", project: { id: "p1", slug: "nice-rep" }, functions: [], capabilities: { reads: true, namedWrites: true } } });
      return;
    }
    if (req.url === "/v1/app/auth/login") {
      json(res, { data: { user: { id: "u1", email: "alice@example.com", name: "Alice", role: "user", createdAt: "now" }, accessToken: "access", refreshToken: "refresh", expiresIn: 3600 } });
      return;
    }
    if (req.url === "/v1/app/auth/me") {
      json(res, { data: { id: "u1", email: "alice@example.com", name: "Alice", role: "user", createdAt: "now" } });
      return;
    }
    if (req.url === "/v1/app/auth/logout") {
      json(res, { data: { ok: true } });
      return;
    }
    json(res, { error: "unexpected" }, 404);
  });
  const port = await listen(server);
  const file = join(tmpdir(), `nice-rep-${Date.now()}.app.json`);
  try {
    withCloudConfig(`http://127.0.0.1:${port}`);
    writeFileSync(file, JSON.stringify({ schemaVersion: "thingd.app/v1", name: "Nice Rep", slug: "nice-rep", description: "", entities: [], audiences: [], roles: [], actions: [], views: [], workflows: [], integrations: [], policies: {}, distribution: {}, presentation: {} }), "utf8");
    const result = await run(["cloud", "app", "smoke", "--project", "nice-rep", "--file", file, "--email", "alice@example.com", "--password", "password"]);
    assert.equal(result.code, 0);
    const smoke = JSON.parse(result.stdout);
    assert.equal(smoke.ok, true);
    assert.deepEqual(smoke.checks, ["manifest:thingd.app/v1", "login", "me", "logout"]);
  } finally {
    await close(server);
    try { unlinkSync(file); } catch {}
  }
});

test("cloud app lifecycle commands map to Publish and function routes", async () => {
  const calls = [];
  const server = createServer((req, res) => {
    calls.push(`${req.method} ${req.url}`);
    if (req.method === "GET" && req.url === "/projects") {
      json(res, { projects: [{ id: "p1", slug: "nice-rep", name: "Nice Rep" }] });
      return;
    }
    if (req.method === "GET" && req.url === "/projects/p1/instances") {
      json(res, { instances: [{ id: "i1", slug: "nice-rep", name: "Nice Rep" }] });
      return;
    }
    if (req.method === "GET" && req.url === "/projects/p1/publish/apps") {
      json(res, { apps: [{ id: "a1", name: "Nice Rep", slug: "nice-rep", instanceId: "i1", status: "testing" }] });
      return;
    }
    if (req.method === "GET" && req.url === "/projects/p1/app-functions") {
      json(res, { functions: [] });
      return;
    }
    if (req.method === "POST" && req.url === "/projects/p1/publish/apps") {
      json(res, { app: { id: "a1", name: "Nice Rep", slug: "nice-rep", instanceId: "i1", status: "draft" } });
      return;
    }
    if (req.method === "PUT" && req.url === "/projects/p1/publish/apps/a1") {
      json(res, { app: { id: "a1", name: "Nice Rep", slug: "nice-rep", instanceId: "i1", status: "draft" } });
      return;
    }
    if (req.method === "POST" && req.url === "/projects/p1/publish/apps/a1/validate") {
      json(res, { report: { compatible: true } });
      return;
    }
    if (req.method === "POST" && /^\/projects\/p1\/publish\/apps\/a1\/(test|publish|disable)$/.test(req.url)) {
      json(res, { app: { id: "a1", name: "Nice Rep", slug: "nice-rep", instanceId: "i1", status: "published" } });
      return;
    }
    if (req.method === "POST" && req.url === "/projects/p1/publish/apps/a1/rollback") {
      json(res, { app: { id: "a1", name: "Nice Rep", slug: "nice-rep", instanceId: "i1", status: "testing" } });
      return;
    }
    if (req.method === "POST" && req.url === "/projects/p1/app-functions") {
      json(res, { function: { name: "workout", status: "draft" } });
      return;
    }
    if (req.method === "PUT" && req.url === "/projects/p1/app-functions/workout") {
      json(res, { function: { name: "workout", status: "draft" } });
      return;
    }
    if (req.method === "POST" && /^\/projects\/p1\/app-functions\/workout\/(test|publish|disable|rollback)$/.test(req.url)) {
      json(res, { function: { name: "workout", status: "published" } });
      return;
    }
    json(res, { error: "unexpected" }, 404);
  });
  const port = await listen(server);
  const file = join(tmpdir(), `nice-rep-${Date.now()}.app.json`);
  const functionFile = join(tmpdir(), `nice-rep-${Date.now()}.function.json`);
  try {
    withCloudConfig(`http://127.0.0.1:${port}`);
    writeFileSync(file, JSON.stringify({ schemaVersion: "thingd.app/v1", name: "Nice Rep", slug: "nice-rep" }), "utf8");
    writeFileSync(functionFile, JSON.stringify({ name: "workout", description: "Build a workout" }), "utf8");

    for (const args of [
      ["cloud", "app", "create", "--project", "nice-rep", "--instance", "nice-rep", "--file", file],
      ["cloud", "app", "update", "--project", "nice-rep", "--app", "a1", "--file", file],
      ["cloud", "app", "validate", "--project", "nice-rep", "--app", "a1"],
      ["cloud", "app", "test", "--project", "nice-rep", "--app", "a1"],
      ["cloud", "app", "publish", "--project", "nice-rep", "--app", "a1"],
      ["cloud", "app", "disable", "--project", "nice-rep", "--app", "a1"],
      ["cloud", "app", "rollback", "--project", "nice-rep", "--app", "a1", "--version", "1"],
      ["cloud", "app", "functions", "list", "--project", "nice-rep"],
      ["cloud", "app", "functions", "create", "--project", "nice-rep", "--file", functionFile],
      ["cloud", "app", "functions", "update", "--project", "nice-rep", "--name", "workout", "--file", functionFile],
      ["cloud", "app", "functions", "test", "--project", "nice-rep", "--name", "workout"],
      ["cloud", "app", "functions", "publish", "--project", "nice-rep", "--name", "workout"],
      ["cloud", "app", "functions", "disable", "--project", "nice-rep", "--name", "workout"],
      ["cloud", "app", "functions", "rollback", "--project", "nice-rep", "--name", "workout", "--version", "1"],
    ]) {
      const result = await run(args);
      assert.equal(result.code, 0, `${args.join(" ")} failed: ${result.stderr}`);
    }

    assert.ok(calls.includes("POST /projects/p1/publish/apps/a1/validate"));
    assert.ok(calls.includes("POST /projects/p1/publish/apps/a1/publish"));
    assert.ok(calls.includes("POST /projects/p1/publish/apps/a1/rollback"));
    assert.ok(calls.includes("POST /projects/p1/app-functions/workout/test"));
    assert.ok(calls.includes("POST /projects/p1/app-functions/workout/publish"));
    assert.ok(calls.includes("POST /projects/p1/app-functions/workout/rollback"));
  } finally {
    await close(server);
    try { unlinkSync(file); } catch {}
    try { unlinkSync(functionFile); } catch {}
  }
});
