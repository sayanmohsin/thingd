import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import type {
  MemoryEvent,
  MemoryObject,
  QueueJob,
  StoredMemoryEvent,
  StoredMemoryObject,
} from "thingd";
import { type CliContext, hasFlag, requiredFlag, stringFlag, withDb, writeJson } from "./index.js";

const DEFAULT_REDACT_KEYS = [
  "password",
  "secret",
  "token",
  "key",
  "auth",
  "credential",
  "email",
  "phone",
  "session",
  "cookie",
  "signature",
  "private",
  "cert",
  "api",
];

function redactText(text: string): string {
  let res = text;
  // Redact emails
  res = res.replace(/[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/g, "[REDACTED_EMAIL]");
  // Redact API keys matching sk-...
  res = res.replace(/sk-[a-zA-Z0-9]{20,}/g, "[REDACTED_KEY]");
  // Redact Bearer tokens
  res = res.replace(/bearer\s+[a-zA-Z0-9\-._~+/]+=*/gi, "Bearer [REDACTED]");
  return res;
}

function redactValue(val: unknown, redactKeys: string[]): unknown {
  if (val === null || val === undefined) return val;

  if (Array.isArray(val)) {
    return val.map((item) => redactValue(item, redactKeys));
  }

  if (typeof val === "object") {
    const obj = val as Record<string, unknown>;
    const result: Record<string, unknown> = {};

    for (const [k, v] of Object.entries(obj)) {
      const lowerKey = k.toLowerCase();
      const shouldRedact = redactKeys.some((rKey) => lowerKey.includes(rKey.toLowerCase()));

      if (shouldRedact) {
        result[k] = "[REDACTED]";
      } else if (v && typeof v === "object") {
        result[k] = redactValue(v, redactKeys);
      } else if (typeof v === "string" && k === "text") {
        result[k] = redactText(v);
      } else {
        result[k] = v;
      }
    }
    return result;
  }

  return val;
}

export async function runExport(context: CliContext): Promise<void> {
  const isEvents = hasFlag(context.parsed, "events");
  const collection = stringFlag(context.parsed, "collection");
  const outPath = requiredFlag(context.parsed, "out");

  if (!isEvents && !collection) {
    throw new Error("Must specify either --collection <name> or --events for export.");
  }
  if (isEvents && collection) {
    throw new Error("Cannot specify both --collection <name> and --events.");
  }

  await withDb(context, async (db) => {
    const redactFlag = stringFlag(context.parsed, "redact");
    const isRedact = hasFlag(context.parsed, "redact");
    const redactKeys = isRedact
      ? redactFlag
        ? redactFlag.split(",").map((k) => k.trim())
        : DEFAULT_REDACT_KEYS
      : null;

    let lines: string[] = [];

    if (collection) {
      const objects = await db.listObjects(collection);
      lines = objects.map((obj) => {
        const finalObj = redactKeys ? (redactValue(obj, redactKeys) as typeof obj) : obj;
        return JSON.stringify(finalObj);
      });
    } else {
      const stream = stringFlag(context.parsed, "stream");
      const events = await db.events.list(stream);
      lines = events.map((ev) => {
        const finalEv = redactKeys ? (redactValue(ev, redactKeys) as typeof ev) : ev;
        return JSON.stringify(finalEv);
      });
    }

    writeFileSync(resolve(outPath), `${lines.join("\n")}\n`, "utf8");
    writeJson(context.stdout, { success: true, count: lines.length, out: outPath }, context.pretty);
  });
}

export async function runImport(context: CliContext): Promise<void> {
  const collection = requiredFlag(context.parsed, "collection");
  const inPath = requiredFlag(context.parsed, "in");

  const resolvedPath = resolve(inPath);
  if (!existsSync(resolvedPath)) {
    throw new Error(`Input file not found: ${inPath}`);
  }

  const content = readFileSync(resolvedPath, "utf8");
  const lines = content
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);

  await withDb(context, async (db) => {
    let count = 0;
    for (const line of lines) {
      const parsedObj = JSON.parse(line) as Record<string, unknown>;
      if (!parsedObj.id || typeof parsedObj.id !== "string") {
        throw new Error("Imported object must contain a string 'id' field.");
      }
      await db.put(collection, parsedObj as unknown as MemoryObject);
      count += 1;
    }
    writeJson(context.stdout, { success: true, count, collection }, context.pretty);
  });
}

export async function runSnapshot(context: CliContext): Promise<void> {
  const subCommand = context.parsed.tokens[1];
  if (subCommand === "create") {
    const outPath = requiredFlag(context.parsed, "out");
    await withDb(context, async (db) => {
      const collectionsMap: Record<string, StoredMemoryObject[]> = {};
      const cols = await db.listCollections();
      for (const col of cols) {
        collectionsMap[col] = await db.listObjects(col);
      }

      const eventsList = await db.events.list();

      const queuesMap: Record<string, { active: QueueJob[]; dead: QueueJob[] }> = {};
      const queues = await db.listQueues();
      for (const q of queues) {
        const queue = db.queue(q);
        const [active, dead] = await Promise.all([queue.list(), queue.dead()]);
        queuesMap[q] = { active, dead };
      }

      const snapshot = {
        version: "1.0.0",
        timestamp: new Date().toISOString(),
        collections: collectionsMap,
        events: eventsList,
        queues: queuesMap,
      };

      writeFileSync(resolve(outPath), JSON.stringify(snapshot, null, 2), "utf8");
      writeJson(context.stdout, { success: true, out: outPath }, context.pretty);
    });
  } else if (subCommand === "restore") {
    const inPath = requiredFlag(context.parsed, "in");
    const resolvedPath = resolve(inPath);
    if (!existsSync(resolvedPath)) {
      throw new Error(`Snapshot file not found: ${inPath}`);
    }

    const snapshot = JSON.parse(readFileSync(resolvedPath, "utf8"));
    if (snapshot.version !== "1.0.0") {
      throw new Error(`Unsupported snapshot version: ${snapshot.version}`);
    }

    await withDb(context, async (db) => {
      // 1. Restore Collections (clear existing first for true restore)
      if (snapshot.collections) {
        for (const [colName, objects] of Object.entries(snapshot.collections)) {
          const currentObjs = await db.listObjects(colName);
          for (const obj of currentObjs) {
            await db.delete(colName, obj.id);
          }
          for (const obj of objects as StoredMemoryObject[]) {
            const cleanObj = { ...obj } as Record<string, unknown>;
            delete cleanObj.collection;
            delete cleanObj.createdAt;
            delete cleanObj.updatedAt;
            delete cleanObj.version;
            await db.put(colName, cleanObj as unknown as MemoryObject);
          }
        }
      }

      // 2. Restore Events
      if (snapshot.events) {
        for (const ev of snapshot.events as StoredMemoryEvent[]) {
          const cleanEv = { ...ev } as Record<string, unknown>;
          delete cleanEv.id;
          delete cleanEv.createdAt;
          delete cleanEv.stream;
          await db.events.append(ev.stream, cleanEv as unknown as MemoryEvent);
        }
      }

      // 3. Restore Queues
      if (snapshot.queues) {
        for (const [qName, jobsData] of Object.entries(snapshot.queues)) {
          const queue = db.queue(qName);
          const { active, dead } = jobsData as { active: QueueJob[]; dead: QueueJob[] };

          for (const job of active) {
            await queue.push(job.payload, {
              idempotencyKey: job.id,
              maxAttempts: job.maxAttempts,
            });
          }
          for (const job of dead) {
            await queue.push(job.payload, {
              idempotencyKey: job.id,
              maxAttempts: job.maxAttempts,
            });
          }
        }
      }

      writeJson(context.stdout, { success: true, in: inPath }, context.pretty);
    });
  } else {
    throw new Error(`Unknown snapshot command: ${subCommand}. Expected 'create' or 'restore'.`);
  }
}
