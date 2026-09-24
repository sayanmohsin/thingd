import assert from "node:assert/strict";
import { createServer } from "node:http";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, unlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { homedir, tmpdir } from "node:os";
import { Readable } from "node:stream";
import test from "node:test";
import { resolveConnection, runCli } from "../dist/index.js";

const ORIGINAL_HOME = process.env.HOME;
const TEST_HOME = mkdtempSync(join(tmpdir(), "thingd-cli-cloud-test-"));
process.env.HOME = TEST_HOME;
const CLOUD_CONFIG_PATH = join(homedir(), ".thingd", "cloud-config.json");

async function run(args, env = {}, stdin) {
  let stdout = "";
  let stderr = "";

  const opts = {
    env: { THINGD_PATH: ":memory:", ...env },
    stdout: { write(c) { stdout += c; } },
    stderr: { write(c) { stderr += c; } },
  };
  if (stdin) opts.stdin = stdin;

  const code = await runCli(args, opts);

  return { code, stdout, stderr };
}

function removeCloudConfig() {
  try { unlinkSync(CLOUD_CONFIG_PATH); } catch {}
}

function withCloudConfig(token = "test-token", email = "test@example.com") {
  mkdirSync(join(homedir(), ".thingd"), { recursive: true });
  writeFileSync(CLOUD_CONFIG_PATH, JSON.stringify({ token, email }), "utf-8");
}

function connectionContext(env) {
  return {
    parsed: { tokens: [], flags: new Map(), booleans: new Set() },
    env,
    stdout: { write() {} },
    stderr: { write() {} },
    stdin: Readable.from([]),
    pretty: false,
  };
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
test.after(() => {
  if (ORIGINAL_HOME === undefined) delete process.env.HOME;
  else process.env.HOME = ORIGINAL_HOME;
  rmSync(TEST_HOME, { recursive: true, force: true });
});

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

test("base THINGD_URL preserves saved Cloud instance routing", () => {
  mkdirSync(join(homedir(), ".thingd"), { recursive: true });
  writeFileSync(
    CLOUD_CONFIG_PATH,
    JSON.stringify({
      url: "https://api.thingd.cloud/",
      instanceUrl: "https://runtime.thingd.cloud/mcp",
      instanceSlug: "dev",
      token: "test-token",
    }),
    "utf-8"
  );
  const connection = resolveConnection(
    connectionContext({ THINGD_URL: "https://api.thingd.cloud" })
  );
  assert.equal(connection.path, "https://runtime.thingd.cloud");
  assert.equal(connection.instanceSlug, "dev");
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
    if (req.method === "POST" && req.url === "/auth/user-tokens") {
      res.end(JSON.stringify({ token: "mock-user-token", userToken: { id: "t1", name: "cli-test" } }));
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

test("cloud assets upload sends bounded bytes through authenticated Cloud", async () => {
  const file = join(tmpdir(), `thingd-test-${process.pid}.webp`);
  const bytes = Buffer.from("RIFF0000WEBPtest");
  writeFileSync(file, bytes);
  const calls = [];
  const asset = {
    id: "ast_0123456789abcdef0123456789abcdef",
    projectId: "p1",
    fileName: "exercise.webp",
    contentType: "image/webp",
    sizeBytes: bytes.byteLength,
    sha256: "test-sha",
    status: "ready",
    url: "https://api.thingd.cloud/v1/assets/ast_0123456789abcdef0123456789abcdef/exercise.webp",
    createdAt: "2026-09-23T00:00:00.000Z",
  };
  const server = createServer(async (req, res) => {
    const chunks = [];
    for await (const chunk of req) chunks.push(chunk);
    calls.push({ method: req.method, url: req.url, headers: req.headers, body: Buffer.concat(chunks) });
    res.setHeader("Content-Type", "application/json");
    if (req.method === "GET" && req.url === "/projects") {
      res.end(JSON.stringify({ projects: [{ id: "p1", slug: "nice-rep" }] }));
    } else if (req.method === "POST" && req.url === "/projects/p1/assets/uploads") {
      res.end(JSON.stringify({ uploadId: "upl_0123456789abcdef0123456789abcdef", expiresAt: "2026-09-23T00:10:00Z", maxBytes: 2097152 }));
    } else if (req.method === "PUT" && req.url === "/projects/p1/assets/uploads/upl_0123456789abcdef0123456789abcdef") {
      res.end(JSON.stringify({ asset, reused: false }));
    } else {
      res.statusCode = 404;
      res.end(JSON.stringify({ error: "not_found" }));
    }
  });
  await listen(server);
  try {
    withCloudConfig();
    const port = server.address().port;
    const config = JSON.parse(readFileSync(CLOUD_CONFIG_PATH, "utf8"));
    config.url = `http://127.0.0.1:${port}`;
    writeFileSync(CLOUD_CONFIG_PATH, JSON.stringify(config), "utf-8");
    const result = await run(["cloud", "assets", "upload", "--project", "nice-rep", "--file", file]);
    assert.equal(result.code, 0, result.stderr);
    assert.match(result.stdout, /https:\/\/api\.thingd\.cloud\/v1\/assets/);
    assert.deepEqual(calls.map((call) => `${call.method} ${call.url}`), [
      "GET /projects",
      "POST /projects/p1/assets/uploads",
      "PUT /projects/p1/assets/uploads/upl_0123456789abcdef0123456789abcdef",
    ]);
    assert.equal(calls[2].body.toString(), bytes.toString());
    assert.equal(calls[2].headers["content-type"], "application/octet-stream");
    assert.equal(calls[2].headers.authorization, "Bearer test-token");
    assert.equal(calls[1].headers.authorization, "Bearer test-token");
    assert.equal(JSON.stringify(calls).includes("uploadUrl"), false);
    assert.doesNotMatch(result.stdout, /mock-token|uploadUrl/);
  } finally {
    unlinkSync(file);
    await close(server);
  }
});

test("cloud assets reject a mismatched local image signature before making requests", async () => {
  const file = join(tmpdir(), `thingd-invalid-image-${process.pid}.png`);
  writeFileSync(file, Buffer.from("not a png"));
  withCloudConfig();
  const originalFetch = globalThis.fetch;
  let fetchCount = 0;
  globalThis.fetch = async (...args) => {
    fetchCount += 1;
    return originalFetch(...args);
  };
  try {
    const result = await run(["cloud", "assets", "upload", "--project", "nice-rep", "--file", file]);
    assert.notEqual(result.code, 0);
    assert.match(result.stderr, /signature/);
    assert.equal(fetchCount, 0);
  } finally {
    globalThis.fetch = originalFetch;
    unlinkSync(file);
  }
});

test("cloud login auto-selects single instance", async () => {
  const server = createServer((req, res) => {
    res.setHeader("Content-Type", "application/json");
    if (req.url === "/users/me") {
      res.end(JSON.stringify({ user: { id: "1", email: "mock@test.com" } }));
    } else if (req.method === "POST" && req.url === "/auth/user-tokens") {
      res.end(JSON.stringify({ token: "mock-user-token", userToken: { id: "t1", name: "cli-test" } }));
    } else if (req.url === "/projects") {
      res.end(JSON.stringify({
        projects: [{ id: "p1", name: "Test", slug: "test-proj" }],
      }));
    } else if (req.url === "/projects/p1/instances") {
      res.end(JSON.stringify({
        instances: [
          { id: "i1", name: "Main", slug: "main", mcpUrl: "https://thingd.cloud/mcp/test-proj/main" },
        ],
      }));
    } else {
      res.statusCode = 404;
      res.end(JSON.stringify({ error: "not_found" }));
    }
  });
  await listen(server);
  try {
    const port = server.address().port;
    const result = await run(
      ["cloud", "login", "--code", "x", "--token", "mock-token", "--url", `http://127.0.0.1:${port}`]
    );
    assert.equal(result.code, 0);
    assert.match(result.stdout, /mock@test\.com/);
    assert.match(result.stdout, /test-proj.*main/);
    assert.doesNotMatch(result.stderr, /Select an instance/);
    assert.equal(statSync(CLOUD_CONFIG_PATH).mode & 0o077, 0);
  } finally {
    await close(server);
  }
});

test("cloud login shows picker for multiple instances", async () => {
  const server = createServer((req, res) => {
    res.setHeader("Content-Type", "application/json");
    if (req.url === "/users/me") {
      res.end(JSON.stringify({ user: { id: "1", email: "mock@test.com" } }));
    } else if (req.method === "POST" && req.url === "/auth/user-tokens") {
      res.end(JSON.stringify({ token: "mock-user-token", userToken: { id: "t1", name: "cli-test" } }));
    } else if (req.url === "/projects") {
      res.end(JSON.stringify({
        projects: [{ id: "p1", name: "Test", slug: "test-proj" }],
      }));
    } else if (req.url === "/projects/p1/instances") {
      res.end(JSON.stringify({
        instances: [
          { id: "i1", name: "Main", slug: "main", mcpUrl: "https://thingd.cloud/mcp/test-proj/main" },
          { id: "i2", name: "Staging", slug: "staging", mcpUrl: "https://thingd.cloud/mcp/test-proj/staging" },
        ],
      }));
    } else {
      res.statusCode = 404;
      res.end(JSON.stringify({ error: "not_found" }));
    }
  });
  await listen(server);
  try {
    const port = server.address().port;
    const stdin = new Readable({
      read() {
        this.push("2\n");
        this.push(null);
      },
    });
    const result = await run(
      ["cloud", "login", "--code", "x", "--token", "mock-token", "--url", `http://127.0.0.1:${port}`],
      {},
      stdin
    );
    assert.equal(result.code, 0);
    assert.match(result.stderr, /test-proj.*main/);
    assert.match(result.stderr, /test-proj.*staging/);
    assert.match(result.stdout, /test-proj.*staging/);
  } finally {
    await close(server);
  }
});
