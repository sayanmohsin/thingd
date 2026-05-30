#!/usr/bin/env node

import { realpathSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import {
  type MemoryEvent,
  type MemoryObject,
  type MemorySearchOptions,
  type QueueClaimOptions,
  type QueueJobOptions,
  type QueueNackOptions,
  ThingD,
  type ThingDDriver,
} from "thingd";
import { runInteractiveCli } from "./interactive.js";
import { runMcp } from "./mcp.js";
import { defaultThingdDbPath, ensureThingdDir } from "./paths.js";

type CliEnv = Record<string, string | undefined>;

type WritableLike = {
  write(chunk: string): void;
};

export type RunCliOptions = {
  env?: CliEnv;
  stdout?: WritableLike;
  stderr?: WritableLike;
};

type ParsedArgs = {
  tokens: string[];
  flags: Map<string, string[]>;
  booleans: Set<string>;
};

export type CliContext = {
  parsed: ParsedArgs;
  env: CliEnv;
  stdout: WritableLike;
  stderr: WritableLike;
  pretty: boolean;
};

export type ConnectionOptions = {
  path: string;
  driver?: ThingDDriver;
  authToken?: string;
  cloud: boolean;
};

const HELP_TEXT = `thingd

Admin and operator CLI for thingd.

Usage:
  thingd status [--url <url>]
  thingd tools --url <url>
  thingd install
  thingd mcp [--path <path>] [--driver <driver>]
  thingd mcp-http [--path <path>] [--driver <driver>] [--host <host>] [--port <port>] [--auth-token <tok>] [--allow-unauthenticated]
  thingd search <query> [--collection <name>] [--limit <n>]
  thingd objects get <collection> <id>
  thingd objects put <collection> <id> --text <text>
  thingd objects put <collection> <id> --data '{"field":"value"}'
  thingd objects delete <collection> <id>
  thingd events append <stream> <type> [--text <text>] [--data '{"field":"value"}']
  thingd events list [stream] [--limit <n>]
  thingd collections list
  thingd streams list
  thingd queues list-all
  thingd queues push <queue> --payload '{"key":"value"}'
  thingd queues claim <queue> [--lease-ms <ms>]
  thingd queues ack <queue> <jobId>
  thingd queues nack <queue> <jobId> [--error <message>] [--delay-ms <ms>]
  thingd queues list <queue> [--limit <n>]
  thingd queues dead <queue> [--limit <n>]
  thingd metrics

Options:
  --url <url>          remote thingd URL. Defaults to THINGD_URL
  --auth-token <tok>  remote bearer token. Defaults to THINGD_AUTH_TOKEN
  --path <path>       local database path. Defaults to THINGD_PATH or ~/.thingd/data.db
  --driver <driver>   memory, native, or cloud
  --pretty            pretty-print JSON output
  --limit <n>         result limit for search and list commands
  -h, --help          show help
`;

const BOOLEAN_FLAGS = new Set(["h", "help", "json", "pretty", "allow-unauthenticated"]);

export async function runCli(
  args = process.argv.slice(2),
  options: RunCliOptions = {},
): Promise<number> {
  const parsed = parseArgs(args);
  const context: CliContext = {
    parsed,
    env: options.env ?? process.env,
    stdout: options.stdout ?? process.stdout,
    stderr: options.stderr ?? process.stderr,
    pretty: hasFlag(parsed, "pretty"),
  };

  try {
    if (hasFlag(parsed, "help") || hasFlag(parsed, "h")) {
      writeText(context.stdout, HELP_TEXT);
      return 0;
    }

    if (parsed.tokens.length === 0) {
      await runInteractiveCli();
      return 0;
    }

    await runCommand(context);
    return 0;
  } catch (error) {
    writeJson(
      context.stderr,
      {
        error: error instanceof Error ? error.message : String(error),
      },
      context.pretty,
    );
    return 1;
  }
}

async function runCommand(context: CliContext): Promise<void> {
  const command = requiredToken(context.parsed, 0, "command");

  if (command === "status") {
    await runStatus(context);
    return;
  }

  if (command === "tools") {
    await runTools(context);
    return;
  }

  if (command === "search") {
    await runSearch(context);
    return;
  }

  if (command === "mcp") {
    await runMcp(context);
    return;
  }

  if (command === "mcp-http") {
    const { runMcpHttp } = await import("./mcp-http.js");
    await runMcpHttp(context);
    return;
  }

  if (command === "install") {
    const { runInstall } = await import("./install.js");
    await runInstall(context);
    return;
  }

  if (command === "objects") {
    await runObjects(context);
    return;
  }

  if (command === "events") {
    await runEvents(context);
    return;
  }

  if (command === "collections") {
    await runCollections(context);
    return;
  }

  if (command === "streams") {
    await runStreams(context);
    return;
  }

  if (command === "queues") {
    await runQueues(context);
    return;
  }

  if (command === "metrics") {
    await runMetrics(context);
    return;
  }

  throw new Error(`Unknown command: ${command}`);
}

async function runStatus(context: CliContext): Promise<void> {
  const connection = resolveConnection(context);

  if (!connection.cloud) {
    writeJson(
      context.stdout,
      {
        mode: "local",
        driver: connection.driver ?? "memory",
        path: connection.path,
      },
      context.pretty,
    );
    return;
  }

  const baseUrl = resolveCloudBaseUrl(connection.path);
  const [health, cluster] = await Promise.all([
    fetchJson(new URL("/healthz", baseUrl), connection.authToken),
    fetchJson(new URL("/cluster/status", baseUrl), connection.authToken),
  ]);

  writeJson(
    context.stdout,
    {
      mode: "cloud",
      url: resolveCloudMcpUrl(connection.path),
      health,
      cluster,
    },
    context.pretty,
  );
}

async function runTools(context: CliContext): Promise<void> {
  const connection = resolveConnection(context);

  if (!connection.cloud) {
    throw new Error(
      "tools requires --url or THINGD_URL because tools are exposed by the MCP runtime",
    );
  }

  const client = new Client({
    name: "thingd-cli",
    version: "0.1.0",
  });
  const transport = new StreamableHTTPClientTransport(
    new URL(resolveCloudMcpUrl(connection.path)),
    {
      requestInit: connection.authToken
        ? {
            headers: {
              Authorization: `Bearer ${connection.authToken}`,
            },
          }
        : undefined,
    },
  );

  try {
    await client.connect(transport);
    const result = await client.listTools();
    writeJson(
      context.stdout,
      {
        tools: result.tools.map((tool) => ({
          name: tool.name,
          description: tool.description,
        })),
      },
      context.pretty,
    );
  } finally {
    await client.close();
  }
}

async function runSearch(context: CliContext): Promise<void> {
  const query = context.parsed.tokens.slice(1).join(" ").trim();
  if (!query) {
    throw new Error("search requires a query");
  }

  await withDb(context, async (db) => {
    const options: MemorySearchOptions = {
      collections: stringFlags(context.parsed, "collection"),
      limit: optionalInt(context.parsed, "limit"),
    };
    const results = await db.search(query, compactOptions(options));
    writeJson(context.stdout, results, context.pretty);
  });
}

async function runObjects(context: CliContext): Promise<void> {
  const action = requiredToken(context.parsed, 1, "objects action");
  const collection = requiredToken(context.parsed, 2, "collection");
  const id = requiredToken(context.parsed, 3, "object id");

  await withDb(context, async (db) => {
    if (action === "get") {
      writeJson(context.stdout, await db.get(collection, id), context.pretty);
      return;
    }

    if (action === "put") {
      const object = buildMemoryObject(context.parsed, id);
      writeJson(context.stdout, await db.put(collection, object), context.pretty);
      return;
    }

    if (action === "delete") {
      writeJson(context.stdout, await db.delete(collection, id), context.pretty);
      return;
    }

    throw new Error(`Unknown objects action: ${action}`);
  });
}

async function runEvents(context: CliContext): Promise<void> {
  const action = requiredToken(context.parsed, 1, "events action");

  await withDb(context, async (db) => {
    if (action === "list") {
      const stream = optionalToken(context.parsed, 2);
      const events = await db.events.list(stream);
      writeJson(
        context.stdout,
        limitItems(events, optionalInt(context.parsed, "limit")),
        context.pretty,
      );
      return;
    }

    if (action === "append") {
      const stream = requiredToken(context.parsed, 2, "stream");
      const type = requiredToken(context.parsed, 3, "event type");
      const event = buildMemoryEvent(context.parsed, type);
      writeJson(context.stdout, await db.events.append(stream, event), context.pretty);
      return;
    }

    throw new Error(`Unknown events action: ${action}`);
  });
}

async function runCollections(context: CliContext): Promise<void> {
  const action = requiredToken(context.parsed, 1, "collections action");
  await withDb(context, async (db) => {
    if (action === "list") {
      writeJson(context.stdout, await db.listCollections(), context.pretty);
      return;
    }
    throw new Error(`Unknown collections action: ${action}`);
  });
}

async function runStreams(context: CliContext): Promise<void> {
  const action = requiredToken(context.parsed, 1, "streams action");
  await withDb(context, async (db) => {
    if (action === "list") {
      writeJson(context.stdout, await db.listStreams(), context.pretty);
      return;
    }
    throw new Error(`Unknown streams action: ${action}`);
  });
}

async function runMetrics(context: CliContext): Promise<void> {
  await withDb(context, async (db) => {
    const [objects, events, activeJobs, deadJobs] = await Promise.all([
      db.countObjects(),
      db.countEvents(),
      db.countActiveJobs(),
      db.countDeadJobs(),
    ]);
    writeJson(
      context.stdout,
      {
        objects,
        events,
        activeJobs,
        deadJobs,
      },
      context.pretty,
    );
  });
}

async function runQueues(context: CliContext): Promise<void> {
  const action = requiredToken(context.parsed, 1, "queues action");

  await withDb(context, async (db) => {
    if (action === "list-all") {
      writeJson(context.stdout, await db.listQueues(), context.pretty);
      return;
    }

    const queueName = requiredToken(context.parsed, 2, "queue");
    const queue = db.queue(queueName);

    if (action === "push") {
      const payload = parseJsonRecord(requiredFlag(context.parsed, "payload"));
      const options: QueueJobOptions = {
        idempotencyKey: stringFlag(context.parsed, "idempotency-key"),
        maxAttempts: optionalInt(context.parsed, "max-attempts"),
        delayMs: optionalInt(context.parsed, "delay-ms"),
      };
      writeJson(context.stdout, await queue.push(payload, compactOptions(options)), context.pretty);
      return;
    }

    if (action === "claim") {
      const options: QueueClaimOptions = {
        leaseMs: optionalInt(context.parsed, "lease-ms"),
      };
      writeJson(context.stdout, await queue.claim(compactOptions(options)), context.pretty);
      return;
    }

    if (action === "ack") {
      writeJson(
        context.stdout,
        await queue.ack(requiredToken(context.parsed, 3, "job id")),
        context.pretty,
      );
      return;
    }

    if (action === "nack") {
      const options: QueueNackOptions = {
        delayMs: optionalInt(context.parsed, "delay-ms"),
        error: stringFlag(context.parsed, "error"),
      };
      writeJson(
        context.stdout,
        await queue.nack(requiredToken(context.parsed, 3, "job id"), compactOptions(options)),
        context.pretty,
      );
      return;
    }

    if (action === "list") {
      writeJson(
        context.stdout,
        limitItems(await queue.list(), optionalInt(context.parsed, "limit")),
        context.pretty,
      );
      return;
    }

    if (action === "dead") {
      writeJson(
        context.stdout,
        limitItems(await queue.dead(), optionalInt(context.parsed, "limit")),
        context.pretty,
      );
      return;
    }

    throw new Error(`Unknown queues action: ${action}`);
  });
}

export async function withDb(
  context: CliContext,
  callback: (db: ThingD) => Promise<void>,
): Promise<void> {
  const connection = resolveConnection(context);
  const db = await ThingD.open({
    path: connection.path,
    url: connection.cloud ? connection.path : undefined,
    driver: connection.driver,
    authToken: connection.authToken,
  });

  try {
    await callback(db);
  } finally {
    await db.close();
  }
}

function buildMemoryObject(parsed: ParsedArgs, id: string): MemoryObject {
  const data = stringFlag(parsed, "data");
  const text = stringFlag(parsed, "text");

  if (data === undefined && text === undefined) {
    throw new Error("objects put requires --text or --data");
  }

  return {
    ...(data === undefined ? {} : parseJsonRecord(data)),
    id,
    ...(text === undefined ? {} : { text }),
  };
}

function buildMemoryEvent(parsed: ParsedArgs, type: string): MemoryEvent {
  const data = stringFlag(parsed, "data");
  const text = stringFlag(parsed, "text");

  return {
    ...(data === undefined ? {} : parseJsonRecord(data)),
    type,
    ...(text === undefined ? {} : { text }),
  };
}

export function resolveConnection(context: CliContext): ConnectionOptions {
  const url = stringFlag(context.parsed, "url") ?? context.env.THINGD_URL;
  const path =
    url ?? stringFlag(context.parsed, "path") ?? context.env.THINGD_PATH ?? defaultThingdDbPath();
  const cloud = isCloudPath(path);
  let driver = parseDriver(stringFlag(context.parsed, "driver") ?? context.env.THINGD_DRIVER);

  if (!driver) {
    if (cloud) {
      driver = "cloud";
    } else if (path !== ":memory:") {
      driver = "native";
    } else {
      driver = "memory";
    }
  }

  if (!cloud && path === defaultThingdDbPath()) {
    ensureThingdDir();
  }

  return {
    path,
    driver,
    authToken: stringFlag(context.parsed, "auth-token") ?? context.env.THINGD_AUTH_TOKEN,
    cloud,
  };
}

function parseArgs(args: string[]): ParsedArgs {
  const tokens: string[] = [];
  const flags = new Map<string, string[]>();
  const booleans = new Set<string>();

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];

    if (arg === undefined) {
      continue;
    }

    if (!arg.startsWith("-")) {
      tokens.push(arg);
      continue;
    }

    const name = arg.replace(/^-+/, "");
    const next = args[index + 1];

    if (BOOLEAN_FLAGS.has(name)) {
      booleans.add(name);
      continue;
    }

    if (next === undefined || next.startsWith("-")) {
      booleans.add(name);
      continue;
    }

    addFlagValue(flags, name, next);
    index += 1;
  }

  return {
    tokens,
    flags,
    booleans,
  };
}

function addFlagValue(flags: Map<string, string[]>, name: string, value: string): void {
  const values = flags.get(name) ?? [];
  values.push(value);
  flags.set(name, values);
}

function hasFlag(parsed: ParsedArgs, name: string): boolean {
  return parsed.booleans.has(name) || parsed.flags.has(name);
}

function stringFlag(parsed: ParsedArgs, name: string): string | undefined {
  return parsed.flags.get(name)?.at(-1);
}

function stringFlags(parsed: ParsedArgs, name: string): string[] | undefined {
  const values = parsed.flags.get(name);
  return values && values.length > 0 ? values : undefined;
}

function requiredFlag(parsed: ParsedArgs, name: string): string {
  const value = stringFlag(parsed, name);
  if (value === undefined) {
    throw new Error(`Missing required flag: --${name}`);
  }
  return value;
}

function optionalToken(parsed: ParsedArgs, index: number): string | undefined {
  return parsed.tokens[index];
}

function requiredToken(parsed: ParsedArgs, index: number, name: string): string {
  const value = optionalToken(parsed, index);
  if (!value) {
    throw new Error(`Missing required argument: ${name}`);
  }
  return value;
}

function optionalInt(parsed: ParsedArgs, name: string): number | undefined {
  const value = stringFlag(parsed, name);
  if (value === undefined) {
    return undefined;
  }

  const parsedValue = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsedValue) || parsedValue < 0) {
    throw new Error(`--${name} must be a non-negative integer`);
  }
  return parsedValue;
}

function parseDriver(value: string | undefined): ThingDDriver | undefined {
  if (value === undefined) {
    return undefined;
  }

  if (value === "memory" || value === "native" || value === "cloud") {
    return value;
  }

  throw new Error(`Unsupported driver: ${value}`);
}

function parseJsonRecord(value: string): Record<string, unknown> {
  const parsed = JSON.parse(value) as unknown;
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("Expected a JSON object");
  }
  return parsed as Record<string, unknown>;
}

function compactOptions<T extends Record<string, unknown>>(options: T): T {
  return Object.fromEntries(
    Object.entries(options).filter(([, value]) => value !== undefined),
  ) as T;
}

function limitItems<T>(items: T[], limit: number | undefined): T[] {
  return limit === undefined ? items : items.slice(0, limit);
}

function isCloudPath(path: string): boolean {
  return path.startsWith("http://") || path.startsWith("https://") || path.startsWith("thingd://");
}

function resolveCloudMcpUrl(value: string): string {
  const url = new URL(normalizeCloudUrl(value));

  if (url.pathname === "" || url.pathname === "/") {
    url.pathname = "/mcp";
  }

  return url.toString();
}

function resolveCloudBaseUrl(value: string): string {
  const url = new URL(normalizeCloudUrl(value));

  if (url.pathname === "/mcp") {
    url.pathname = "/";
  }

  return url.toString();
}

function normalizeCloudUrl(value: string): string {
  return value.startsWith("thingd://") ? `http://${value.slice("thingd://".length)}` : value;
}

async function fetchJson(url: URL, authToken: string | undefined): Promise<unknown> {
  const response = await fetch(url, {
    headers: authToken
      ? {
          Authorization: `Bearer ${authToken}`,
        }
      : undefined,
  });

  if (!response.ok) {
    throw new Error(`HTTP ${response.status} from ${url.toString()}`);
  }

  return response.json() as Promise<unknown>;
}

function writeJson(target: WritableLike, data: unknown, pretty: boolean): void {
  target.write(`${JSON.stringify(data, null, pretty ? 2 : 0)}\n`);
}

function writeText(target: WritableLike, text: string): void {
  target.write(text.endsWith("\n") ? text : `${text}\n`);
}

let isMain = false;
if (process.argv[1]) {
  try {
    isMain = import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href;
  } catch {
    // Ignore realpath resolution errors.
  }
}

if (isMain) {
  process.exitCode = await runCli();
}
