import pc from "picocolors";
import type { CliContext } from "../index.js";
import { requiredFlag, stringFlag, writeJson } from "../index.js";
import {
  readSyncConfig,
  removeSyncConfig,
  type SyncConfig,
  writeSyncConfig,
} from "../lib/sync-config.js";

type ChangePage = {
  sourceId: string;
  next: number;
  changes: unknown[];
};

function endpoint(url: string, path: string): string {
  return `${url.replace(/\/$/, "")}${path}`;
}

async function request<T>(url: string, token: string | undefined, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  headers.set("Accept", "application/json");
  if (init?.body !== undefined) {
    headers.set("Content-Type", "application/json");
  }
  if (token) {
    headers.set("Authorization", `Bearer ${token}`);
  }
  const response = await fetch(url, { ...init, headers });
  const body = (await response.json()) as T & { data?: T; error?: unknown };
  if (!response.ok) {
    throw new Error(`${response.status}: ${JSON.stringify(body)}`);
  }
  return body.data ?? body;
}

function requireConfig(): SyncConfig {
  const config = readSyncConfig();
  if (!config) {
    throw new Error("No sync configuration. Run `thingd sync configure` first.");
  }
  return config;
}

function sourceAndTarget(config: SyncConfig): {
  sourceUrl: string;
  sourceToken?: string;
  targetUrl: string;
  targetToken?: string;
} {
  if (config.role === "source") {
    return {
      sourceUrl: config.localUrl,
      sourceToken: config.localToken,
      targetUrl: config.remoteUrl,
      targetToken: config.remoteToken,
    };
  }
  return {
    sourceUrl: config.remoteUrl,
    sourceToken: config.remoteToken,
    targetUrl: config.localUrl,
    targetToken: config.localToken,
  };
}

async function runSync(config: SyncConfig): Promise<SyncConfig> {
  if (config.paused) {
    return config;
  }
  const { sourceUrl, sourceToken, targetUrl, targetToken } = sourceAndTarget(config);
  const page = await request<ChangePage>(
    endpoint(sourceUrl, `/v1/replication/events?after=${config.cursor}&limit=500`),
    sourceToken
  );
  if (page.changes.length > 0) {
    await request(endpoint(targetUrl, "/v1/replication/apply"), targetToken, {
      method: "POST",
      body: JSON.stringify({ changes: page.changes }),
    });
    return { ...config, cursor: page.next };
  }
  return config;
}

export async function runSyncCommand(context: CliContext): Promise<void> {
  const subcommand = context.parsed.tokens[1] ?? "status";
  if (subcommand === "configure") {
    const config: SyncConfig = {
      localUrl: requiredFlag(context.parsed, "local-url"),
      remoteUrl: requiredFlag(context.parsed, "remote-url"),
      localToken: stringFlag(context.parsed, "local-token"),
      remoteToken: stringFlag(context.parsed, "remote-token"),
      role: stringFlag(context.parsed, "role") === "replica" ? "replica" : "source",
      cursor: 0,
      paused: false,
    };
    writeSyncConfig(config);
    writeJson(context.stdout, { configured: true, role: config.role }, context.pretty);
    return;
  }

  if (subcommand === "status") {
    writeJson(context.stdout, readSyncConfig() ?? { configured: false }, context.pretty);
    return;
  }

  if (subcommand === "pause" || subcommand === "resume") {
    const config = requireConfig();
    writeSyncConfig({ ...config, paused: subcommand === "pause" });
    writeJson(context.stdout, { paused: subcommand === "pause" }, context.pretty);
    return;
  }

  if (subcommand === "reset") {
    const config = requireConfig();
    removeSyncConfig();
    writeJson(context.stdout, { reset: true, previousCursor: config.cursor }, context.pretty);
    return;
  }

  if (subcommand === "push" || subcommand === "pull") {
    const config = requireConfig();
    if (config.role === "source" && subcommand === "pull") {
      throw new Error(
        "A source instance pushes changes; configure the replica and run `sync pull` there."
      );
    }
    if (config.role === "replica" && subcommand === "push") {
      throw new Error("A replica pulls changes; configure the source and run `sync push` there.");
    }
    const next = await runSync(config);
    writeSyncConfig(next);
    writeJson(
      context.stdout,
      { synced: next.cursor !== config.cursor, cursor: next.cursor, role: next.role },
      context.pretty
    );
    return;
  }

  throw new Error(
    `${pc.red(`Unknown sync command: ${subcommand}`)}. Use configure, status, push, pull, pause, resume, or reset.`
  );
}
