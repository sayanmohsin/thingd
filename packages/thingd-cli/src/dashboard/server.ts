import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { join, dirname, extname } from "node:path";
import { fileURLToPath } from "node:url";
import { existsSync, promises as fs, statSync } from "node:fs";
import { ThingD } from "thingd";
import type { ConnectionOptions } from "../index.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

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
  port: number,
): Promise<{ server: any; close: () => Promise<void> }> {
  // 1. Maintain dynamic active database options
  let activeOptions = { ...connectionOptions };
  let db = await ThingD.open({
    path: activeOptions.path,
    url: activeOptions.cloud ? activeOptions.path : undefined,
    driver: activeOptions.driver,
    authToken: activeOptions.authToken,
  });

  // 2. Create HTTP Server
  const server = createServer(async (req, res) => {
    try {
      const url = new URL(req.url || "", `http://${req.headers.host || "localhost"}`);
      const pathname = url.pathname;

      // Handle CORS for ease of developer integrations
      res.setHeader("Access-Control-Allow-Origin", "*");
      res.setHeader("Access-Control-Allow-Methods", "GET, POST, DELETE, OPTIONS");
      res.setHeader("Access-Control-Allow-Headers", "Content-Type, Authorization");

      if (req.method === "OPTIONS") {
        res.writeHead(204);
        res.end();
        return;
      }

      // Security Gate middleware for API endpoints
      const isApiRoute = pathname.startsWith("/api/");
      const isConnectRoute = pathname === "/api/connect";

      if (isApiRoute && !isConnectRoute && activeOptions.authToken) {
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
          const { path, driver, authToken } = JSON.parse(bodyStr);

          if (!path || !driver) {
            sendError(res, 400, "Fields 'path' and 'driver' are required.");
            return;
          }

          // Safely shut down the old db instance
          await db.close();

          // Spawn new db connection dynamically
          db = await ThingD.open({
            path,
            url: isCloudPath(path) ? path : undefined,
            driver,
            authToken,
          });

          activeOptions = {
            path,
            driver,
            authToken,
            cloud: isCloudPath(path),
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
              mode: activeOptions.cloud ? "cloud" : "local",
              driver: activeOptions.driver || "memory",
              path: activeOptions.path,
              metrics: { objects, events, activeJobs, deadJobs, dbSize },
              authRequired: !!activeOptions.authToken,
            }),
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
            }),
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

        sendError(res, 404, `Endpoint ${req.method} ${pathname} not found.`);
        return;
      }

      // Static File Server
      const targetFilePath = pathname === "/" ? "index.html" : pathname.replace(/^\//, "");
      const fullFilePath = join(publicDir, targetFilePath);

      // Security: ensure the resolved path is inside the public folder
      if (!fullFilePath.startsWith(publicDir)) {
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
    } catch (err: any) {
      console.error("Dashboard server exception:", err);
      sendError(res, 500, err?.message || String(err));
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
