import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type { MemoryD } from "@sayanmohsin/memoryd";
import { z } from "zod";
import { jsonResult } from "./result.js";

const memoryObjectSchema = z.object({ id: z.string().min(1) }).catchall(z.unknown());
const memoryEventSchema = z.object({ type: z.string().min(1) }).catchall(z.unknown());
const objectPayloadSchema = z.record(z.string(), z.unknown());

export function registerMemorydTools(server: McpServer, db: MemoryD): void {
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
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async ({ collection, object }) => jsonResult(await db.put(collection, object)),
  );

  server.registerTool(
    "memory.objects.delete",
    {
      title: "Delete Object",
      description: "Delete one memoryd object by collection and id.",
      inputSchema: {
        collection: z.string().min(1),
        id: z.string().min(1),
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: true,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ collection, id }) => jsonResult(await db.delete(collection, id)),
  );

  server.registerTool(
    "memory.events.append",
    {
      title: "Append Event",
      description: "Append an event to a memoryd stream.",
      inputSchema: {
        stream: z.string().min(1),
        event: memoryEventSchema,
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async ({ stream, event }) => jsonResult(await db.events.append(stream, event)),
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
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async ({ queue, payload, idempotencyKey, maxAttempts, delayMs }) =>
      jsonResult(
        await db.queue(queue).push(payload, {
          idempotencyKey,
          maxAttempts,
          delayMs,
        }),
      ),
  );

  server.registerTool(
    "memory.queue.claim",
    {
      title: "Claim Queue Job",
      description: "Claim the next ready job from a memoryd queue.",
      inputSchema: {
        queue: z.string().min(1),
        leaseMs: z.number().int().optional(),
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async ({ queue, leaseMs }) => jsonResult(await db.queue(queue).claim({ leaseMs })),
  );

  server.registerTool(
    "memory.queue.ack",
    {
      title: "Acknowledge Queue Job",
      description: "Mark one leased queue job as completed.",
      inputSchema: {
        queue: z.string().min(1),
        id: z.string().min(1),
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async ({ queue, id }) => jsonResult(await db.queue(queue).ack(id)),
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
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async ({ queue, id, delayMs, error }) =>
      jsonResult(await db.queue(queue).nack(id, { delayMs, error })),
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
