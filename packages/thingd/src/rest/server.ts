import type { IncomingMessage, ServerResponse } from "node:http";
import type { ThingD } from "../thingd.js";
import { SDK_VERSION } from "../version.js";
import {
  parseFilter,
  parseIntParam,
  parseSortBy,
  readBody,
  sendData,
  sendDataList,
  sendError,
} from "./helpers.js";

type RouteMatch = {
  collection?: string;
  id?: string;
  queue?: string;
  stream?: string;
  name?: string;
};

function matchRoute(pathname: string, pattern: string): RouteMatch | null {
  const patternParts = pattern.split("/");
  const pathParts = pathname.split("/");

  if (patternParts.length !== pathParts.length) {
    return null;
  }

  const match: RouteMatch = {};
  for (let i = 0; i < patternParts.length; i++) {
    const pp = patternParts[i] ?? "";
    const xp = pathParts[i] ?? "";
    if (pp.startsWith(":")) {
      const key = pp.slice(1) as keyof RouteMatch;
      match[key] = xp;
    } else if (pp !== xp) {
      return null;
    }
  }
  return match;
}

export async function handleRestRequest(
  db: ThingD,
  req: IncomingMessage,
  res: ServerResponse,
  pathname: string
): Promise<void> {
  const method = req.method ?? "GET";
  const url = new URL(req.url || "/", `http://${req.headers.host || "localhost"}`);

  try {
    // ─── Health ──────────────────────────────────────────────────
    if (pathname === "/v1/health" && method === "GET") {
      const [objects, events, links, queues, collections, streams] = await Promise.all([
        db.countObjects(),
        db.countEvents(),
        db.countLinks(),
        db.listQueues(),
        db.listCollections(),
        db.listStreams(),
      ]);
      sendData(res, {
        status: "ok",
        version: SDK_VERSION,
        counts: {
          objects,
          events,
          links,
          queues: queues.length,
          collections: collections.length,
          streams: streams.length,
        },
      });
      return;
    }

    // ─── Counts ──────────────────────────────────────────────────
    if (pathname === "/v1/counts/objects" && method === "GET") {
      sendData(res, { count: await db.countObjects() });
      return;
    }
    if (pathname === "/v1/counts/events" && method === "GET") {
      sendData(res, { count: await db.countEvents() });
      return;
    }
    if (pathname === "/v1/counts/links" && method === "GET") {
      sendData(res, { count: await db.countLinks() });
      return;
    }

    // ─── Collections / Streams / Queues ──────────────────────────
    if (pathname === "/v1/collections" && method === "GET") {
      sendDataList(res, await db.listCollections());
      return;
    }

    // GET /v1/collections/schema — all schemas
    if (pathname === "/v1/collections/schema" && method === "GET") {
      sendDataList(res, await db.schema());
      return;
    }

    // GET /v1/collections/:name/schema — single schema
    const schemaMatch = matchRoute(pathname, "/v1/collections/:name/schema");
    if (schemaMatch?.name && method === "GET") {
      const schemas = await db.schema(schemaMatch.name);
      if (schemas.length === 0) {
        sendError(
          res,
          404,
          "not_found",
          `Collection '${schemaMatch.name}' not found or has no objects`
        );
        return;
      }
      sendData(res, schemas[0]);
      return;
    }

    if (pathname === "/v1/streams" && method === "GET") {
      sendDataList(res, await db.listStreams());
      return;
    }
    if (pathname === "/v1/queues" && method === "GET") {
      sendDataList(res, await db.listQueues());
      return;
    }

    // ─── Objects ─────────────────────────────────────────────────
    // GET /v1/objects?collection=...&filter.x=...&sortBy=...&limit=...&offset=...
    if (pathname === "/v1/objects" && method === "GET") {
      const collection = url.searchParams.get("collection");
      if (!collection) {
        sendError(res, 400, "bad_request", "Query parameter 'collection' is required");
        return;
      }
      const filter = parseFilter(url.searchParams);
      const sortBy = parseSortBy(url.searchParams);
      const limit = parseIntParam(url.searchParams.get("limit"));
      const offset = parseIntParam(url.searchParams.get("offset"));
      const objects = await db.listObjects(collection, { filter, sortBy, limit, offset });
      sendDataList(res, objects);
      return;
    }

    // PUT /v1/objects/:collection/:id
    const objMatch = matchRoute(pathname, "/v1/objects/:collection/:id");
    if (objMatch?.collection && objMatch?.id && method === "PUT") {
      const body = JSON.parse(await readBody(req));
      body.id = objMatch.id;
      const result = await db.put(objMatch.collection, body);
      sendData(res, result);
      return;
    }

    // GET /v1/objects/:collection/:id
    if (objMatch?.collection && objMatch?.id && method === "GET") {
      const result = await db.get(objMatch.collection, objMatch.id);
      if (!result) {
        sendError(
          res,
          404,
          "not_found",
          `Object '${objMatch.id}' not found in collection '${objMatch.collection}'`
        );
        return;
      }
      sendData(res, result);
      return;
    }

    // DELETE /v1/objects/:collection/:id
    if (objMatch?.collection && objMatch?.id && method === "DELETE") {
      const result = await db.delete(objMatch.collection, objMatch.id);
      sendData(res, result);
      return;
    }

    // GET /v1/objects/batch?collection=...
    if (pathname === "/v1/objects/batch" && method === "GET") {
      const collection = url.searchParams.get("collection");
      if (!collection) {
        sendError(res, 400, "bad_request", "Query parameter 'collection' is required");
        return;
      }
      const body = JSON.parse(await readBody(req));
      const ids = Array.isArray(body) ? body : body.ids;
      if (!Array.isArray(ids)) {
        sendError(res, 400, "bad_request", "Body must be an array or { ids: [...] }");
        return;
      }
      const objects = await db.getBatch(collection, ids);
      sendDataList(res, objects);
      return;
    }

    // PUT /v1/objects/batch?collection=...
    if (pathname === "/v1/objects/batch" && method === "PUT") {
      const collection = url.searchParams.get("collection");
      if (!collection) {
        sendError(res, 400, "bad_request", "Query parameter 'collection' is required");
        return;
      }
      const body = JSON.parse(await readBody(req));
      const objects = Array.isArray(body) ? body : body.objects;
      if (!Array.isArray(objects)) {
        sendError(res, 400, "bad_request", "Body must be an array or { objects: [...] }");
        return;
      }
      const result = await db.putBatch(collection, objects);
      sendData(res, result);
      return;
    }

    // DELETE /v1/objects/batch?collection=...
    if (pathname === "/v1/objects/batch" && method === "DELETE") {
      const collection = url.searchParams.get("collection");
      if (!collection) {
        sendError(res, 400, "bad_request", "Query parameter 'collection' is required");
        return;
      }
      const body = JSON.parse(await readBody(req));
      const ids = Array.isArray(body) ? body : body.ids;
      if (!Array.isArray(ids)) {
        sendError(res, 400, "bad_request", "Body must be an array or { ids: [...] }");
        return;
      }
      const count = await db.deleteBatch(collection, ids);
      sendData(res, { deleted: count });
      return;
    }

    // ─── Search ──────────────────────────────────────────────────
    if (pathname === "/v1/search" && method === "POST") {
      const body = JSON.parse(await readBody(req));
      if (!body.query) {
        sendError(res, 400, "bad_request", "Field 'query' is required");
        return;
      }
      const results = await db.search(body.query, {
        collections: body.collections,
        limit: body.limit,
        filter: body.filter,
      });
      sendData(res, results);
      return;
    }

    // ─── Aggregate ──────────────────────────────────────────────
    if (pathname === "/v1/aggregate" && method === "POST") {
      const body = JSON.parse(await readBody(req));
      if (!body.collection) {
        sendError(res, 400, "bad_request", "Field 'collection' is required");
        return;
      }
      if (!body.function) {
        sendError(res, 400, "bad_request", "Field 'function' is required");
        return;
      }
      let result: unknown;
      switch (body.function) {
        case "sum":
          result = await db.aggregate.sum(body.collection, body.field ?? "", {
            groupBy: body.groupBy,
            filter: body.filter,
          });
          break;
        case "avg":
          result = await db.aggregate.avg(body.collection, body.field ?? "", {
            groupBy: body.groupBy,
            filter: body.filter,
          });
          break;
        case "min":
          result = await db.aggregate.min(body.collection, body.field ?? "", {
            groupBy: body.groupBy,
            filter: body.filter,
          });
          break;
        case "max":
          result = await db.aggregate.max(body.collection, body.field ?? "", {
            groupBy: body.groupBy,
            filter: body.filter,
          });
          break;
        default:
          result = await db.aggregate.count(body.collection, {
            groupBy: body.groupBy,
            filter: body.filter,
          });
          break;
      }
      sendData(res, result);
      return;
    }
    if (pathname === "/v1/aggregate/timeseries" && method === "POST") {
      const body = JSON.parse(await readBody(req));
      if (!body.collection) {
        sendError(res, 400, "bad_request", "Field 'collection' is required");
        return;
      }
      if (!body.function) {
        sendError(res, 400, "bad_request", "Field 'function' is required");
        return;
      }
      if (!body.bucket) {
        sendError(res, 400, "bad_request", "Field 'bucket' is required");
        return;
      }
      const result = await db.timeseries(body.collection, {
        function: body.function,
        bucket: body.bucket,
        field: body.field,
        filter: body.filter,
        from: body.from,
        to: body.to,
      });
      sendData(res, result);
      return;
    }

    // ─── NLQ ──────────────────────────────────────────────────
    if (pathname === "/v1/nlq" && method === "POST") {
      const body = JSON.parse(await readBody(req));
      if (!body.question) {
        sendError(res, 400, "bad_request", "Field 'question' is required");
        return;
      }
      const result = await db.nlq.query(body.question, {
        collection: body.collection,
      });
      sendData(res, result);
      return;
    }

    // ─── Events ──────────────────────────────────────────────────
    // POST /v1/events/:stream
    const streamMatch = matchRoute(pathname, "/v1/events/:stream");
    if (streamMatch?.stream && method === "POST") {
      const body = JSON.parse(await readBody(req));
      if (!body.type) {
        sendError(res, 400, "bad_request", "Field 'type' is required");
        return;
      }
      const event = await db.events.append(streamMatch.stream, body);
      sendData(res, event);
      return;
    }

    // GET /v1/events?stream=...&fromSequence=...&limit=...&since=...
    if (pathname === "/v1/events" && method === "GET") {
      const stream = url.searchParams.get("stream") ?? undefined;
      const fromSequence = parseIntParam(url.searchParams.get("fromSequence"));
      const limit = parseIntParam(url.searchParams.get("limit"));
      const since = url.searchParams.get("since") ?? undefined;
      const events = await db.events.list(stream, { fromSequence, limit, since });
      sendDataList(res, events);
      return;
    }

    // ─── Queues ──────────────────────────────────────────────────
    // POST /v1/queues/:queue/push
    const pushMatch = matchRoute(pathname, "/v1/queues/:queue/push");
    if (pushMatch?.queue && method === "POST") {
      const body = JSON.parse(await readBody(req));
      const job = await db.queue(pushMatch.queue).push(body.payload ?? body, {
        idempotencyKey: body.idempotencyKey,
        maxAttempts: body.maxAttempts,
        delayMs: body.delayMs,
      });
      sendData(res, job);
      return;
    }

    // POST /v1/queues/:queue/claim
    const claimMatch = matchRoute(pathname, "/v1/queues/:queue/claim");
    if (claimMatch?.queue && method === "POST") {
      const body = JSON.parse(await readBody(req));
      const job = await db.queue(claimMatch.queue).claim({
        leaseMs: body.leaseMs,
      });
      if (!job) {
        sendData(res, null);
        return;
      }
      sendData(res, job);
      return;
    }

    // POST /v1/queues/:queue/ack
    const ackMatch = matchRoute(pathname, "/v1/queues/:queue/ack");
    if (ackMatch?.queue && method === "POST") {
      const body = JSON.parse(await readBody(req));
      const result = await db.queue(ackMatch.queue).ack(body.jobId);
      if (!result.ok) {
        sendError(res, 400, result.reason, `Ack failed: ${result.reason}`);
        return;
      }
      sendData(res, result.job);
      return;
    }

    // POST /v1/queues/:queue/nack
    const nackMatch = matchRoute(pathname, "/v1/queues/:queue/nack");
    if (nackMatch?.queue && method === "POST") {
      const body = JSON.parse(await readBody(req));
      const result = await db.queue(nackMatch.queue).nack(body.jobId, {
        delayMs: body.delayMs,
        error: body.error,
      });
      if (!result.ok) {
        sendError(res, 400, result.reason, `Nack failed: ${result.reason}`);
        return;
      }
      sendData(res, result.job);
      return;
    }

    // GET /v1/queues/:queue/jobs
    const jobsMatch = matchRoute(pathname, "/v1/queues/:queue/jobs");
    if (jobsMatch?.queue && method === "GET") {
      const jobs = await db.queue(jobsMatch.queue).list();
      sendDataList(res, jobs);
      return;
    }

    // GET /v1/queues/:queue/dead
    const deadMatch = matchRoute(pathname, "/v1/queues/:queue/dead");
    if (deadMatch?.queue && method === "GET") {
      const jobs = await db.queue(deadMatch.queue).dead();
      sendDataList(res, jobs);
      return;
    }

    // ─── Links ───────────────────────────────────────────────────
    // POST /v1/links
    if (pathname === "/v1/links" && method === "POST") {
      const body = JSON.parse(await readBody(req));
      if (!body.fromRef || !body.linkType || !body.toRef) {
        sendError(res, 400, "bad_request", "Fields 'fromRef', 'linkType', 'toRef' are required");
        return;
      }
      const link = await db.links.create(
        body.fromRef,
        body.linkType,
        body.toRef,
        body.weight,
        body.metadataJson
      );
      sendData(res, link);
      return;
    }

    // GET /v1/links?id=...
    if (pathname === "/v1/links" && method === "GET") {
      const id = url.searchParams.get("id");
      if (id) {
        const link = await db.links.get(id);
        if (!link) {
          sendError(res, 404, "not_found", `Link '${id}' not found`);
          return;
        }
        sendData(res, link);
        return;
      }

      // Neighbors query
      const reference = url.searchParams.get("reference");
      if (reference) {
        const direction =
          (url.searchParams.get("direction") as "Outgoing" | "Incoming" | "Both") ?? "Both";
        const linkType = url.searchParams.get("linkType") ?? undefined;
        const limit = parseIntParam(url.searchParams.get("limit"));
        const neighbors = await db.links.neighbors(reference, direction, { linkType, limit });
        sendDataList(res, neighbors);
        return;
      }

      sendError(res, 400, "bad_request", "Query parameter 'id' or 'reference' is required");
      return;
    }

    // DELETE /v1/links/:id
    const linkDeleteMatch = matchRoute(pathname, "/v1/links/:id");
    if (linkDeleteMatch?.id && method === "DELETE") {
      const deleted = await db.links.delete(linkDeleteMatch.id);
      sendData(res, { deleted });
      return;
    }

    // GET /v1/links/:id
    if (linkDeleteMatch?.id && method === "GET") {
      const link = await db.links.get(linkDeleteMatch.id);
      if (!link) {
        sendError(res, 404, "not_found", `Link '${linkDeleteMatch.id}' not found`);
        return;
      }
      sendData(res, link);
      return;
    }

    // ─── 404 ─────────────────────────────────────────────────────
    sendError(res, 404, "not_found", `No route for ${method} ${pathname}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    sendError(res, 500, "internal_error", message);
  }
}
