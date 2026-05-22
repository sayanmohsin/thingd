import assert from "node:assert/strict";
import { createServer } from "node:http";
import test from "node:test";
import { runCli } from "../dist/index.js";

test("prints help", async () => {
  const result = await run(["--help"]);

  assert.equal(result.code, 0);
  assert.match(result.stdout, /Admin and operator CLI/);
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
  assert.match(result.stdout, /\n  "mode": "local"/);
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
    assert.equal(status.mode, "remote");
    assert.equal(status.health.service, "thingd-mcp");
    assert.equal(status.cluster.replication, "not-implemented");
  } finally {
    await close(server);
  }
});

async function run(args, env = {}) {
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
