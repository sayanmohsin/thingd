import assert from "node:assert/strict";
import { existsSync, rmSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";
import { runCli } from "../dist/index.js";

const dbPath = resolve("test-cli-b.db");
const env = {
  THINGD_PATH: dbPath,
  THINGD_DRIVER: "native",
};

function cleanup() {
  for (const file of [dbPath, `${dbPath}-wal`, `${dbPath}-shm`]) {
    if (existsSync(file)) {
      try {
        rmSync(file, { force: true });
      } catch {
        // Ignore cleanup errors
      }
    }
  }
}

test.beforeEach(() => {
  cleanup();
});

test.after(() => {
  cleanup();
});

test("thingd doctor prints system and driver status", async () => {
  const result = await run(["doctor"]);

  assert.equal(result.code, 0);
  assert.match(result.stderr, /thingd doctor/);
  assert.match(result.stderr, /Node version:/);
  assert.match(result.stderr, /Connectivity:/);
  assert.match(result.stderr, /Everything looks healthy!/);
});

test("thingd collections list works in json and pretty mode", async () => {
  // Put an object to auto-create a collection
  const putRes = await run(["objects", "put", "tasks", "task-1", "--text", "Finish CLI B"]);
  assert.equal(putRes.code, 0);

  // Test JSON mode (default)
  const jsonResult = await run(["collections", "list"]);
  assert.equal(jsonResult.code, 0);
  const collections = JSON.parse(jsonResult.stdout);
  assert.ok(Array.isArray(collections));
  assert.ok(collections.includes("tasks"));

  // Test Pretty mode
  const prettyResult = await run(["--pretty", "collections", "list"]);
  assert.equal(prettyResult.code, 0);
  assert.match(prettyResult.stdout, /tasks/);
});

test("thingd objects list works in json and pretty mode", async () => {
  const id = `task-${Date.now()}`;
  const putRes = await run(["objects", "put", "tasks", id, "--text", "Finish CLI B"]);
  assert.equal(putRes.code, 0);

  // Test JSON mode
  const jsonResult = await run(["objects", "list", "tasks"]);
  assert.equal(jsonResult.code, 0);
  const list = JSON.parse(jsonResult.stdout);
  assert.ok(Array.isArray(list));
  assert.equal(list.length, 1);
  assert.equal(list[0].id, id);

  // Test Pretty mode
  const prettyResult = await run(["--pretty", "objects", "list", "tasks"]);
  assert.equal(prettyResult.code, 0);
  assert.match(prettyResult.stdout, /v1/);
});

test("thingd events streams alias and streams list works in pretty mode", async () => {
  const appRes = await run(["events", "append", "log-stream", "user.signup", "--text", "User registered"]);
  assert.equal(appRes.code, 0);

  // Test events streams in pretty mode
  const prettyResult = await run(["--pretty", "events", "streams"]);
  assert.equal(prettyResult.code, 0);
  assert.match(prettyResult.stdout, /log-stream/);

  // Test streams list in pretty mode
  const aliasResult = await run(["--pretty", "streams", "list"]);
  assert.equal(aliasResult.code, 0);
  assert.match(aliasResult.stdout, /log-stream/);
});

test("thingd queues stats works in json and pretty mode", async () => {
  const pushRes = await run(["queues", "push", "worker-queue", "--payload", '{"task":"build"}']);
  assert.equal(pushRes.code, 0);

  // Test JSON mode
  const jsonResult = await run(["queues", "stats", "worker-queue"]);
  assert.equal(jsonResult.code, 0);
  const stats = JSON.parse(jsonResult.stdout);
  assert.equal(stats.queue, "worker-queue");
  assert.equal(stats.totalActive, 1);
  assert.equal(stats.ready, 1);
  assert.equal(stats.leased, 0);
  assert.equal(stats.dead, 0);

  // Test Pretty mode
  const prettyResult = await run(["--pretty", "queues", "stats", "worker-queue"]);
  assert.equal(prettyResult.code, 0);
  assert.match(prettyResult.stdout, /worker-queue/);
  assert.match(prettyResult.stdout, /Ready/);
});

async function run(args) {
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
