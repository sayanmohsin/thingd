import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import type {
  ConnectorAuth,
  ConnectorSchema,
  MemoryEvent,
  MemoryObject,
  QueueJob,
  StoredMemoryEvent,
  StoredMemoryObject,
} from "@thingd/sdk";
import { type CliContext, hasFlag, requiredFlag, stringFlag, withDb, writeJson } from "./index.js";

const SIDECAR_DEFAULT_URL = "http://localhost:8757";

type ConnectorType = "postgres" | "mysql";

type ConnectorPullResult = {
  imported: number;
  collection: string;
};

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
  if (val === null || val === undefined) {
    return val;
  }

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
  const source = stringFlag(context.parsed, "source") ?? context.parsed.tokens[1] ?? "";
  const type = stringFlag(context.parsed, "type");
  const isDb =
    source.startsWith("postgresql://") ||
    source.startsWith("postgres://") ||
    source.startsWith("mysql://") ||
    type === "postgres" ||
    type === "mysql";

  if (isDb) {
    await runImportDb(context, source, type);
    return;
  }

  // --- existing file import logic (unchanged) ---
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

  // Detect file type
  const isCsv = inPath.endsWith(".csv");

  await withDb(context, async (db) => {
    let count = 0;

    if (isCsv) {
      // Parse CSV
      const headers = lines[0]?.split(",").map((h) => h.trim()) ?? [];
      for (let i = 1; i < lines.length; i++) {
        const values = lines[i]?.split(",") ?? [];
        const obj: Record<string, unknown> = {};
        for (let j = 0; j < headers.length; j++) {
          const key = headers[j];
          if (!key) {
            continue;
          }
          const val = values[j]?.trim() ?? "";
          // Try to infer types
          if (val === "" || val === "null") {
            obj[key] = null;
          } else if (val === "true") {
            obj[key] = true;
          } else if (val === "false") {
            obj[key] = false;
          } else if (!Number.isNaN(Number(val))) {
            obj[key] = Number(val);
          } else {
            obj[key] = val;
          }
        }
        // Add row index as ID if not present
        if (!obj.id) {
          obj.id = `row-${i}`;
        }
        await db.put(collection, obj as unknown as MemoryObject);
        count += 1;
      }
    } else {
      // Parse JSONL
      for (const line of lines) {
        const parsedObj = JSON.parse(line) as Record<string, unknown>;
        if (!parsedObj.id || typeof parsedObj.id !== "string") {
          throw new Error("Imported object must contain a string 'id' field.");
        }
        await db.put(collection, parsedObj as unknown as MemoryObject);
        count += 1;
      }
    }

    writeJson(
      context.stdout,
      { success: true, count, collection, format: isCsv ? "csv" : "jsonl" },
      context.pretty
    );
  });
}

// ── DB import helpers ──────────────────────────────────────────────────

function resolveSidecarUrl(context: CliContext): string {
  const flag = stringFlag(context.parsed, "sidecar");
  if (flag) {
    return flag.replace(/\/$/, "");
  }
  const envUrl = context.env.THINGD_URL;
  if (envUrl) {
    const normalized = envUrl.replace(/\/$/, "");
    if (normalized.startsWith("http://") || normalized.startsWith("https://")) {
      return normalized;
    }
  }
  return SIDECAR_DEFAULT_URL;
}

function authHeaders(context: CliContext): Record<string, string> {
  const token = stringFlag(context.parsed, "auth-token") ?? context.env.THINGD_AUTH_TOKEN;
  return token ? { Authorization: `Bearer ${token}` } : {};
}

function detectConnectorType(source: string, explicitType?: string): ConnectorType {
  if (explicitType === "postgres" || explicitType === "mysql") {
    return explicitType;
  }
  if (source.startsWith("postgresql://") || source.startsWith("postgres://")) {
    return "postgres";
  }
  if (source.startsWith("mysql://")) {
    return "mysql";
  }
  throw new Error(
    `Could not detect connector type from "${source.slice(0, 30)}...". Use --type postgres or --type mysql.`
  );
}

function parseConnectionString(source: string): ConnectorAuth {
  const isUri =
    source.startsWith("postgresql://") ||
    source.startsWith("postgres://") ||
    source.startsWith("mysql://");

  if (isUri) {
    const url = new URL(source);
    const defaultPort = source.startsWith("mysql://") ? 3306 : 5432;
    return {
      host: url.hostname,
      port: url.port ? Number(url.port) : defaultPort,
      database: decodeURIComponent(url.pathname.replace(/^\//, "")),
      username: decodeURIComponent(url.username),
      password: decodeURIComponent(url.password),
      sslMode: (url.searchParams.get("sslmode") as ConnectorAuth["sslMode"]) ?? "prefer",
    };
  }

  // Shorthand: host:port:database:username:password
  const parts = source.split(":");
  return {
    host: parts[0] ?? "localhost",
    port: Number(parts[1]) || 5432,
    database: parts[2] ?? "",
    username: parts[3] ?? "",
    password: parts.slice(4).join(":"),
    sslMode: "prefer",
  };
}

function parseMappings(context: CliContext): [string, string][] {
  const maps = stringFlag(context.parsed, "map");
  if (!maps) {
    return [];
  }
  return maps.split(",").map((m) => {
    const parts = m.split(":");
    return [parts[0] ?? "", parts[1] ?? parts[0] ?? ""];
  });
}

function buildColumnMapping(pairs: [string, string][]): Record<string, string> | undefined {
  if (pairs.length === 0) {
    return undefined;
  }
  return Object.fromEntries(pairs);
}

async function callConnectorSchema(
  url: string,
  type: string,
  auth: ConnectorAuth,
  query: string,
  headers: Record<string, string>
): Promise<ConnectorSchema> {
  const res = await fetch(`${url}/v1/connectors/${type}/schema`, {
    method: "POST",
    headers: { "Content-Type": "application/json", ...headers },
    body: JSON.stringify({ auth, query }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => null);
    throw new Error(err?.error?.detail ?? `Schema discovery failed (HTTP ${res.status})`);
  }
  const json = await res.json();
  return json.data as ConnectorSchema;
}

async function callConnectorPull(
  url: string,
  type: string,
  auth: ConnectorAuth,
  opts: {
    collection: string;
    query: string;
    batchSize: number;
    columnMapping?: Record<string, string>;
  },
  headers: Record<string, string>
): Promise<ConnectorPullResult> {
  const res = await fetch(`${url}/v1/connectors/${type}/pull`, {
    method: "POST",
    headers: { "Content-Type": "application/json", ...headers },
    body: JSON.stringify({
      auth,
      collection: opts.collection,
      query: opts.query,
      batchSize: opts.batchSize,
      columnMapping: opts.columnMapping,
    }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => null);
    throw new Error(err?.error?.detail ?? `Import failed (HTTP ${res.status})`);
  }
  const json = await res.json();
  return json.data as ConnectorPullResult;
}

async function runImportDb(
  context: CliContext,
  source: string,
  explicitType?: string
): Promise<void> {
  const sidecarUrl = resolveSidecarUrl(context);
  const headers = authHeaders(context);
  const connectorType = detectConnectorType(source, explicitType);
  const auth = parseConnectionString(source);
  const collection = requiredFlag(context.parsed, "collection");
  const batchSize = Number(stringFlag(context.parsed, "batch-size") ?? "1000");
  const columnMapping = buildColumnMapping(parseMappings(context));

  // Verify sidecar is reachable
  try {
    const healthRes = await fetch(`${sidecarUrl}/v1/health`, { headers });
    if (!healthRes.ok) {
      throw new Error(`Sidecar returned HTTP ${healthRes.status}`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    throw new Error(
      `Cannot reach sidecar at ${sidecarUrl}: ${message}. ` +
        `Is thingd running? Try 'thingd mcp' first.`
    );
  }

  // --list-tables: show available tables
  if (hasFlag(context.parsed, "list-tables")) {
    const tables = await callConnectorSchema(sidecarUrl, connectorType, auth, "", headers);
    writeJson(context.stdout, { tables }, context.pretty);
    return;
  }

  // Resolve query from --tables or --query
  const tablesFlag = stringFlag(context.parsed, "tables");
  const queryFlag = stringFlag(context.parsed, "query");

  if (!queryFlag && !tablesFlag) {
    throw new Error("Must specify --tables (comma-separated) or --query for DB import.");
  }

  const queries: string[] = tablesFlag
    ? tablesFlag.split(",").map((t) => `SELECT * FROM ${t.trim()}`)
    : [queryFlag as string];

  // --dry-run: show schema without importing
  if (hasFlag(context.parsed, "dry-run")) {
    const schemas: ConnectorSchema[] = [];
    for (const q of queries) {
      const schema = await callConnectorSchema(sidecarUrl, connectorType, auth, q, headers);
      schemas.push(schema);
    }
    writeJson(context.stdout, { dryRun: true, schemas }, context.pretty);
    return;
  }

  // Execute imports for each query/table
  let totalImported = 0;
  for (const q of queries) {
    const result = await callConnectorPull(
      sidecarUrl,
      connectorType,
      auth,
      { collection, query: q, batchSize, columnMapping },
      headers
    );
    totalImported += result.imported;
  }

  writeJson(context.stdout, { success: true, imported: totalImported, collection }, context.pretty);
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

    context.stderr.write("Restoring snapshot... (consider 'thingd backup --out pre-restore.db' first)\n");

    await withDb(context, async (db) => {
      try {
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
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        // Restore failed — recommend using backup for atomic recovery
        context.stderr.write(
          `Restore failed: ${message}. Data may be partially restored.\n` +
            `For atomic restore, use 'thingd backup --out backup.db' before making changes\n` +
            `and copy the backup file to restore.\n`
        );
        throw err;
      }
    });
  } else {
    throw new Error(`Unknown snapshot command: ${subCommand}. Expected 'create' or 'restore'.`);
  }
}

export async function runBackup(context: CliContext): Promise<void> {
  const inPath = stringFlag(context.parsed, "in");
  const outPath = stringFlag(context.parsed, "out");

  if (inPath) {
    // Restore from backup file
    const resolvedIn = resolve(inPath);
    if (!existsSync(resolvedIn)) {
      throw new Error(`Backup file not found: ${inPath}`);
    }

    await withDb(context, async (db) => {
      if (db.path === ":memory:") {
        throw new Error("Cannot restore an in-memory database. Use a file-based SQLite database.");
      }
      // Close current DB, copy backup over, reopen happens after callback
      await db.close();
      const { copyFileSync } = await import("node:fs");
      copyFileSync(resolvedIn, db.path);
      context.stderr.write(`Restored from: ${resolvedIn}\n`);
    });
    return;
  }

  if (!outPath) {
    throw new Error("Expected --out <path> (backup) or --in <path> (restore).");
  }

  await withDb(context, async (db) => {
    if (db.path === ":memory:") {
      throw new Error("Cannot backup an in-memory database. Use a file-based SQLite database.");
    }

    db.backupTo(outPath);

    const { statSync } = await import("node:fs");
    const stats = statSync(outPath);
    const sizeMb = (stats.size / (1024 * 1024)).toFixed(2);

    context.stderr.write(`Backup created: ${outPath} (${sizeMb} MB)\n`);
  });
}
