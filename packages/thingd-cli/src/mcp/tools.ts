import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type { ThingD } from "thingd";
import { z } from "zod";
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
  options: RegisterThingdToolsOptions = {},
): void {
  const audit = resolveThingdMcpAuditOptions(options.audit);
  const allowlist = options.hardening?.collectionAllowlist;
  const readOnly = options.hardening?.readOnly ?? false;

  /** Throw a tool-level error if the collection is not in the allowlist. */
  function assertCollectionAllowed(collection: string): void {
    if (allowlist && !allowlist.has(collection)) {
      throw new Error(
        `Collection "${collection}" is not permitted by this thingd MCP server. Allowed: ${[...allowlist].join(", ")}.`,
      );
    }
  }

  /** Throw a tool-level error if the server is in read-only mode. */
  function assertWriteAllowed(): void {
    if (readOnly) {
      throw new Error(
        "This thingd MCP server is configured in read-only mode. Write operations are not permitted.",
      );
    }
  }

  server.registerTool(
    "thing_search",
    {
      title: "Search Memory",
      description: "Search thingd objects and events by text.",
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
        for (const c of collections) assertCollectionAllowed(c);
      }
      return jsonResult(await db.search(query, { collections, limit, filter }));
    },
  );

  server.registerTool(
    "thing_get",
    {
      title: "Get Object",
      description: "Read one thingd object by collection and id.",
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
    },
  );

  server.registerTool(
    "thing_put",
    {
      title: "Put Object",
      description: "Create or replace one object-shaped memory record.",
      inputSchema: {
        collection: z.string().min(1),
        object: memoryObjectSchema,
        ...auditInputSchema,
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async ({ collection, object, actor, source }) => {
      assertWriteAllowed();
      assertCollectionAllowed(collection);
      const stored = await db.put(collection, object);
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
    },
  );

  server.registerTool(
    "thing_delete",
    {
      title: "Delete Object",
      description: "Delete one thingd object by collection and id.",
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
    },
  );

  server.registerTool(
    "thing_events_append",
    {
      title: "Append Event",
      description: "Append an event to a thingd stream.",
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
    },
  );

  server.registerTool(
    "thing_events_list",
    {
      title: "List Events",
      description: "List thingd events, optionally filtered by stream.",
      inputSchema: {
        stream: z.string().min(1).optional(),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ stream }) => jsonResult(await db.events.list(stream)),
  );

  server.registerTool(
    "thing_queue_push",
    {
      title: "Push Queue Job",
      description: "Push a durable job onto a thingd queue.",
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
    },
  );

  server.registerTool(
    "thing_queue_claim",
    {
      title: "Claim Queue Job",
      description: "Claim the next ready job from a thingd queue.",
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
    },
  );

  server.registerTool(
    "thing_queue_ack",
    {
      title: "Acknowledge Queue Job",
      description: "Mark one leased queue job as completed.",
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
    },
  );

  server.registerTool(
    "thing_queue_nack",
    {
      title: "Reject Queue Job",
      description: "Reject a leased queue job for retry or dead-letter routing.",
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
    },
  );

  server.registerTool(
    "thing_queue_list",
    {
      title: "List Queue Jobs",
      description: "List jobs in a thingd queue.",
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
    async ({ queue }) => jsonResult(await db.queue(queue).list()),
  );

  server.registerTool(
    "thing_queue_dead",
    {
      title: "List Dead Queue Jobs",
      description: "List dead-letter jobs in a thingd queue.",
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
    async ({ queue }) => jsonResult(await db.queue(queue).dead()),
  );

  server.registerTool(
    "thing_objects_list",
    {
      title: "List Objects",
      description: "List all thingd objects in a collection.",
      inputSchema: {
        collection: z.string().min(1),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ collection }) => {
      assertCollectionAllowed(collection);
      return jsonResult(await db.listObjects(collection));
    },
  );

  server.registerTool(
    "thing_count_objects",
    {
      title: "Count Objects",
      description: "Count all objects across all collections.",
      inputSchema: {},
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async () => jsonResult(await db.countObjects()),
  );

  server.registerTool(
    "thing_count_events",
    {
      title: "Count Events",
      description: "Count all events across all streams.",
      inputSchema: {},
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async () => jsonResult(await db.countEvents()),
  );

  server.registerTool(
    "thing_count_active_jobs",
    {
      title: "Count Active Jobs",
      description: "Count all active (non-dead) queue jobs.",
      inputSchema: {},
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async () => jsonResult(await db.countActiveJobs()),
  );

  server.registerTool(
    "thing_count_dead_jobs",
    {
      title: "Count Dead Jobs",
      description: "Count all dead-letter queue jobs.",
      inputSchema: {},
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async () => jsonResult(await db.countDeadJobs()),
  );

  server.registerTool(
    "thing_list_collections",
    {
      title: "List Collections",
      description: "List all object collection names.",
      inputSchema: {},
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async () => jsonResult(await db.listCollections()),
  );

  server.registerTool(
    "thing_list_streams",
    {
      title: "List Streams",
      description: "List all event stream names.",
      inputSchema: {},
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async () => jsonResult(await db.listStreams()),
  );

  server.registerTool(
    "thing_list_queues",
    {
      title: "List Queues",
      description: "List all queue names.",
      inputSchema: {},
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async () => jsonResult(await db.listQueues()),
  );
}

function auditMetadata(
  actor: string | undefined,
  source: string | undefined,
): ThingdMcpAuditMetadata {
  return {
    actor,
    source,
  };
}
