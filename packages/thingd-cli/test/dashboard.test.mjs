import assert from "node:assert/strict";
import { once } from "node:events";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { createServer, request as httpRequest } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { startDashboardServer } from "../dist/dashboard/server.js";

test("Dashboard - keeps access protection separate from remote database credentials", async (t) => {
  const upstreamTokens = [];
  const upstream = createServer((req, res) => {
    upstreamTokens.push(req.headers.authorization);
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ data: { count: 0, queues: 0 } }));
  });
  upstream.listen(0, "127.0.0.1");
  await once(upstream, "listening");
  t.after(() => new Promise((resolve) => {
    upstream.close(resolve);
    upstream.closeAllConnections();
  }));
  for (const token of [undefined, "dashboard-access-key"]) {
    await t.test(token ? "protected local session" : "local session without a token", async () => {
      const { server, close } = await startDashboardServer({
        path: ":memory:", driver: "memory", cloud: false, authToken: token,
      }, 0);
      const url = `http://127.0.0.1:${server.address().port}`;
      const headers = {
        "content-type": "application/json",
        ...(token ? { authorization: `Bearer ${token}` } : {}),
      };
      try {
        if (token) {
          for (const authorization of [undefined, "Bearer wrong-token"]) {
            const denied = await fetch(`${url}/api/connect`, {
              method: "POST",
              headers: { "content-type": "application/json", ...(authorization ? { authorization } : {}) },
              body: JSON.stringify({ path: ":memory:", driver: "memory" }),
            });
            assert.equal(denied.status, 401);
          }
        }
        const remote = await fetch(`${url}/api/connect`, {
          method: "POST", headers,
          body: JSON.stringify({
            path: `http://127.0.0.1:${upstream.address().port}`,
            driver: "cloud", authToken: "remote-database-key",
          }),
        });
        assert.equal(remote.status, 200);
        const status = await fetch(`${url}/api/status`, { headers });
        assert.equal(status.status, 200);
        assert.equal((await status.json()).authRequired, Boolean(token));
        assert.ok(upstreamTokens.length > 0);
        assert.ok(upstreamTokens.every((value) => value === "Bearer remote-database-key"));
        if (token) {
          assert.equal((await fetch(`${url}/api/status`)).status, 401);
          assert.equal((await fetch(`${url}/api/status`, {
            headers: { authorization: "Bearer remote-database-key" },
          })).status, 401);
        }
        const local = await fetch(`${url}/api/connect`, {
          method: "POST", headers,
          body: JSON.stringify({ path: ":memory:", driver: "memory" }),
        });
        assert.equal(local.status, 200);
        assert.equal((await fetch(`${url}/api/status`, { headers })).status, 200);
        if (token) {
          assert.equal((await fetch(`${url}/api/status`)).status, 401);
        }
      } finally {
        await close();
      }
    });
  }
});

test("Dashboard - rejects foreign origins and hosts on its actual local port", async () => {
  const { server, close } = await startDashboardServer({
    path: ":memory:", driver: "memory", cloud: false,
  }, 0);
  const { port, address } = server.address();
  const url = `http://127.0.0.1:${port}`;
  assert.equal(address, "127.0.0.1");
  try {
    for (const method of ["GET", "POST", "OPTIONS"]) {
      for (const origin of ["https://foreign.example", "null", `http://127.0.0.1:${port + 1}`]) {
        const response = await fetch(`${url}/api/connect`, { method, headers: { origin } });
        assert.equal(response.status, 403);
        assert.equal(response.headers.get("access-control-allow-origin"), null);
      }
    }
    for (const fetchSite of ["cross-site", "same-site"]) {
      const response = await fetch(`${url}/api/status`, { headers: { "sec-fetch-site": fetchSite } });
      assert.equal(response.status, 403);
    }
    const allowed = await fetch(`${url}/api/status`, { headers: { origin: url } });
    assert.equal(allowed.status, 200);
    assert.equal(allowed.headers.get("access-control-allow-origin"), url);
    const write = await fetch(`${url}/api/objects`, {
      method: "POST", headers: { origin: url, "content-type": "application/json" },
      body: JSON.stringify({ collection: "notes", id: "same-origin", text: "Allowed" }),
    });
    assert.equal(write.status, 200);
    const preflight = await fetch(`${url}/api/objects`, { method: "OPTIONS", headers: { origin: url } });
    assert.equal(preflight.status, 204);
    const untrustedHost = await new Promise((resolve, reject) => {
      const req = httpRequest(`${url}/api/status`, { headers: { host: "foreign.example" } }, (res) => {
        res.resume();
        res.on("end", () => resolve(res.statusCode));
      });
      req.on("error", reject);
      req.end();
    });
    assert.equal(untrustedHost, 403);
    assert.equal((await fetch(`${url}/api/status`)).status, 200);
  } finally {
    await close();
  }
});

test("Dashboard - a failed replacement leaves the current database usable", async () => {
  const directory = await mkdtemp(join(tmpdir(), "thingd-dashboard-switch-"));
  const invalidPath = join(directory, "not-a-directory");
  await writeFile(invalidPath, "not a database directory");
  const { server, close } = await startDashboardServer({
    path: ":memory:", driver: "memory", cloud: false,
  }, 0);
  const url = `http://127.0.0.1:${server.address().port}`;
  try {
    const created = await fetch(`${url}/api/objects`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ collection: "notes", id: "keep", text: "Keep this connection" }),
    });
    assert.equal(created.status, 200);
    const failed = await fetch(`${url}/api/connect`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ path: invalidPath, driver: "native" }),
    });
    assert.equal(failed.status, 500);
    const response = await fetch(`${url}/api/objects?collection=notes`);
    assert.equal(response.status, 200);
    assert.equal((await response.json())[0].id, "keep");
  } finally {
    await close();
    await rm(directory, { recursive: true, force: true });
  }
});

test("Dashboard - serves static assets and REST API endpoints", async () => {
  // Start the dashboard server on a random available port (0)
  const connection = {
    path: ":memory:",
    driver: "memory",
    cloud: false,
  };
  const { server, close } = await startDashboardServer(connection, 0);
  const address = server.address();
  const url = `http://localhost:${address.port}`;

  try {
    // 1. Test Static File Serving
    const indexResponse = await fetch(`${url}/`);
    const htmlText = await indexResponse.text();
    assert.equal(indexResponse.status, 200);
    assert.ok(indexResponse.headers.get("content-type").includes("text/html"));
    assert.ok(htmlText.includes("<title>thingd | Inspector Dashboard</title>"));

    // 2. Test GET /api/status
    const statusResponse = await fetch(`${url}/api/status`);
    const status = await statusResponse.json();
    assert.equal(statusResponse.status, 200);
    assert.equal(status.mode, "local");
    assert.equal(status.driver, "memory");
    assert.equal(status.path, ":memory:");
    assert.equal(status.metrics.objects, 0);

    // 3. Test POST /api/objects
    const putResponse = await fetch(`${url}/api/objects`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        collection: "memories",
        id: "diagnostic-job",
        text: "The agent started building the localhost dashboard.",
        data: { importance: "high", tags: ["phase5"] },
      }),
    });
    const putResult = await putResponse.json();
    assert.equal(putResponse.status, 200);
    assert.equal(putResult.id, "diagnostic-job");

    // 4. Test GET /api/objects
    const getResponse = await fetch(`${url}/api/objects?collection=memories`);
    const getResult = await getResponse.json();
    assert.equal(getResponse.status, 200);
    assert.equal(getResult.length, 1);
    assert.equal(getResult[0].id, "diagnostic-job");

    // 5. Test Tantivy stemming search endpoint
    const searchResponse = await fetch(
      `${url}/api/search?query=dashboard&collections=memories`
    );
    const searchResult = await searchResponse.json();
    assert.equal(searchResponse.status, 200);
    assert.equal(searchResult.length, 1);

    // 6. Test GET /api/collections
    const collectionsResponse = await fetch(`${url}/api/collections`);
    const collections = await collectionsResponse.json();
    assert.deepEqual(collections, ["memories"]);

    // 7. Test Queue push and claim
    const pushResponse = await fetch(`${url}/api/queues/push`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        queue: "tasks-worker",
        payload: { action: "verify_dashboard" },
        maxAttempts: 3,
      }),
    });
    const pushResult = await pushResponse.json();
    assert.equal(pushResponse.status, 200);

    // List queues
    const queuesResponse = await fetch(`${url}/api/queues`);
    const queues = await queuesResponse.json();
    assert.deepEqual(queues, ["tasks-worker"]);

    // 8. Test DELETE /api/objects
    const deleteResponse = await fetch(
      `${url}/api/objects?collection=memories&id=diagnostic-job`,
      { method: "DELETE" }
    );
    assert.equal(deleteResponse.status, 200);

  } finally {
    await close();
  }
});

test("Dashboard - enforces security gates (authentication) when configured", async () => {
  const connection = {
    path: ":memory:",
    driver: "memory",
    cloud: false,
    authToken: "secure-auth-key",
  };
  const { server, close } = await startDashboardServer(connection, 0);
  const address = server.address();
  const url = `http://localhost:${address.port}`;

  try {
    // 1. Test Static files bypass authentication
    const indexResponse = await fetch(`${url}/`);
    assert.equal(indexResponse.status, 200);

    // 2. Test API endpoints require auth token
    const statusNoAuthResponse = await fetch(`${url}/api/status`);
    assert.equal(statusNoAuthResponse.status, 401);

    const statusBadAuthResponse = await fetch(`${url}/api/status`, {
      headers: { Authorization: "Bearer wrong-token" },
    });
    assert.equal(statusBadAuthResponse.status, 401);

    // 3. Test API endpoints resolve with correct token
    const statusGoodAuthResponse = await fetch(`${url}/api/status`, {
      headers: { Authorization: "Bearer secure-auth-key" },
    });
    const status = await statusGoodAuthResponse.json();
    assert.equal(statusGoodAuthResponse.status, 200);
    assert.equal(status.authRequired, true);

  } finally {
    await close();
  }
});

test("Dashboard - supports dynamic connection swapping", async () => {
  const connection = {
    path: ":memory:",
    driver: "memory",
    cloud: false,
  };
  const { server, close } = await startDashboardServer(connection, 0);
  const address = server.address();
  const url = `http://localhost:${address.port}`;

  try {
    // Write an object to the first database (in-memory)
    await fetch(`${url}/api/objects`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ collection: "col1", id: "obj1", text: "first database" }),
    });

    const getResponse1 = await fetch(`${url}/api/objects?collection=col1`);
    const objects1 = await getResponse1.json();
    assert.equal(objects1.length, 1);

    // Swap connection dynamically to a fresh in-memory database
    const swapResponse = await fetch(`${url}/api/connect`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        path: ":memory:",
        driver: "memory",
      }),
    });
    const swapResult = await swapResponse.json();
    assert.equal(swapResponse.status, 200);
    assert.equal(swapResult.success, true);

    // Verify database was swapped and objects are empty
    const getResponse2 = await fetch(`${url}/api/objects?collection=col1`);
    const objects2 = await getResponse2.json();
    assert.equal(objects2.length, 0);

  } finally {
    await close();
  }
});
