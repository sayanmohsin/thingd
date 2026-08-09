import { existsSync, promises as fs, statSync } from "node:fs";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { createRequire } from "node:module";
import { dirname, extname, isAbsolute, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { handleRestRequest, ThingD } from "@thingd/sdk";
import type { ConnectionOptions } from "../index.js";
import { readCloudConfig } from "../lib/cloud-config.js";
import { readSyncConfig } from "../lib/sync-config.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const _require = createRequire(__filename);
let pkgVersion = "0.0.0";
try {
  const pkg = _require("../../package.json");
  pkgVersion = pkg.version || "0.0.0";
} catch {
  // fallback — try relative to dist
  try {
    const pkg = _require("../package.json");
    pkgVersion = pkg.version || "0.0.0";
  } catch {
    // leave default
  }
}

// Candidate public folders to support both tsx dev and compiled dist packaging
const publicDirCandidates = [
  join(__dirname, "public"),
  join(__dirname, "../public"),
  join(__dirname, "../../../src/dashboard/public"),
  join(__dirname, "../../src/dashboard/public"),
];

let publicDir = "";
for (const cand of publicDirCandidates) {
  if (existsSync(cand)) {
    publicDir = cand;
    break;
  }
}

const MIME_TYPES: Record<string, string> = {
  ".html": "text/html",
  ".css": "text/css",
  ".js": "application/javascript",
  ".json": "application/json",
  ".png": "image/png",
  ".ico": "image/x-icon",
  ".svg": "image/svg+xml",
  ".webmanifest": "application/manifest+json",
};

async function readBody(req: IncomingMessage): Promise<string> {
  return new Promise((resolvePromise, rejectPromise) => {
    let body = "";
    req.on("data", (chunk) => {
      body += chunk;
    });
    req.on("end", () => {
      resolvePromise(body);
    });
    req.on("error", (err) => {
      rejectPromise(err);
    });
  });
}

function sendError(res: ServerResponse, status: number, message: string): void {
  res.writeHead(status, { "Content-Type": "application/json" });
  res.end(JSON.stringify({ error: message }));
}

function isCloudPath(path: string): boolean {
  return path.startsWith("http://") || path.startsWith("https://") || path.startsWith("thingd://");
}

export async function startDashboardServer(
  connectionOptions: ConnectionOptions,
  port: number
): Promise<{ server: import("node:http").Server; close: () => Promise<void> }> {
  // 1. Maintain dynamic active database options
  let activeOptions = { ...connectionOptions };
  let db = await ThingD.open({
    path: activeOptions.path,
    url: activeOptions.cloud ? activeOptions.path : undefined,
    driver: activeOptions.driver,
    authToken: activeOptions.authToken,
    instanceSlug: activeOptions.instanceSlug,
  });

  // 2. Create HTTP Server
  const server = createServer(async (req, res) => {
    try {
      const url = new URL(req.url || "", `http://${req.headers.host || "localhost"}`);
      const pathname = url.pathname;

      // Handle CORS for ease of developer integrations
      const allowedOrigins = [
        "http://localhost:8757",
        "http://localhost:8758",
        "http://127.0.0.1:8757",
        "http://127.0.0.1:8758",
      ];
      const origin = req.headers.origin;
      if (origin && allowedOrigins.includes(origin)) {
        res.setHeader("Access-Control-Allow-Origin", origin);
      } else {
        res.setHeader("Access-Control-Allow-Origin", "*");
      }
      res.setHeader("Access-Control-Allow-Methods", "GET, POST, DELETE, OPTIONS");
      res.setHeader("Access-Control-Allow-Headers", "Content-Type, Authorization");

      if (req.method === "OPTIONS") {
        res.writeHead(204);
        res.end();
        return;
      }

      // CSRF protection: state-changing requests must come from a known origin
      if (req.method !== "GET" && req.method !== "OPTIONS" && req.method !== "HEAD") {
        const requestOrigin = req.headers.origin;
        if (requestOrigin && !allowedOrigins.includes(requestOrigin)) {
          sendError(res, 403, "Cross-origin state-changing requests are not allowed");
          return;
        }
      }

      // Security Gate middleware for API endpoints
      const isApiRoute = pathname.startsWith("/api/");
      const isConnectRoute = pathname === "/api/connect";
      const isRestRoute = pathname.startsWith("/v1/");

      if ((isApiRoute || isRestRoute) && !isConnectRoute && activeOptions.authToken) {
        const authHeader = req.headers.authorization;
        const expectedHeader = `Bearer ${activeOptions.authToken}`;
        if (!authHeader || authHeader !== expectedHeader) {
          res.writeHead(401, { "Content-Type": "application/json" });
          res.end(JSON.stringify({ error: "Unauthorized. Valid auth token is required." }));
          return;
        }
      }

      // REST API Routes
      if (pathname.startsWith("/api/")) {
        // POST /api/connect (Dynamic connection swapping)
        if (pathname === "/api/connect" && req.method === "POST") {
          const bodyStr = await readBody(req);
          const { path, driver, authToken, instanceSlug } = JSON.parse(bodyStr);

          if (!path || !driver) {
            sendError(res, 400, "Fields 'path' and 'driver' are required.");
            return;
          }

          const cloudMode = isCloudPath(path);
          const resolvedToken = authToken || (cloudMode ? readCloudConfig()?.token : undefined);
          const resolvedInstanceSlug = instanceSlug || activeOptions.instanceSlug;

          // Safely shut down the old db instance
          await db.close();

          // Spawn new db connection dynamically
          db = await ThingD.open({
            path,
            url: cloudMode ? path : undefined,
            driver,
            authToken: resolvedToken,
            instanceSlug: resolvedInstanceSlug,
          });

          activeOptions = {
            path,
            driver,
            authToken: resolvedToken,
            cloud: cloudMode,
            instanceSlug: resolvedInstanceSlug,
          };

          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify({ success: true, path, driver }));
          return;
        }

        // GET /api/status
        if (pathname === "/api/status" && req.method === "GET") {
          const [objects, events, activeJobs, deadJobs] = await Promise.all([
            db.countObjects(),
            db.countEvents(),
            db.countActiveJobs(),
            db.countDeadJobs(),
          ]);

          let dbSize = "N/A (in-memory)";
          if (activeOptions.driver === "native" && existsSync(activeOptions.path)) {
            try {
              const stats = statSync(activeOptions.path);
              dbSize = `${(stats.size / 1024).toFixed(1)} KB`;
            } catch {
              dbSize = "N/A (error)";
            }
          }

          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(
            JSON.stringify({
              version: pkgVersion,
              mode: activeOptions.cloud ? "cloud" : "local",
              driver: activeOptions.driver || "memory",
              path: activeOptions.path,
              metrics: { objects, events, activeJobs, deadJobs, dbSize },
              authRequired: !!activeOptions.authToken,
            })
          );
          return;
        }

        if (pathname === "/api/replication/status" && req.method === "GET") {
          const config = readSyncConfig();
          const replicationEvents = await db.events.list("__thingd:system:replication", {
            limit: 1_000_000,
          });
          const last = replicationEvents.at(-1) as { sequence?: number } | undefined;
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(
            JSON.stringify({
              configured: Boolean(config),
              config,
              sourceId: config?.sourceId,
              latestCursor: last?.sequence ?? 0,
              protectedCloudTarget: config?.provider === "thingd.cloud" && !config.allowCloudTarget,
            })
          );
          return;
        }

        // GET /api/collections
        if (pathname === "/api/collections" && req.method === "GET") {
          const collections = await db.listCollections();
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify(collections));
          return;
        }

        // GET/POST/DELETE /api/objects
        if (pathname === "/api/objects") {
          if (req.method === "GET") {
            const collection = url.searchParams.get("collection");
            if (!collection) {
              sendError(res, 400, "Query parameter 'collection' is required.");
              return;
            }
            const objects = await db.listObjects(collection);
            res.writeHead(200, { "Content-Type": "application/json" });
            res.end(JSON.stringify(objects));
            return;
          }

          if (req.method === "POST") {
            const bodyStr = await readBody(req);
            const { collection, id, text, data } = JSON.parse(bodyStr);
            if (!collection || !id) {
              sendError(res, 400, "Fields 'collection' and 'id' are required.");
              return;
            }
            const result = await db.put(collection, { id, text, ...data });
            res.writeHead(200, { "Content-Type": "application/json" });
            res.end(JSON.stringify(result));
            return;
          }

          if (req.method === "DELETE") {
            const collection = url.searchParams.get("collection");
            const id = url.searchParams.get("id");
            if (!collection || !id) {
              sendError(res, 400, "Query parameters 'collection' and 'id' are required.");
              return;
            }
            const result = await db.delete(collection, id);
            res.writeHead(200, { "Content-Type": "application/json" });
            res.end(JSON.stringify(result));
            return;
          }
        }

        // GET/POST /api/events
        if (pathname === "/api/events") {
          if (req.method === "GET") {
            const stream = url.searchParams.get("stream") || undefined;
            const limitVal = url.searchParams.get("limit");
            const limit = limitVal ? Number.parseInt(limitVal, 10) : undefined;

            const events = await db.events.list(stream);
            const sliced = limit ? events.slice(0, limit) : events;

            res.writeHead(200, { "Content-Type": "application/json" });
            res.end(JSON.stringify(sliced));
            return;
          }

          if (req.method === "POST") {
            const bodyStr = await readBody(req);
            const { stream, type, text, data } = JSON.parse(bodyStr);
            if (!stream || !type) {
              sendError(res, 400, "Fields 'stream' and 'type' are required.");
              return;
            }
            const result = await db.events.append(stream, { type, text, ...data });
            res.writeHead(200, { "Content-Type": "application/json" });
            res.end(JSON.stringify(result));
            return;
          }
        }

        // GET /api/events/streams
        if (pathname === "/api/events/streams" && req.method === "GET") {
          const streams = await db.listStreams();
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify(streams));
          return;
        }

        // GET /api/queues
        if (pathname === "/api/queues" && req.method === "GET") {
          const queues = await db.listQueues();
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify(queues));
          return;
        }

        // GET /api/queues/jobs
        if (pathname === "/api/queues/jobs" && req.method === "GET") {
          const queue = url.searchParams.get("queue");
          const status = url.searchParams.get("status");
          if (!queue) {
            sendError(res, 400, "Query parameter 'queue' is required.");
            return;
          }
          const q = db.queue(queue);
          const jobs = status === "dead" ? await q.dead() : await q.list();
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify(jobs));
          return;
        }

        // GET /api/queues/stats
        if (pathname === "/api/queues/stats" && req.method === "GET") {
          const queue = url.searchParams.get("queue");
          if (!queue) {
            sendError(res, 400, "Query parameter 'queue' is required.");
            return;
          }
          const q = db.queue(queue);
          const [activeJobs, deadJobs] = await Promise.all([q.list(), q.dead()]);
          const leased = activeJobs.filter((j) => j.status === "leased").length;
          const ready = activeJobs.filter((j) => j.status === "ready").length;

          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(
            JSON.stringify({
              queue,
              totalActive: activeJobs.length,
              ready,
              leased,
              dead: deadJobs.length,
            })
          );
          return;
        }

        // POST /api/queues/push
        if (pathname === "/api/queues/push" && req.method === "POST") {
          const bodyStr = await readBody(req);
          const { queue, payload, delayMs, maxAttempts, idempotencyKey } = JSON.parse(bodyStr);
          if (!queue || !payload) {
            sendError(res, 400, "Fields 'queue' and 'payload' are required.");
            return;
          }
          const q = db.queue(queue);
          const result = await q.push(payload, { delayMs, maxAttempts, idempotencyKey });
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify(result));
          return;
        }

        // POST /api/queues/claim
        if (pathname === "/api/queues/claim" && req.method === "POST") {
          const bodyStr = await readBody(req);
          const { queue, leaseMs } = JSON.parse(bodyStr);
          if (!queue) {
            sendError(res, 400, "Field 'queue' is required.");
            return;
          }
          const q = db.queue(queue);
          const job = await q.claim({ leaseMs });
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify(job || null));
          return;
        }

        // POST /api/queues/ack
        if (pathname === "/api/queues/ack" && req.method === "POST") {
          const bodyStr = await readBody(req);
          const { queue, jobId } = JSON.parse(bodyStr);
          if (!queue || !jobId) {
            sendError(res, 400, "Fields 'queue' and 'jobId' are required.");
            return;
          }
          const q = db.queue(queue);
          const result = await q.ack(jobId);
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify(result));
          return;
        }

        // POST /api/queues/nack
        if (pathname === "/api/queues/nack" && req.method === "POST") {
          const bodyStr = await readBody(req);
          const { queue, jobId, error, delayMs } = JSON.parse(bodyStr);
          if (!queue || !jobId) {
            sendError(res, 400, "Fields 'queue' and 'jobId' are required.");
            return;
          }
          const q = db.queue(queue);
          const result = await q.nack(jobId, { error, delayMs });
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify(result));
          return;
        }

        // GET /api/search
        if (pathname === "/api/search" && req.method === "GET") {
          const query = url.searchParams.get("query");
          const limitVal = url.searchParams.get("limit");
          const collectionsStr = url.searchParams.get("collections");
          const filterStr = url.searchParams.get("filter");

          if (!query) {
            sendError(res, 400, "Query parameter 'query' is required.");
            return;
          }

          const limit = limitVal ? Number.parseInt(limitVal, 10) : undefined;
          const collections = collectionsStr ? collectionsStr.split(",") : undefined;
          const filter = filterStr ? JSON.parse(filterStr) : undefined;

          const results = await db.search(query, { limit, collections, filter });
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify(results));
          return;
        }

        // GET /api/schema
        if (pathname === "/api/schema" && req.method === "GET") {
          const collection = url.searchParams.get("collection") || undefined;
          const schemas = await db.schema(collection);
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify(schemas));
          return;
        }

        // POST /api/nlq
        if (pathname === "/api/nlq" && req.method === "POST") {
          const bodyStr = await readBody(req);
          const { question, collection, model, endpoint, apiKey } = JSON.parse(bodyStr);
          if (!question) {
            sendError(res, 400, "Field 'question' is required.");
            return;
          }

          const llmModel = model || "llama3";
          const llmEndpoint = (endpoint || "http://localhost:11434/v1").replace(/\/+$/, "");
          const llmApiKey = apiKey || "";

          // Step 1: Reflect schema
          const schemas = await db.schema(collection || undefined);
          if (!schemas || schemas.length === 0) {
            sendError(
              res,
              400,
              "No collections found. Add objects first or specify a valid collection."
            );
            return;
          }

          // Step 2: Build prompt and call LLM
          const systemPrompt = `You are a data analysis assistant. The user has a thingd database with these collections and inferred schemas:

${JSON.stringify(schemas, null, 2)}

You can perform these operations on the data:
- "aggregate": count/sum/avg/min/max with optional groupBy
- "timeseries": time-bucketed aggregation by hour/day/week/month
- "search": full-text search across objects

Respond with ONLY a JSON object (no markdown, no explanation) matching this type:
{
  "action": "aggregate" | "timeseries" | "search",
  "collection": "string (collection name)",
  "function": "count" | "sum" | "avg" | "min" | "max" (omit for search)",
  "field": "string (field name for sum/avg/min/max, omit for count)",
  "groupBy": "string (field name to group by, optional)",
  "bucket": "hour" | "day" | "week" | "month" (only for timeseries)",
  "query": "string (search query, only for search action)",
  "limit": number (optional, max 100)
}}

Example: { "action": "aggregate", "collection": "orders", "function": "sum", "field": "revenue", "groupBy": "region" }`;

          const llmResponse = await fetch(`${llmEndpoint}/chat/completions`, {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              ...(llmApiKey ? { Authorization: `Bearer ${llmApiKey}` } : {}),
            },
            body: JSON.stringify({
              model: llmModel,
              messages: [
                { role: "system", content: systemPrompt },
                { role: "user", content: question },
              ],
              max_tokens: 1024,
              temperature: 0.1,
            }),
          });

          if (!llmResponse.ok) {
            const errText = await llmResponse.text();
            console.error("LLM request failed:", llmResponse.status, errText);
            sendError(res, 502, "LLM request failed");
            return;
          }

          const llmData = await llmResponse.json();
          const llmText = llmData.choices?.[0]?.message?.content;
          if (!llmText) {
            sendError(res, 502, "LLM returned no choices");
            return;
          }

          // Step 3: Parse intent
          const cleaned = llmText
            .trim()
            .replace(/^```(?:json)?\s*/, "")
            .replace(/\s*```$/, "")
            .trim();

          let intent: Record<string, unknown>;
          try {
            intent = JSON.parse(cleaned);
          } catch {
            sendError(res, 502, `Failed to parse LLM response as JSON: ${cleaned}`);
            return;
          }

          // Step 4: Execute
          let data: unknown;
          switch (intent.action) {
            case "aggregate": {
              const fn = (intent.function as string) || "count";
              const col = intent.collection as string;
              if (fn === "count") {
                data = await db.aggregate.count(col, {
                  groupBy: intent.groupBy as string | undefined,
                });
              } else if (fn === "sum") {
                data = await db.aggregate.sum(col, intent.field as string, {
                  groupBy: intent.groupBy as string | undefined,
                });
              } else if (fn === "avg") {
                data = await db.aggregate.avg(col, intent.field as string, {
                  groupBy: intent.groupBy as string | undefined,
                });
              } else if (fn === "min") {
                data = await db.aggregate.min(col, intent.field as string, {
                  groupBy: intent.groupBy as string | undefined,
                });
              } else {
                data = await db.aggregate.max(col, intent.field as string, {
                  groupBy: intent.groupBy as string | undefined,
                });
              }
              break;
            }
            case "timeseries": {
              const bucket = ((intent.bucket as string) || "day") as
                | "hour"
                | "day"
                | "week"
                | "month";
              data = await db.timeseries(intent.collection as string, {
                function: ((intent.function as string) || "count") as
                  | "count"
                  | "sum"
                  | "avg"
                  | "min"
                  | "max",
                bucket,
                field: intent.field as string | undefined,
              });
              break;
            }
            case "search": {
              data = await db.search((intent.query as string) || question, {
                collections: [intent.collection as string],
                limit: (intent.limit as number) || 10,
              });
              break;
            }
            default:
              sendError(res, 400, `Unknown action: ${intent.action}`);
              return;
          }

          const result = {
            answer: formatAnswer(intent, data),
            data,
            intent,
          };

          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify(result));
          return;
        }

        // GET /api/db/checkpoint
        if (pathname === "/api/db/checkpoint" && req.method === "GET") {
          try {
            const result = db.walCheckpoint();
            res.writeHead(200, { "Content-Type": "application/json" });
            res.end(JSON.stringify(result));
          } catch (e) {
            console.error("Checkpoint failed:", e);
            sendError(res, 400, "Checkpoint failed");
          }
          return;
        }

        // GET /api/db/integrity
        if (pathname === "/api/db/integrity" && req.method === "GET") {
          try {
            await db.countObjects();
            res.writeHead(200, { "Content-Type": "application/json" });
            res.end(JSON.stringify({ ok: true, message: "Database is accessible" }));
          } catch (e) {
            console.error("Integrity check failed:", e);
            res.writeHead(200, { "Content-Type": "application/json" });
            res.end(JSON.stringify({ ok: false, message: "Database integrity check failed" }));
          }
          return;
        }

        // POST /api/backup
        if (pathname === "/api/backup" && req.method === "POST") {
          try {
            let body = "";
            for await (const chunk of req) {
              body += chunk;
            }
            const { path: backupPath } = JSON.parse(body);
            if (!backupPath) {
              sendError(res, 400, "Missing 'path' in request body");
              return;
            }
            db.backupTo(backupPath);
            const { statSync } = await import("node:fs");
            const stats = statSync(backupPath);
            res.writeHead(200, { "Content-Type": "application/json" });
            res.end(JSON.stringify({ path: backupPath, sizeBytes: stats.size }));
          } catch (e) {
            console.error("Backup failed:", e);
            sendError(res, 500, "Backup failed");
          }
          return;
        }

        // GET /api/config/error-mode
        if (pathname === "/api/config/error-mode" && req.method === "GET") {
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify({ productionMode: false }));
          return;
        }

        sendError(res, 404, `Endpoint ${req.method} ${pathname} not found.`);
        return;
      }

      // REST API Routes (/v1/*)
      if (pathname.startsWith("/v1/")) {
        await handleRestRequest(db, req, res, pathname);
        return;
      }

      // Static File Server
      const targetFilePath = pathname === "/" ? "index.html" : pathname.replace(/^\//, "");
      const fullFilePath = join(publicDir, targetFilePath);

      // Security: ensure the resolved path is inside the public folder
      const relativePath = relative(publicDir, fullFilePath);
      if (isAbsolute(relativePath) || relativePath.startsWith("..")) {
        res.writeHead(403);
        res.end("Forbidden");
        return;
      }

      if (existsSync(fullFilePath)) {
        const fileContent = await fs.readFile(fullFilePath);
        const ext = extname(fullFilePath).toLowerCase();
        const contentType = MIME_TYPES[ext] || "application/octet-stream";

        res.writeHead(200, { "Content-Type": contentType });
        res.end(fileContent);
      } else {
        res.writeHead(404);
        res.end("Not Found");
      }
    } catch (err: unknown) {
      console.error("Dashboard server exception:", err);
      sendError(res, 500, "Internal server error");
    }
  });

  return new Promise((resolvePromise, rejectPromise) => {
    server.listen(port, () => {
      resolvePromise({
        server,
        close: async () => {
          await new Promise<void>((closeRes) => server.close(() => closeRes()));
          await db.close();
        },
      });
    });

    server.on("error", (err) => {
      rejectPromise(err);
    });
  });
}

function formatAnswer(intent: Record<string, unknown>, data: unknown): string {
  const fnName = (intent.function as string) || "count";
  const field = (intent.field as string) || "objects";
  switch (intent.action) {
    case "aggregate": {
      const d = data as { total?: number; groups?: unknown[] };
      const total = d?.total ?? 0;
      const groups = d?.groups?.length ?? 0;
      if (groups > 0) {
        return `${fnName} of ${field} = ${total}, grouped by ${intent.groupBy as string} into ${groups} groups`;
      }
      return `${fnName} of ${field} = ${total}`;
    }
    case "timeseries": {
      const d = data as { buckets?: unknown[] };
      return `Time series with ${d?.buckets?.length ?? 0} buckets`;
    }
    case "search": {
      const hits = (data as unknown[])?.length ?? 0;
      return `Found ${hits} results`;
    }
    default:
      return "Query executed.";
  }
}
