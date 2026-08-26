/**
 * thingd + Bun + Hono example
 *
 * Run the thingd sidecar first:
 *   thingd serve --http :8757
 *
 * Then run this app:
 *   bun run src/index.ts
 */

import { HttpThingStore } from "@thingd/sdk/client";
import { Hono } from "hono";

// ── Connect to thingd sidecar over HTTP ──────────────────────
const thingd = await HttpThingStore.open({
  url: Bun.env.THINGD_URL ?? "http://localhost:8757",
  authToken: Bun.env.THINGD_AUTH_TOKEN,
});

const app = new Hono();

// ── Objects ─────────────────────────────────────────────────

app.post("/objects", async (c) => {
  const body = await c.req.json<{ id: string; [key: string]: unknown }>();
  if (!body.id) {
    return c.json({ error: "id is required" }, 400);
  }
  const stored = await thingd.put("items", body);
  return c.json(stored, 201);
});

app.get("/objects/:collection/:id", async (c) => {
  const { collection, id } = c.req.param();
  const obj = await thingd.get(collection, id);
  if (!obj) {
    return c.json({ error: "not found" }, 404);
  }
  return c.json(obj);
});

app.get("/objects", async (c) => {
  const collection = c.req.query("collection") ?? "items";
  const limit = Number(c.req.query("limit")) || 20;
  const objects = await thingd.listObjects(collection, { limit });
  return c.json({ objects, count: objects.length });
});

app.delete("/objects/:collection/:id", async (c) => {
  const { collection, id } = c.req.param();
  const result = await thingd.delete(collection, id);
  return c.json(result);
});

// ── Events ──────────────────────────────────────────────────

app.post("/events/:stream", async (c) => {
  const body = await c.req.json<{ type: string; text?: string }>();
  if (!body.type) {
    return c.json({ error: "type is required" }, 400);
  }
  const event = await thingd.appendEvent(c.req.param("stream"), body);
  return c.json(event, 201);
});

app.get("/events", async (c) => {
  const stream = c.req.query("stream");
  const events = await thingd.listEvents(stream);
  return c.json({ events });
});

// ── Queues ──────────────────────────────────────────────────

app.post("/queues/:name/push", async (c) => {
  const body = await c.req.json<Record<string, unknown>>();
  const job = await thingd.pushJob(c.req.param("name"), body);
  return c.json(job, 201);
});

app.post("/queues/:name/claim", async (c) => {
  const job = await thingd.claimJob(c.req.param("name"));
  if (!job) {
    return c.body(null, 204);
  }
  return c.json(job);
});

app.post("/queues/:name/ack/:jobId", async (c) => {
  const result = await thingd.ackJob(c.req.param("name"), c.req.param("jobId"));
  return c.json(result);
});

// ── Search ──────────────────────────────────────────────────

app.get("/search", async (c) => {
  const query = c.req.query("q") ?? "";
  if (!query) {
    return c.json({ error: "q is required" }, 400);
  }
  const results = await thingd.search(query);
  return c.json({ results });
});

// ── Health ──────────────────────────────────────────────────

app.get("/health", (c) => c.json({ ok: true }));

// ── Start ───────────────────────────────────────────────────

const port = Number(Bun.env.PORT) || 3000;
console.log(`Listening on http://localhost:${port}`);

export default {
  port,
  fetch: app.fetch,
};
