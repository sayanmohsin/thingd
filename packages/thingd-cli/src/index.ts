#!/usr/bin/env node

import { realpathSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import pc from "picocolors";
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

export type ParsedArgs = {
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

// ── Opencode-style log output ──────────────────────────────────────

function writeLog(
  target: WritableLike,
  data: { label: string; value: string }[],
  header?: string,
): void {
  const W = 60;
  if (header) {
    target.write(` ${pc.bold(header)}\n`);
    target.write(` ${pc.dim("─".repeat(W))}\n`);
  }
  for (const { label, value } of data) {
    target.write(` ${pc.cyan("●")} ${pc.dim(label.padEnd(14))} ${value}\n`);
  }
  target.write("\n");
}

function writeLogBullets(
  target: WritableLike,
  items: { icon?: string; text: string; indent?: number }[],
  header?: string,
): void {
  const W = 60;
  if (header) {
    target.write(` ${pc.bold(header)}\n`);
    target.write(` ${pc.dim("─".repeat(W))}\n`);
  }
  for (const item of items) {
    const icon = item.icon ?? pc.cyan("○");
    const indent = " ".repeat(item.indent ?? 1);
    target.write(`${indent}${icon} ${item.text}\n`);
  }
  target.write("\n");
}

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
  thingd install [--raw] [--claude] [--cursor] [--antigravity]
  thingd doctor
  thingd mcp [--path <path>] [--driver <driver>]
  thingd mcp-http [--path <path>] [--driver <driver>] [--host <host>] [--port <port>] [--auth-token <tok>] [--allow-unauthenticated]
  thingd search <query> [--collection <name>] [--limit <n>] [--filter <json>]
  thingd objects list <collection>
  thingd objects get <collection> <id>
  thingd objects put <collection> <id> --text <text>
  thingd objects put <collection> <id> --data '{"field":"value"}'
  thingd objects delete <collection> <id>
  thingd events streams
  thingd events append <stream> <type> [--text <text>] [--data '{"field":"value"}']
  thingd events list [stream] [--limit <n>]
  thingd collections list
  thingd streams list
  thingd queues list-all
  thingd queues stats <queue>
  thingd queues push <queue> --payload '{"key":"value"}'
  thingd queues claim <queue> [--lease-ms <ms>]
  thingd queues ack <queue> <jobId>
  thingd queues nack <queue> <jobId> [--error <message>] [--delay-ms <ms>]
  thingd queues list <queue> [--limit <n>]
  thingd queues dead <queue> [--limit <n>]
  thingd bench rust --smoke
  thingd bench rust --count <n>
  thingd metrics
  thingd dashboard [--port <port>] [--path <path>] [--driver <driver>]
  thingd export --collection <name> --out <path> [--redact [keys]]
  thingd export --events [--stream <name>] --out <path> [--redact [keys]]
  thingd import --collection <name> --in <path>
  thingd snapshot create --out <path>
  thingd snapshot restore --in <path>

Options:
  --url <url>          remote thingd URL. Defaults to THINGD_URL
  --auth-token <tok>  remote bearer token. Defaults to THINGD_AUTH_TOKEN
  --path <path>       local database path. Defaults to THINGD_PATH or ~/.thingd/data.db
  --driver <driver>   memory, native, or cloud
  --pretty            opencode-style log output (human-readable)
  --limit <n>         result limit for search and list commands
  --filter <json>     metadata key-value filter (e.g. '{"status":"active"}')
  -h, --help          show help
`;

const BOOLEAN_FLAGS = new Set([
  "h",
  "help",
  "json",
  "pretty",
  "allow-unauthenticated",
  "raw",
  "claude",
  "cursor",
  "antigravity",
  "smoke",
  "events",
]);

export async function runCli(
  args = process.argv.slice(2),
  options: RunCliOptions = {},
): Promise<number> {
  // Auto-detect native binary for global/local dev execution
  if (!process.env.THINGD_NATIVE_PATH) {
    try {
      const { existsSync } = await import("node:fs");
      const { join } = await import("node:path");

      const cliDir = join(resolveCliPath(), "..", "..");
      const platform = process.platform;
      const arch = process.arch;
      const candidates = [
        // installed via pnpm/npm as transitive dependency of thingd
        join(cliDir, "node_modules", "thingd-native", "dist", "thingd_native.node"),
        join(
          cliDir,
          "node_modules",
          "thingd-native",
          "prebuilds",
          `${platform}-${arch}`,
          "thingd_native.node",
        ),
        // workspace sibling (local dev)
        join(cliDir, "..", "thingd-native", "dist", "thingd_native.node"),
        join(
          cliDir,
          "..",
          "thingd-native",
          "prebuilds",
          `${platform}-${arch}`,
          "thingd_native.node",
        ),
      ];
      for (const candidate of candidates) {
        if (existsSync(candidate)) {
          process.env.THINGD_NATIVE_PATH = candidate;
          break;
        }
      }
    } catch {
      // Ignore detection errors
    }
  }

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

  if (command === "doctor") {
    const { runDoctor } = await import("./doctor.js");
    await runDoctor(context);
    return;
  }

  if (command === "bench") {
    await runBench(context);
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

  if (command === "dashboard") {
    await runDashboard(context);
    return;
  }

  if (command === "export") {
    const { runExport } = await import("./data-movement.js");
    await runExport(context);
    return;
  }

  if (command === "import") {
    const { runImport } = await import("./data-movement.js");
    await runImport(context);
    return;
  }

  if (command === "snapshot") {
    const { runSnapshot } = await import("./data-movement.js");
    await runSnapshot(context);
    return;
  }

  throw new Error(`Unknown command: ${command}`);
}

async function runBench(context: CliContext): Promise<void> {
  const target = requiredToken(context.parsed, 1, "benchmark target (rust)");
  if (target !== "rust") {
    throw new Error(`Unsupported benchmark target: ${target}`);
  }

  const isSmoke = hasFlag(context.parsed, "smoke");
  const countStr = stringFlag(context.parsed, "count");
  const count = countStr ? Number.parseInt(countStr, 10) : isSmoke ? 100 : undefined;

  if (count === undefined) {
    throw new Error("bench rust requires --smoke or --count <n>");
  }

  if (Number.isNaN(count) || count <= 0) {
    throw new Error("--count must be a positive integer");
  }

  try {
    const { execSync } = await import("node:child_process");
    try {
      execSync("cargo --version", { stdio: "ignore" });
    } catch {
      throw new Error(
        "Rust toolchain (cargo) is not installed or not in the PATH. Cannot run Rust benchmarks.",
      );
    }

    context.stderr.write(
      `\n${pc.bold("Running Rust storage benchmark")} (Count: ${pc.cyan(count)})...\n\n`,
    );

    const { spawn } = await import("node:child_process");
    const child = spawn(
      "cargo",
      [
        "run",
        "--release",
        "-p",
        "thingd-core",
        "--example",
        "storage_bench",
        "--features",
        "sqlite",
        "--",
        String(count),
      ],
      {
        stdio: "inherit",
        cwd: resolve(resolveCliPath(), "../../../.."),
      },
    );

    return new Promise((resolvePromise, rejectPromise) => {
      child.on("close", (code) => {
        if (code === 0) {
          resolvePromise();
        } else {
          rejectPromise(new Error(`Benchmark failed with exit code: ${code}`));
        }
      });
      child.on("error", (error) => {
        rejectPromise(error);
      });
    });
  } catch (err) {
    throw new Error(`Failed to run benchmark: ${err instanceof Error ? err.message : String(err)}`);
  }
}

async function runStatus(context: CliContext): Promise<void> {
  const connection = resolveConnection(context);

  if (!connection.cloud) {
    if (context.pretty) {
      writeLog(
        context.stdout,
        [
          { label: "Driver", value: pc.cyan(connection.driver ?? "memory") },
          { label: "Path", value: pc.dim(connection.path) },
        ],
        "thingd  status",
      );
      return;
    }
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

  const castCluster = cluster as
    | {
        leaderUrl?: string;
        replication?: { lastReplicatedSequence?: number; lag?: number };
      }
    | null
    | undefined;
  const replication = castCluster?.replication;
  const lastReplicatedSequence =
    replication && typeof replication === "object" && "lastReplicatedSequence" in replication
      ? replication.lastReplicatedSequence
      : undefined;
  const replicationLag =
    replication && typeof replication === "object" && "lag" in replication
      ? replication.lag
      : undefined;

  if (context.pretty) {
    const items: { label: string; value: string }[] = [
      { label: "Mode", value: pc.cyan("cloud") },
      { label: "URL", value: pc.dim(resolveCloudMcpUrl(connection.path)) },
    ];
    if (lastReplicatedSequence !== undefined) {
      items.push({ label: "Last Seq", value: String(lastReplicatedSequence) });
    }
    if (replicationLag !== undefined) {
      items.push({ label: "Repl Lag", value: `${replicationLag}ms` });
    }
    writeLog(context.stdout, items, "thingd  status");
    return;
  }

  writeJson(
    context.stdout,
    {
      mode: "cloud",
      url: resolveCloudMcpUrl(connection.path),
      health,
      cluster,
      leaderUrl: castCluster?.leaderUrl,
      lastReplicatedSequence,
      replicationLag,
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
    const filterStr = stringFlag(context.parsed, "filter");
    const options: MemorySearchOptions = {
      collections: stringFlags(context.parsed, "collection"),
      limit: optionalInt(context.parsed, "limit"),
      filter: filterStr ? JSON.parse(filterStr) : undefined,
    };
    const results = await db.search(query, compactOptions(options));
    if (context.pretty) {
      const bullets = results.map((r) => {
        const res = r as {
          id: string;
          kind: string;
          collection?: string;
          stream?: string;
          value?: { text?: string };
        };
        const col = res.kind === "object" ? (res.collection ?? "") : (res.stream ?? "");
        return `${pc.green(res.id)} ${pc.dim(col)}`;
      });
      writeLogBullets(
        context.stdout,
        bullets.map((t) => ({ text: t })),
        `thingd  search  ${query}`,
      );
      return;
    }
    writeJson(context.stdout, results, context.pretty);
  });
}

async function runObjects(context: CliContext): Promise<void> {
  const action = requiredToken(context.parsed, 1, "objects action");
  const collection = requiredToken(context.parsed, 2, "collection");

  await withDb(context, async (db) => {
    if (action === "list") {
      const objects = await db.listObjects(collection);
      if (context.pretty) {
        const bullets = objects.map((obj) => {
          const { id, version, createdAt } = obj;
          const meta = `${pc.dim(`v${version}`)}  ${createdAt ? pc.dim(new Date(createdAt).toLocaleString()) : ""}`;
          return `${pc.green(id)}  ${meta}`;
        });
        writeLogBullets(
          context.stdout,
          bullets.map((t) => ({ text: t })),
          `thingd  objects  list  ${collection}`,
        );
      } else {
        writeJson(context.stdout, objects, false);
      }
      return;
    }

    const id = requiredToken(context.parsed, 3, "object id");

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
    if (action === "streams") {
      const streams = await db.listStreams();
      if (context.pretty) {
        writeLogBullets(
          context.stdout,
          streams.map((s) => ({ text: pc.green(s), icon: pc.green("●") })),
          "thingd  events  streams",
        );
      } else {
        writeJson(context.stdout, streams, false);
      }
      return;
    }

    if (action === "list") {
      const stream = optionalToken(context.parsed, 2);
      const events = limitItems(await db.events.list(stream), optionalInt(context.parsed, "limit"));
      if (context.pretty) {
        const bullets = events.map((ev) => {
          const { id, type, createdAt, ...data } = ev;
          const ts = createdAt ? pc.dim(new Date(createdAt).toLocaleString()) : "";
          const dataStr = Object.keys(data).length > 0 ? ` ${pc.dim(JSON.stringify(data))}` : "";
          return `${pc.green(id)} ${pc.magenta(type)} ${ts}${dataStr}`;
        });
        writeLogBullets(
          context.stdout,
          bullets.map((t) => ({ text: t, icon: pc.green("●") })),
          stream ? `thingd  events  list  ${stream}` : "thingd  events  list",
        );
      } else {
        writeJson(context.stdout, events, false);
      }
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
      const collections = await db.listCollections();
      if (context.pretty) {
        writeLogBullets(
          context.stdout,
          collections.map((c) => ({ text: pc.cyan(c) })),
          "thingd  collections  list",
        );
      } else {
        writeJson(context.stdout, collections, false);
      }
      return;
    }
    throw new Error(`Unknown collections action: ${action}`);
  });
}

async function runStreams(context: CliContext): Promise<void> {
  const action = requiredToken(context.parsed, 1, "streams action");
  await withDb(context, async (db) => {
    if (action === "list") {
      const streams = await db.listStreams();
      if (context.pretty) {
        writeLogBullets(
          context.stdout,
          streams.map((s) => ({ text: pc.green(s), icon: pc.green("●") })),
          "thingd  streams  list",
        );
      } else {
        writeJson(context.stdout, streams, false);
      }
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
    if (context.pretty) {
      writeLog(
        context.stdout,
        [
          { label: "Objects", value: pc.cyan(String(objects)) },
          { label: "Events", value: pc.green(String(events)) },
          { label: "Active Jobs", value: pc.yellow(String(activeJobs)) },
          { label: "Dead Jobs", value: pc.red(String(deadJobs)) },
        ],
        "thingd  metrics",
      );
      return;
    }
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
      const queues = await db.listQueues();
      if (context.pretty) {
        writeLogBullets(
          context.stdout,
          queues.map((q) => ({ text: pc.magenta(q), icon: pc.magenta("◇") })),
          "thingd  queues  list-all",
        );
      } else {
        writeJson(context.stdout, queues, false);
      }
      return;
    }

    const queueName = requiredToken(context.parsed, 2, "queue");
    const queue = db.queue(queueName);

    if (action === "stats") {
      const [activeJobs, deadJobs] = await Promise.all([queue.list(), queue.dead()]);

      const totalActive = activeJobs.length;
      const totalDead = deadJobs.length;
      const leasedJobs = activeJobs.filter((job) => job.status === "leased");
      const readyJobs = activeJobs.filter((job) => job.status === "ready");

      if (context.pretty) {
        writeLog(
          context.stdout,
          [
            { label: "Queue", value: pc.magenta(queueName) },
            { label: "Ready", value: pc.cyan(String(readyJobs.length)) },
            { label: "Leased", value: pc.yellow(String(leasedJobs.length)) },
            { label: "Dead", value: pc.red(String(totalDead)) },
            { label: "Total Active", value: String(totalActive) },
          ],
          "thingd  queues  stats",
        );
        return;
      }

      writeJson(
        context.stdout,
        {
          queue: queueName,
          totalActive,
          ready: readyJobs.length,
          leased: leasedJobs.length,
          dead: totalDead,
        },
        false,
      );
      return;
    }

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
      const jobs = limitItems(await queue.list(), optionalInt(context.parsed, "limit"));
      if (context.pretty) {
        const bullets = jobs.map((job) => {
          const statusColor = job.status === "leased" ? pc.yellow : pc.cyan;
          return `${pc.dim(job.id)} ${statusColor(job.status)} ${pc.dim(`${job.attempts}/${job.maxAttempts}`)}`;
        });
        writeLogBullets(
          context.stdout,
          bullets.map((t) => ({ text: t, icon: pc.cyan("●") })),
          `thingd  queues  list  ${queueName}`,
        );
      } else {
        writeJson(context.stdout, jobs, false);
      }
      return;
    }

    if (action === "dead") {
      const jobs = limitItems(await queue.dead(), optionalInt(context.parsed, "limit"));
      if (context.pretty) {
        const bullets = jobs.map((job) => {
          const err = job.lastError ? ` ${pc.dim(job.lastError)}` : "";
          return `${pc.dim(job.id)} ${pc.dim(`${job.attempts}/${job.maxAttempts}`)}${err}`;
        });
        writeLogBullets(
          context.stdout,
          bullets.map((t) => ({ text: t, icon: pc.red("○") })),
          `thingd  queues  dead  ${queueName}`,
        );
      } else {
        writeJson(context.stdout, jobs, false);
      }
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

export function hasFlag(parsed: ParsedArgs, name: string): boolean {
  return parsed.booleans.has(name) || parsed.flags.has(name);
}

export function stringFlag(parsed: ParsedArgs, name: string): string | undefined {
  return parsed.flags.get(name)?.at(-1);
}

export function stringFlags(parsed: ParsedArgs, name: string): string[] | undefined {
  const values = parsed.flags.get(name);
  return values && values.length > 0 ? values : undefined;
}

export function requiredFlag(parsed: ParsedArgs, name: string): string {
  const value = stringFlag(parsed, name);
  if (value === undefined) {
    throw new Error(`Missing required flag: --${name}`);
  }
  return value;
}

export function optionalToken(parsed: ParsedArgs, index: number): string | undefined {
  return parsed.tokens[index];
}

export function requiredToken(parsed: ParsedArgs, index: number, name: string): string {
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

export function writeJson(target: WritableLike, data: unknown, pretty: boolean): void {
  target.write(`${JSON.stringify(data, null, pretty ? 2 : 0)}\n`);
}

export function writeText(target: WritableLike, text: string): void {
  target.write(text.endsWith("\n") ? text : `${text}\n`);
}

function resolveCliPath(): string {
  const scriptPath = process.argv[1];
  if (!scriptPath) {
    throw new Error("Could not detect thingd CLI path from process.argv[1].");
  }
  try {
    return realpathSync(resolve(scriptPath));
  } catch {
    return resolve(scriptPath);
  }
}

async function runDashboard(context: CliContext): Promise<void> {
  const portStr = stringFlag(context.parsed, "port");
  const port = portStr ? Number.parseInt(portStr, 10) : 8758;

  if (Number.isNaN(port) || port <= 0) {
    throw new Error("--port must be a positive integer");
  }

  const connection = resolveConnection(context);
  context.stderr.write(
    `\n${pc.bold(pc.blue("thingd Inspector Dashboard"))}\n` +
      `Starting local REST server on ${pc.cyan(`http://localhost:${port}`)}...\n` +
      `Database path: ${pc.green(connection.path)}\n` +
      `Storage engine: ${pc.cyan(connection.driver || "memory")}\n\n`,
  );

  const { startDashboardServer } = await import("./dashboard/server.js");
  const { server: _server, close } = await startDashboardServer(connection, port);

  context.stderr.write(
    `${pc.green("✔ Dashboard successfully loaded.")}\n` +
      `Opening browser... (Press ${pc.yellow("Ctrl+C")} to stop the server)\n\n`,
  );

  await openBrowser(`http://localhost:${port}`);

  return new Promise<void>((_resolve) => {
    process.on("SIGINT", async () => {
      await close();
      process.exit(0);
    });
    process.on("SIGTERM", async () => {
      await close();
      process.exit(0);
    });
  });
}

async function openBrowser(url: string): Promise<void> {
  const { exec } = await import("node:child_process");
  const startCommand =
    process.platform === "darwin" ? "open" : process.platform === "win32" ? "start" : "xdg-open";
  exec(`${startCommand} ${url}`, () => {
    // Ignore browser spawn errors silently
  });
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
  runCli()
    .then((code) => {
      if (code !== 0) {
        process.exit(code);
      }
    })
    .catch((error) => {
      console.error(error);
      process.exit(1);
    });
}
