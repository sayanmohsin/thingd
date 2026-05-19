import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type { MemoryD } from "@sayanmohsin/memoryd";
import { z } from "zod";
import {
  appendMcpAuditEvent,
  type MemorydMcpAuditMetadata,
  type MemorydMcpAuditOptions,
  resolveMemorydMcpAuditOptions,
} from "./audit.js";
import { jsonResult } from "./result.js";

const memoryObjectSchema = z.object({ id: z.string().min(1) }).catchall(z.unknown());
const memoryEventSchema = z.object({ type: z.string().min(1) }).catchall(z.unknown());
const objectPayloadSchema = z.record(z.string(), z.unknown());
const auditInputSchema = {
  actor: z.string().min(1).optional(),
  source: z.string().min(1).optional(),
};

export type RegisterMemorydToolsOptions = {
  audit?: MemorydMcpAuditOptions | false;
};

export function registerMemorydTools(
  server: McpServer,
  db: MemoryD,
  options: RegisterMemorydToolsOptions = {},
): void {
  const audit = resolveMemorydMcpAuditOptions(options.audit);

  server.registerTool(
    "memory.search",
    {
      title: "Search Memory",
      description: "Search memoryd objects and events by text.",
      inputSchema: {
        query: z.string().min(1),
        collections: z.array(z.string().min(1)).optional(),
        limit: z.number().int().positive().max(100).optional(),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ query, collections, limit }) =>
      jsonResult(await db.search(query, { collections, limit })),
  );

  server.registerTool(
    "memory.objects.get",
    {
      title: "Get Object",
      description: "Read one memoryd object by collection and id.",
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
    async ({ collection, id }) => jsonResult(await db.get(collection, id)),
  );

  server.registerTool(
    "memory.objects.put",
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
    "memory.objects.delete",
    {
      title: "Delete Object",
      description: "Delete one memoryd object by collection and id.",
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
    "memory.events.append",
    {
      title: "Append Event",
      description: "Append an event to a memoryd stream.",
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
    "memory.events.list",
    {
      title: "List Events",
      description: "List memoryd events, optionally filtered by stream.",
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
    "memory.queue.push",
    {
      title: "Push Queue Job",
      description: "Push a durable job onto a memoryd queue.",
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
    "memory.queue.claim",
    {
      title: "Claim Queue Job",
      description: "Claim the next ready job from a memoryd queue.",
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
    "memory.queue.ack",
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
    "memory.queue.nack",
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
    "memory.queue.list",
    {
      title: "List Queue Jobs",
      description: "List jobs in a memoryd queue.",
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
    "memory.queue.dead",
    {
      title: "List Dead Queue Jobs",
      description: "List dead-letter jobs in a memoryd queue.",
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
}

function auditMetadata(
  actor: string | undefined,
  source: string | undefined,
): MemorydMcpAuditMetadata {
  return {
    actor,
    source,
  };
}
