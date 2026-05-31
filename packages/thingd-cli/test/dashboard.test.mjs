import assert from "node:assert/strict";
import test from "node:test";
import { startDashboardServer } from "../dist/dashboard/server.js";

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

    // 5. Test FTS5 Stemming Search endpoint
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
