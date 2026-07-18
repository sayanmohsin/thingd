import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type { ThingD } from "../thingd.js";
import {
  appendMcpAuditEvent,
  resolveThingdMcpAuditOptions,
  type ThingdMcpAuditMetadata,
  type ThingdMcpAuditOptions,
} from "./audit.js";
import type { ThingdMcpHardeningOptions } from "./config.js";
import { jsonResult } from "./result.js";

const memoryObjectSchema = z.object({ id: z.string().min(1) }).catchall(z.unknown());
const memoryEventSchema = z.object({ type: z.string().min(1) }).catchall(z.unknown());
const objectPayloadSchema = z.record(z.string(), z.unknown());
const auditInputSchema = {
  actor: z.string().min(1).optional(),
  source: z.string().min(1).optional(),
};

export type RegisterThingdToolsOptions = {
  audit?: ThingdMcpAuditOptions | false;
  hardening?: ThingdMcpHardeningOptions;
};

export function registerThingdTools(
  server: McpServer,
  db: ThingD,
  options: RegisterThingdToolsOptions = {}
): void {
  const audit = resolveThingdMcpAuditOptions(options.audit);
  const allowlist = options.hardening?.collectionAllowlist;
  const readOnly = options.hardening?.readOnly ?? false;

  /** Throw a tool-level error if the collection is not in the allowlist. */
  function assertCollectionAllowed(collection: string): void {
    if (allowlist && !allowlist.has(collection)) {
      throw new Error(
        `Collection "${collection}" is not permitted by this thingd MCP server. Allowed: ${[...allowlist].join(", ")}.`
      );
    }
  }

  /** Throw a tool-level error if the server is in read-only mode. */
  function assertWriteAllowed(): void {
    if (readOnly) {
      throw new Error(
        "This thingd MCP server is configured in read-only mode. Write operations are not permitted."
      );
    }
  }

  server.registerTool(
    "thing_search",
    {
      title: "Search Memory",
      description:
        "Search thingd objects and events by full-text query. Returns matching items ranked by relevance using SQLite FTS5 with Porter word stemming. Use this to find previously stored memories, notes, or events by keyword or phrase. Accepts a query string and optional filter by collection names, metadata key-value pairs, and a result limit. Returns an array of matching objects with relevance scores.",
      inputSchema: {
        query: z.string().min(1),
        collections: z.array(z.string().min(1)).optional(),
        limit: z.number().int().positive().max(100).optional(),
        filter: z.record(z.string(), z.unknown()).optional(),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ query, collections, limit, filter }) => {
      if (collections) {
        for (const c of collections) {
          assertCollectionAllowed(c);
        }
      }
      return jsonResult(await db.search(query, { collections, limit, filter }));
    }
  );

  server.registerTool(
    "thing_get",
    {
      title: "Get Object",
      description:
        "Read one thingd object by collection name and id. Returns the full object if found, or null if not found. Use this to retrieve a specific stored record when you know its exact collection and id. Returns a single object or null.",
      inputSchema: {
        collection: z.string().min(1),
        id: z.string().min(1),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ collection, id }) => {
      assertCollectionAllowed(collection);
      return jsonResult(await db.get(collection, id));
    }
  );

  server.registerTool(
    "thing_put",
    {
      title: "Put Object",
      description:
        "Create or replace one object-shaped memory record in a collection. The object must have an 'id' field. If an object with the same id already exists in the collection, it is replaced. Use this to store memories, notes, tasks, or any structured data. Returns the stored object with collection, version, and timestamps.",
      inputSchema: {
        collection: z.string().min(1),
        object: memoryObjectSchema,
        expectedVersion: z.number().int().nonnegative().optional(),
        ...auditInputSchema,
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async ({ collection, object, actor, source, expectedVersion }) => {
      assertWriteAllowed();
      assertCollectionAllowed(collection);
      const stored = await db.put(collection, object, { expectedVersion });
      await appendMcpAuditEvent(db, audit, {
        action: "objects.put",
        target: {
          collection,
          id: stored.id,
        },
        metadata: auditMetadata(actor, source),
        result: {
          collection: stored.collection,
          id: stored.id,
          version: stored.version,
        },
      });

      return jsonResult(stored);
    }
  );

  server.registerTool(
    "thing_delete",
    {
      title: "Delete Object",
      description:
        "Delete one thingd object by collection name and id. Permanently removes the object from the store. Returns a result indicating whether the deletion was successful. Use this to remove outdated or incorrect records.",
      inputSchema: {
        collection: z.string().min(1),
        id: z.string().min(1),
        ...auditInputSchema,
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: true,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ collection, id, actor, source }) => {
      assertWriteAllowed();
      assertCollectionAllowed(collection);
      const result = await db.delete(collection, id);
      await appendMcpAuditEvent(db, audit, {
        action: "objects.delete",
        target: {
          collection,
          id,
        },
        metadata: auditMetadata(actor, source),
        result,
      });

      return jsonResult(result);
    }
  );

  server.registerTool(
    "thing_events_append",
    {
      title: "Append Event",
      description:
        "Append an event to a named event stream. Events are append-only, ordered records with a 'type' field and arbitrary payload. Use this to record occurrences, state changes, or audit entries. Each event gets an auto-incremented sequence id. Returns the stored event with id, stream, and timestamp.",
      inputSchema: {
        stream: z.string().min(1),
        event: memoryEventSchema,
        ...auditInputSchema,
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async ({ stream, event, actor, source }) => {
      assertWriteAllowed();

      // Prevent direct writes to the protected audit stream
      if (stream === audit.stream) {
        throw new Error(`Stream '${stream}' is protected and cannot be written to directly`);
      }

      const stored = await db.events.append(stream, event);
      await appendMcpAuditEvent(db, audit, {
        action: "events.append",
        target: {
          stream,
          eventType: stored.type,
          eventId: stored.id,
        },
        metadata: auditMetadata(actor, source),
        result: {
          id: stored.id,
          stream: stored.stream,
        },
      });

      return jsonResult(stored);
    }
  );

  server.registerTool(
    "thing_events_list",
    {
      title: "List Events",
      description:
        "List events from a thingd stream, optionally filtered by stream name, starting from a specific sequence number, with a configurable limit, or filtered by timestamp. Use this to review recent activity, replay events, or audit changes. Returns an array of events ordered by sequence.",
      inputSchema: {
        stream: z.string().min(1).optional(),
        fromSequence: z.number().int().positive().optional(),
        limit: z.number().int().positive().optional(),
        since: z.string().optional(),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ stream, fromSequence, limit, since }) =>
      jsonResult(await db.events.list(stream, { fromSequence, limit, since }))
  );

  server.registerTool(
    "thing_queue_push",
    {
      title: "Push Queue Job",
      description:
        "Push a durable job onto a named queue. Jobs have a JSON payload and can include an idempotency key, max retry attempts, and a delay before the job becomes ready. Use this to schedule background work like processing, notifications, or data pipelines. Returns the created job with id, queue, and status.",
      inputSchema: {
        queue: z.string().min(1),
        payload: objectPayloadSchema,
        idempotencyKey: z.string().min(1).optional(),
        maxAttempts: z.number().int().positive().max(100).optional(),
        delayMs: z.number().int().min(0).optional(),
        ...auditInputSchema,
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async ({ queue, payload, idempotencyKey, maxAttempts, delayMs, actor, source }) => {
      assertWriteAllowed();
      const job = await db.queue(queue).push(payload, {
        idempotencyKey,
        maxAttempts,
        delayMs,
      });
      await appendMcpAuditEvent(db, audit, {
        action: "queue.push",
        target: {
          queue,
          id: job.id,
        },
        metadata: auditMetadata(actor, source),
        result: {
          id: job.id,
          queue: job.queue,
          status: job.status,
        },
      });

      return jsonResult(job);
    }
  );

  server.registerTool(
    "thing_queue_claim",
    {
      title: "Claim Queue Job",
      description:
        "Claim the next ready job from a queue. The job is leased for a configurable duration (default 30s). If not acked or nacked before the lease expires, it returns to the ready state. Returns the claimed job with payload, or null if no jobs are ready. Use this to process queue work in a worker loop.",
      inputSchema: {
        queue: z.string().min(1),
        leaseMs: z.number().int().optional(),
        ...auditInputSchema,
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async ({ queue, leaseMs, actor, source }) => {
      assertWriteAllowed();
      const job = await db.queue(queue).claim({ leaseMs });
      if (job) {
        await appendMcpAuditEvent(db, audit, {
          action: "queue.claim",
          target: {
            queue,
            id: job.id,
          },
          metadata: auditMetadata(actor, source),
          result: {
            id: job.id,
            queue: job.queue,
            status: job.status,
            attempts: job.attempts,
          },
        });
      }

      return jsonResult(job);
    }
  );

  server.registerTool(
    "thing_queue_ack",
    {
      title: "Acknowledge Queue Job",
      description:
        "Mark one leased queue job as completed. This removes the job from the queue permanently. Call this after successfully processing a claimed job. Returns a result with ok status and the final job status.",
      inputSchema: {
        queue: z.string().min(1),
        id: z.string().min(1),
        ...auditInputSchema,
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async ({ queue, id, actor, source }) => {
      assertWriteAllowed();
      const result = await db.queue(queue).ack(id);
      if (result.ok) {
        await appendMcpAuditEvent(db, audit, {
          action: "queue.ack",
          target: {
            queue,
            id,
          },
          metadata: auditMetadata(actor, source),
          result: {
            ok: true,
            status: result.job.status,
          },
        });
      }

      return jsonResult(result);
    }
  );

  server.registerTool(
    "thing_queue_nack",
    {
      title: "Reject Queue Job",
      description:
        "Reject a leased queue job for retry or dead-letter routing. If the job has remaining attempts, it goes back to ready (optionally after a delay). If attempts are exhausted, it moves to the dead-letter list. Optionally attach an error message. Returns a result with ok status and the updated job status.",
      inputSchema: {
        queue: z.string().min(1),
        id: z.string().min(1),
        delayMs: z.number().int().min(0).optional(),
        error: z.string().optional(),
        ...auditInputSchema,
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async ({ queue, id, delayMs, error, actor, source }) => {
      assertWriteAllowed();
      const result = await db.queue(queue).nack(id, { delayMs, error });
      if (result.ok) {
        await appendMcpAuditEvent(db, audit, {
          action: "queue.nack",
          target: {
            queue,
            id,
          },
          metadata: auditMetadata(actor, source),
          result: {
            ok: true,
            status: result.job.status,
          },
        });
      }

      return jsonResult(result);
    }
  );

  server.registerTool(
    "thing_queue_list",
    {
      title: "List Queue Jobs",
      description:
        "List all jobs in a queue across all states (ready, leased, dead-letter). Use this to inspect queue contents, monitor backlog, or debug stuck jobs. Returns an array of job objects with id, payload, status, attempts, and timestamps.",
      inputSchema: {
        queue: z.string().min(1),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ queue }) => jsonResult(await db.queue(queue).list())
  );

  server.registerTool(
    "thing_queue_dead",
    {
      title: "List Dead Queue Jobs",
      description:
        "List dead-letter jobs in a queue. These are jobs that exhausted all retry attempts. Use this to inspect failed work, diagnose errors, or decide whether to retry or discard. Returns an array of dead-letter job objects.",
      inputSchema: {
        queue: z.string().min(1),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ queue }) => jsonResult(await db.queue(queue).dead())
  );

  server.registerTool(
    "thing_objects_list",
    {
      title: "List Objects",
      description:
        "List objects in a collection with optional filtering, sorting, limit, and offset. Returns an array of objects. Use sortBy.field to sort by id, collection, created_at, updated_at, or version.",
      inputSchema: {
        collection: z.string().min(1),
        filter: z.record(z.string(), z.unknown()).optional(),
        sortBy: z
          .object({
            field: z.enum(["id", "collection", "created_at", "updated_at", "version"]),
            direction: z.enum(["asc", "desc"]).default("asc"),
          })
          .optional(),
        limit: z.number().int().positive().optional(),
        offset: z.number().int().nonnegative().optional(),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ collection, filter, sortBy, limit, offset }) => {
      assertCollectionAllowed(collection);
      return jsonResult(await db.listObjects(collection, { filter, sortBy, limit, offset }));
    }
  );

  server.registerTool(
    "thing_count_objects",
    {
      title: "Count Objects",
      description:
        "Count all objects stored across all collections. Returns a single number representing the total object count. Use this for quick inventory checks or monitoring storage usage.",
      inputSchema: {},
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async () => jsonResult(await db.countObjects())
  );

  server.registerTool(
    "thing_count_objects_in_collection",
    {
      title: "Count Objects in Collection",
      description:
        "Count objects in a specific collection. Returns a single number. Uses an indexed query for O(log n) performance.",
      inputSchema: {
        collection: z.string().describe("Collection name to count"),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ collection }) => jsonResult(await db.countObjectsInCollection(collection))
  );

  server.registerTool(
    "thing_count_events",
    {
      title: "Count Events",
      description:
        "Count all events across all streams. Returns a single number representing the total event count. Use this to monitor event volume or check stream activity.",
      inputSchema: {},
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async () => jsonResult(await db.countEvents())
  );

  server.registerTool(
    "thing_count_active_jobs",
    {
      title: "Count Active Jobs",
      description:
        "Count all active (non-dead) queue jobs across all queues. Includes ready, leased, and delayed jobs. Use this to monitor queue depth and worker load.",
      inputSchema: {},
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async () => jsonResult(await db.countActiveJobs())
  );

  server.registerTool(
    "thing_count_dead_jobs",
    {
      title: "Count Dead Jobs",
      description:
        "Count all dead-letter queue jobs across all queues. These are jobs that failed all retry attempts. Use this to monitor failure rates and decide when to investigate or discard.",
      inputSchema: {},
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async () => jsonResult(await db.countDeadJobs())
  );

  server.registerTool(
    "thing_list_collections",
    {
      title: "List Collections",
      description:
        "List all object collection names in the store. Returns an array of collection name strings. Use this to discover what data is stored before searching or querying.",
      inputSchema: {},
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async () => jsonResult(await db.listCollections())
  );

  server.registerTool(
    "thing_list_streams",
    {
      title: "List Streams",
      description:
        "List all event stream names in the store. Returns an array of stream name strings. Use this to discover available event streams before listing or appending events.",
      inputSchema: {},
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async () => jsonResult(await db.listStreams())
  );

  server.registerTool(
    "thing_list_queues",
    {
      title: "List Queues",
      description:
        "List all queue names in the store. Returns an array of queue name strings. Use this to discover available queues before pushing or claiming jobs.",
      inputSchema: {},
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async () => jsonResult(await db.listQueues())
  );

  server.registerTool(
    "thing_create_index",
    {
      title: "Create Index",
      description:
        "Create a functional index on a JSON body field for a collection. Subsequent listObjects calls with filter on this field will use the index for O(log n) lookups instead of full table scans. Idempotent — recreating an existing index is a no-op.",
      inputSchema: {
        collection: z.string().describe("Collection name"),
        field: z.string().describe("JSON body field name to index"),
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ collection, field }) => {
      await db.createIndex(collection, field);
      return jsonResult({ created: true });
    }
  );

  server.registerTool(
    "thing_list_indexes",
    {
      title: "List Indexes",
      description:
        "List all custom functional indexes. Returns an array of [collection, field] pairs.",
      inputSchema: {},
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async () => jsonResult(await db.listIndexes())
  );

  server.registerTool(
    "thing_objects_put_batch",
    {
      title: "Put Objects Batch",
      description:
        "Create or replace multiple objects in a collection in a single operation. More efficient than calling thing_put repeatedly. Returns the array of stored objects.",
      inputSchema: {
        collection: z.string().min(1),
        objects: z
          .array(z.object({ id: z.string() }).passthrough())
          .min(1)
          .max(1000),
        ...auditInputSchema,
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async ({ collection, objects, actor, source }) => {
      assertWriteAllowed();
      assertCollectionAllowed(collection);
      const result = await db.putBatch(collection, objects);
      await appendMcpAuditEvent(db, audit, {
        action: "objects.put_batch",
        target: { collection, count: objects.length },
        metadata: auditMetadata(actor, source),
        result: { count: result.length },
      });
      return jsonResult(result);
    }
  );

  server.registerTool(
    "thing_objects_delete_batch",
    {
      title: "Delete Objects Batch",
      description:
        "Delete multiple objects by ID in a single operation. Returns the count of deleted objects.",
      inputSchema: {
        collection: z.string().min(1),
        ids: z.array(z.string().min(1)).min(1).max(1000),
        ...auditInputSchema,
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: true,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ collection, ids, actor, source }) => {
      assertWriteAllowed();
      assertCollectionAllowed(collection);
      const deleted = await db.deleteBatch(collection, ids);
      await appendMcpAuditEvent(db, audit, {
        action: "objects.delete_batch",
        target: { collection, count: ids.length },
        metadata: auditMetadata(actor, source),
        result: { deleted },
      });
      return jsonResult({ deleted });
    }
  );

  server.registerTool(
    "thing_objects_get_batch",
    {
      title: "Get Objects Batch",
      description:
        "Read multiple objects by ID in a single operation. Returns an array of objects (null for missing IDs). More efficient than calling thing_get repeatedly.",
      inputSchema: {
        collection: z.string().min(1),
        ids: z.array(z.string().min(1)).min(1).max(1000),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ collection, ids }) => jsonResult(await db.getBatch(collection, ids))
  );

  server.registerTool(
    "thing_link_create",
    {
      title: "Create Link",
      description:
        "Create a directed graph link between two references (e.g., thingd objects, external URLs). You can optionally assign a linkType (e.g., 'parent', 'related_to'), weight, and metadata JSON string.",
      inputSchema: {
        fromRef: z.string().min(1),
        linkType: z.string().min(1),
        toRef: z.string().min(1),
        weight: z.number().optional(),
        metadataJson: z.string().optional(),
        ...auditInputSchema,
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async ({ fromRef, linkType, toRef, weight, metadataJson, actor, source }) => {
      assertWriteAllowed();
      const link = await db.links.create(fromRef, linkType, toRef, weight, metadataJson);
      await appendMcpAuditEvent(db, audit, {
        action: "link.create",
        target: { fromRef, linkType, toRef },
        metadata: auditMetadata(actor, source),
        result: { id: link.id },
      });
      return jsonResult(link);
    }
  );

  server.registerTool(
    "thing_link_delete",
    {
      title: "Delete Link",
      description: "Delete a graph link by its id. Returns an object with a deleted boolean.",
      inputSchema: {
        id: z.string().min(1),
        ...auditInputSchema,
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: true,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ id, actor, source }) => {
      assertWriteAllowed();
      const deleted = await db.links.delete(id);
      await appendMcpAuditEvent(db, audit, {
        action: "link.delete",
        target: { id },
        metadata: auditMetadata(actor, source),
        result: { deleted },
      });
      return jsonResult({ deleted });
    }
  );

  server.registerTool(
    "thing_link_get",
    {
      title: "Get Link",
      description: "Get a graph link by its id.",
      inputSchema: {
        id: z.string().min(1),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ id }) => jsonResult(await db.links.get(id))
  );

  server.registerTool(
    "thing_link_neighbors",
    {
      title: "Get Link Neighbors",
      description:
        "Get all links connected to a specific reference. You can filter by direction (Outgoing, Incoming, Both) and linkType.",
      inputSchema: {
        reference: z.string().min(1),
        direction: z.enum(["Outgoing", "Incoming", "Both"]).optional(),
        linkType: z.string().optional(),
        limit: z.number().int().positive().optional(),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ reference, direction, linkType, limit }) =>
      jsonResult(await db.links.neighbors(reference, direction, { linkType, limit }))
  );

  server.registerTool(
    "thing_link_count",
    {
      title: "Count Links",
      description: "Count all graph links in the store.",
      inputSchema: {},
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async () => jsonResult(await db.countLinks())
  );

  server.registerTool(
    "thing_aggregate",
    {
      title: "Aggregate Objects",
      description:
        "Run aggregate queries on objects in a collection. Supports count, sum, avg, min, max with optional groupBy and filter.",
      inputSchema: {
        collection: z.string().min(1),
        function: z.enum(["count", "sum", "avg", "min", "max"]),
        field: z.string().min(1).optional(),
        groupBy: z.string().min(1).optional(),
        filter: z.record(z.string(), z.unknown()).optional(),
        ...auditInputSchema,
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ collection, function: fn, field, groupBy, filter, actor, source }) => {
      assertCollectionAllowed(collection);
      let result: unknown;
      switch (fn) {
        case "sum":
          result = await db.aggregate.sum(collection, field ?? "", { groupBy, filter });
          break;
        case "avg":
          result = await db.aggregate.avg(collection, field ?? "", { groupBy, filter });
          break;
        case "min":
          result = await db.aggregate.min(collection, field ?? "", { groupBy, filter });
          break;
        case "max":
          result = await db.aggregate.max(collection, field ?? "", { groupBy, filter });
          break;
        default:
          result = await db.aggregate.count(collection, { groupBy, filter });
          break;
      }
      await appendMcpAuditEvent(db, audit, {
        action: "thing_aggregate",
        target: { collection, function: fn },
        metadata: { actor, source },
      });
      return jsonResult(result);
    }
  );

  server.registerTool(
    "thing_timeseries",
    {
      title: "Time Series Aggregation",
      description:
        "Run time-series aggregation on objects. Buckets objects by hour/day/week/month and applies an aggregate function.",
      inputSchema: {
        collection: z.string().min(1),
        function: z.enum(["count", "sum", "avg", "min", "max"]),
        bucket: z.enum(["hour", "day", "week", "month"]),
        field: z.string().min(1).optional(),
        filter: z.record(z.string(), z.unknown()).optional(),
        from: z.string().optional(),
        to: z.string().optional(),
        ...auditInputSchema,
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ collection, function: fn, bucket, field, filter, from, to, actor, source }) => {
      assertCollectionAllowed(collection);
      const result = await db.timeseries(collection, {
        function: fn,
        bucket,
        field,
        filter,
        from,
        to,
      });
      await appendMcpAuditEvent(db, audit, {
        action: "thing_timeseries",
        target: { collection, function: fn, bucket },
        metadata: { actor, source },
      });
      return jsonResult(result);
    }
  );

  server.registerTool(
    "thing_schema",
    {
      title: "Reflect Schema",
      description:
        "Reflect the schema of one or all collections by sampling stored objects. Returns inferred field names, types, and sample values.",
      inputSchema: {
        collection: z.string().min(1).optional(),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ collection }) => {
      if (collection) {
        assertCollectionAllowed(collection);
      }
      return jsonResult(await db.schema(collection));
    }
  );

  server.registerTool(
    "thing_nlq",
    {
      title: "Natural Language Query",
      description:
        "Ask a natural language question about your data. Requires NLQ configuration with an LLM endpoint.",
      inputSchema: {
        question: z.string().min(1),
        collection: z.string().min(1).optional(),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ question, collection }) => {
      return jsonResult(await db.nlq.query(question, { collection }));
    }
  );
}

function auditMetadata(
  actor: string | undefined,
  source: string | undefined
): ThingdMcpAuditMetadata {
  return {
    actor,
    source,
  };
}
