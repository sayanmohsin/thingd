import assert from "node:assert/strict";
import test from "node:test";
import { createThingdAppClient, ThingdAppError } from "../dist/app.js";

function response(body, status = 200, headers = {}) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", ...headers },
  });
}

test("app client normalizes the base URL and sends app credentials", async () => {
  const requests = [];
  const client = createThingdAppClient({
    baseUrl: "https://cloud.example///",
    publishableKey: "pk_nice_rep",
    accessToken: "app-access-token",
    fetch: async (input, init) => {
      requests.push({ url: String(input), init: init ?? {} });
      return response({
        data: {
          schemaVersion: "thingd.app/v1",
          version: "thingd.app/v1",
          project: { id: "project-1", slug: "nice-rep" },
          app: { id: "app-1", slug: "nice-rep" },
          instance: { id: "instance-1", slug: "nice-rep" },
          actions: [],
          capabilities: { reads: true, namedWrites: true },
        },
      });
    },
  });

  await client.manifest();

  assert.equal(requests[0]?.url, "https://cloud.example/v1/app/manifest");
  const headers = new Headers(requests[0]?.init.headers);
  assert.equal(headers.get("x-thingd-publishable-key"), "pk_nice_rep");
  assert.equal(headers.get("authorization"), "Bearer app-access-token");
});

test("app client exposes canonical actions and a compatibility functions alias", async () => {
  const client = createThingdAppClient({
    baseUrl: "https://cloud.example",
    publishableKey: "pk_nice_rep",
    fetch: async (input) => {
      assert.equal(String(input), "https://cloud.example/v1/app/functions");
      return response({ data: [{ name: "getProfile", description: "Read profile", auth: "user", inputSchema: {}, outputSchema: {}, version: 1, idempotency: "optional" }] });
    },
  });

  assert.strictEqual(client.functions, client.actions);
  assert.equal((await client.actions.list())[0].name, "getProfile");
});

test("app client sends idempotency keys for canonical actions", async () => {
  let captured;
  const client = createThingdAppClient({
    baseUrl: "https://cloud.example/v1",
    publishableKey: "pk_nice_rep",
    fetch: async (_input, init) => {
      captured = init;
      return response({ data: { ok: true } });
    },
  });

  await client.actions.invoke("createProfile", { timezone: "UTC" }, {
    idempotencyKey: "profile:create:user-1",
  });

  assert.equal(new Headers(captured?.headers).get("idempotency-key"), "profile:create:user-1");
});

test("app object listing uses exact collection route and pagination without search", async () => {
  let captured;
  const client = createThingdAppClient({
    baseUrl: "https://cloud.example",
    publishableKey: "pk_nice_rep",
    accessToken: "access-token",
    fetch: async (input, init) => {
      captured = { url: String(input), init };
      return response({ data: [{ id: "session-1", status: "completed" }] });
    },
  });

  const result = await client.objects.list("nice_rep_workout_sessions", { limit: 50, offset: 100 });

  assert.deepEqual(result, [{ id: "session-1", status: "completed" }]);
  assert.equal(captured.url, "https://cloud.example/v1/app/objects/nice_rep_workout_sessions?limit=50&offset=100");
  assert.equal(new Headers(captured.init.headers).get("authorization"), "Bearer access-token");
  assert.equal(new Headers(captured.init.headers).get("x-thingd-publishable-key"), "pk_nice_rep");
});

test("signup, refresh, and logout update the session callback and clear access", async () => {
  const events = [];
  const requests = [];
  const client = createThingdAppClient({
    baseUrl: "https://cloud.example",
    publishableKey: "pk_nice_rep",
    onSessionChange: (session) => events.push(session?.accessToken ?? null),
    fetch: async (input) => {
      const url = String(input);
      requests.push(url);
      if (url.endsWith("/signup")) {
        return response({ data: { user: { id: "u1" }, accessToken: "access-1", refreshToken: "refresh-1", expiresIn: 3600 } });
      }
      if (url.endsWith("/refresh")) {
        return response({ data: { user: { id: "u1" }, accessToken: "access-2", refreshToken: "refresh-2", expiresIn: 3600 } });
      }
      return response({ data: { ok: true } });
    },
  });

  await client.auth.signUp({ email: "alice@example.com", password: "password", name: "Alice" });
  assert.equal(client.getAccessToken(), "access-1");
  await client.auth.refresh("refresh-1");
  assert.equal(client.getAccessToken(), "access-2");
  await client.auth.signOut();

  assert.equal(client.getAccessToken(), undefined);
  assert.deepEqual(events, ["access-1", "access-2", null]);
  assert.ok(requests.some((url) => url.endsWith("/logout")));
});

test("app client exposes structured Cloud errors", async () => {
  const client = createThingdAppClient({
    baseUrl: "https://cloud.example",
    publishableKey: "pk_nice_rep",
    fetch: async () => response({ error: { code: "app_forbidden", message: "Denied" }, requestId: "req-1" }, 403),
  });

  await assert.rejects(
    () => client.manifest(),
    (error) => {
      assert.ok(error instanceof ThingdAppError);
      assert.equal(error.status, 403);
      assert.equal(error.code, "app_forbidden");
      assert.equal(error.requestId, "req-1");
      return true;
    }
  );
});

test("app client accepts legacy string errors and reads the request ID header", async () => {
  const client = createThingdAppClient({
    baseUrl: "https://cloud.example",
    publishableKey: "pk_nice_rep",
    fetch: async () => response({ error: "ownership_denied" }, 403, { "x-request-id": "req-header" }),
  });

  await assert.rejects(
    () => client.manifest(),
    (error) => {
      assert.ok(error instanceof ThingdAppError);
      assert.equal(error.code, "ownership_denied");
      assert.equal(error.requestId, "req-header");
      return true;
    }
  );
});

test("app client exposes bounded structured error details", async () => {
  const client = createThingdAppClient({
    baseUrl: "https://cloud.example",
    publishableKey: "pk_nice_rep",
    fetch: async () => response({
      error: {
        code: "ai_output_schema_invalid",
        message: "Output did not match the schema",
        details: [
          { path: "/sets/0/reps", keyword: "minimum", params: { limit: 1, secret: { token: "hidden" } } },
          { path: 3, keyword: "type" },
        ],
      },
    }, 502),
  });

  await assert.rejects(
    () => client.manifest(),
    (error) => {
      assert.ok(error instanceof ThingdAppError);
      assert.deepEqual(error.details, [
        { path: "/sets/0/reps", keyword: "minimum", params: { limit: 1 } },
      ]);
      return true;
    }
  );
});

test("app client verifies configured project, app, and instance identity", async () => {
  const identity = {
    project: { id: "project-1", slug: "nice-rep" },
    app: { id: "app-1", slug: "nice-rep" },
    instance: { id: "instance-1", slug: "development" },
  };
  const makeClient = (expectedIdentity) => createThingdAppClient({
    baseUrl: "https://cloud.example",
    publishableKey: "pk_nice_rep",
    expectedIdentity,
    fetch: async () => response({ data: { ...identity, schemaVersion: "thingd.app/v1", actions: [] } }),
  });

  await makeClient({ projectId: "project-1", appId: "app-1", instanceId: "instance-1" }).manifest();
  await assert.rejects(
    () => makeClient({ instanceId: "production" }).manifest(),
    (error) => error instanceof ThingdAppError && error.code === "app_identity_mismatch"
  );
  await makeClient(undefined).manifest();
});

test("app action usage is fetched from the authenticated action-scoped route", async () => {
  let captured;
  const client = createThingdAppClient({
    baseUrl: "https://cloud.example",
    publishableKey: "pk_nice_rep",
    accessToken: "user-token",
    fetch: async (input, init) => {
      captured = { url: String(input), init };
      return response({ data: { action: "generateWorkout", limit: 3, used: 1, remaining: 2, period: "lifetime" } });
    },
  });

  assert.deepEqual(await client.actions.getUsage("generateWorkout"), {
    action: "generateWorkout",
    limit: 3,
    used: 1,
    remaining: 2,
    period: "lifetime",
  });
  assert.equal(captured.url, "https://cloud.example/v1/app/actions/generateWorkout/usage");
  assert.equal(new Headers(captured.init.headers).get("authorization"), "Bearer user-token");
});

test("app client reports invalid success envelopes as structured errors", async () => {
  const client = createThingdAppClient({
    baseUrl: "https://cloud.example",
    publishableKey: "pk_nice_rep",
    fetch: async () => response({ ok: true }),
  });

  await assert.rejects(
    () => client.manifest(),
    (error) => error instanceof ThingdAppError && error.code === "invalid_app_response"
  );
});

test("app client rejects manifests without explicit app and instance identity", async () => {
  const client = createThingdAppClient({
    baseUrl: "https://cloud.example",
    publishableKey: "pk_nice_rep",
    fetch: async () => response({ data: { version: "thingd.app/v1", project: { id: "p1", slug: "nice-rep" }, actions: [] } }),
  });

  await assert.rejects(
    () => client.manifest(),
    (error) => {
      assert.ok(error instanceof ThingdAppError);
      assert.equal(error.status, 502);
      assert.equal(error.code, "invalid_app_manifest");
      return true;
    }
  );
});
