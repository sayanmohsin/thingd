import assert from "node:assert/strict";
import { createServer } from "node:http";
import test from "node:test";
import { runCli } from "../dist/index.js";

test("prints help", async () => {
  const result = await run(["--help"]);

  assert.equal(result.code, 0);
  assert.match(result.stdout, /Admin and operator CLI/);
  assert.match(result.stdout, /thingd db repack --path <source> --destination <path>/);
});

test("requires a destination for native database repack", async () => {
  const result = await run(["db", "repack", "--path", "/tmp/source.db"]);

  assert.equal(result.code, 1);
  assert.match(result.stderr, /db repack requires --path <source> and --destination <path>/);
});

test("prints local status", async () => {
  const result = await run(["status"]);

  assert.equal(result.code, 0);
  assert.deepEqual(JSON.parse(result.stdout), {
    mode: "local",
    driver: "memory",
    path: ":memory:",
  });
});

test("supports boolean flags before the command", async () => {
  const result = await run(["--pretty", "status"]);

  assert.equal(result.code, 0);
  assert.match(result.stdout, /Driver/);
  assert.match(result.stdout, /memory/);
});

test("puts an object as JSON", async () => {
  const result = await run(["objects", "put", "decisions", "cli", "--text", "CLI stores objects."]);

  assert.equal(result.code, 0);
  const object = JSON.parse(result.stdout);
  assert.equal(object.collection, "decisions");
  assert.equal(object.id, "cli");
  assert.equal(object.text, "CLI stores objects.");
});

test("pushes a queue job", async () => {
  const result = await run(["queues", "push", "embed", "--payload", '{"object":"docs/readme"}']);

  assert.equal(result.code, 0);
  const job = JSON.parse(result.stdout);
  assert.equal(job.queue, "embed");
  assert.equal(job.status, "ready");
  assert.deepEqual(job.payload, {
    object: "docs/readme",
  });
});

test("reports remote status", async () => {
  const server = createServer((request, response) => {
    response.setHeader("Content-Type", "application/json");

    if (request.url === "/healthz") {
      response.end(
        JSON.stringify({
          ok: true,
          service: "thingd-mcp",
          driver: "native",
        }),
      );
      return;
    }

    if (request.url === "/cluster/status") {
      response.end(
        JSON.stringify({
          mode: "single",
          writable: true,
          forwarding: false,
          discovery: "none",
          peers: [],
          replication: "not-implemented",
        }),
      );
      return;
    }

    response.statusCode = 404;
    response.end(
      JSON.stringify({
        error: "not_found",
      }),
    );
  });
  await listen(server);

  try {
    const address = server.address();
    assert.equal(typeof address, "object");
    assert.ok(address);

    const result = await run(["--url", `http://127.0.0.1:${address.port}/mcp`, "status"]);

    assert.equal(result.code, 0);
    const status = JSON.parse(result.stdout);
    assert.equal(status.mode, "cloud");
    assert.equal(status.health.service, "thingd-mcp");
    assert.equal(status.cluster.replication, "not-implemented");
  } finally {
    await close(server);
  }
});

test("resolves to default database path when THINGD_PATH is not set", async () => {
  const result = await run(["status"], {});

  assert.equal(result.code, 0);
  const status = JSON.parse(result.stdout);
  assert.equal(status.mode, "local");
  assert.match(status.path, /\.thingd[\\/]data\.db$/);
});

test("runs install command and prints configuration", async () => {
  const result = await run(["install"]);

  assert.equal(result.code, 0);
  
  // Stderr contains setup status and instructions
  assert.match(result.stderr, /Database path:/);
  assert.match(result.stderr, /Driver:/);
  assert.match(result.stderr, /Node:/);
  assert.match(result.stderr, /CLI:/);
  assert.match(result.stderr, /Cursor:/);

  // Stdout contains the JSON configuration block
  const config = JSON.parse(result.stdout);
  assert.ok(config.mcpServers);
  assert.ok(config.mcpServers.thingd);
  assert.equal(config.mcpServers.thingd.command, process.execPath);
  assert.ok(Array.isArray(config.mcpServers.thingd.args));
  assert.equal(config.mcpServers.thingd.args[1], "mcp");
});


async function run(args, env = { THINGD_PATH: ":memory:" }) {
  let stdout = "";
  let stderr = "";

  const code = await runCli(args, {
    env,
    stdout: {
      write: (chunk) => {
        stdout += chunk;
      },
    },
    stderr: {
      write: (chunk) => {
        stderr += chunk;
      },
    },
  });

  return {
    code,
    stdout,
    stderr,
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
