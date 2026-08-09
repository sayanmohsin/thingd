import pc from "picocolors";
import type { CliContext } from "../index.js";
import { hasFlag, requiredFlag, stringFlag, writeJson } from "../index.js";
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

type Snapshot = {
  sourceId: string;
  cursor: number;
  objects: unknown[];
  events: unknown[];
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

function syncHeaders(config: SyncConfig): HeadersInit {
  const headers: Record<string, string> = {};
  if (config.provider) {
    headers["X-Thingd-Provider"] = config.provider;
  }
  if (config.projectId) {
    headers["X-Thingd-Project-Id"] = config.projectId;
  }
  if (config.instanceSlug) {
    headers["X-Thingd-Instance-Slug"] = config.instanceSlug;
  }
  if (config.sourceId) {
    headers["X-Thingd-Source-Id"] = config.sourceId;
  }
  if (config.allowCloudTarget && config.targetConfirmed) {
    headers["X-Thingd-Allow-Cloud-Target"] = "true";
  }
  return headers;
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
    sourceToken,
    { headers: syncHeaders(config) }
  );
  if (page.changes.length > 0) {
    await request(endpoint(targetUrl, "/v1/replication/apply"), targetToken, {
      method: "POST",
      body: JSON.stringify({ changes: page.changes }),
      headers: syncHeaders(config),
    });
    return { ...config, cursor: page.next };
  }
  return config;
}

async function runBootstrap(config: SyncConfig, replace: boolean): Promise<SyncConfig> {
  if (!config.role || config.role !== "replica") {
    throw new Error("Bootstrap must run from a configured replica.");
  }
  const { sourceUrl, sourceToken, targetUrl, targetToken } = sourceAndTarget(config);
  const snapshot = await request<Snapshot>(
    endpoint(sourceUrl, "/v1/replication/snapshot"),
    sourceToken,
    {
      headers: syncHeaders(config),
    }
  );
  const effective = { ...config, sourceId: snapshot.sourceId };
  await request(endpoint(targetUrl, "/v1/replication/snapshot"), targetToken, {
    method: "POST",
    headers: syncHeaders(effective),
    body: JSON.stringify({ sourceId: snapshot.sourceId, snapshot, replace }),
  });
  return { ...effective, cursor: snapshot.cursor };
}

export async function runSyncCommand(context: CliContext): Promise<void> {
  const subcommand = context.parsed.tokens[1] ?? "status";
  if (subcommand === "configure") {
    const role = stringFlag(context.parsed, "role");
    if (role !== "source" && role !== "replica") {
      throw new Error(
        "Sync role is required. Choose --role source or --role replica; cloud authority is never inferred."
      );
    }
    const provider = stringFlag(context.parsed, "provider") ?? "self-hosted";
    const allowCloudTarget = hasFlag(context.parsed, "allow-cloud-target");
    const targetConfirmed = hasFlag(context.parsed, "confirm-target");
    if (provider === "thingd.cloud" && role === "source" && !allowCloudTarget) {
      throw new Error(
        "Cloud targets are protected by default. Use --allow-cloud-target --confirm-target only for an explicitly authorized cloud replica."
      );
    }
    const config: SyncConfig = {
      localUrl: requiredFlag(context.parsed, "local-url"),
      remoteUrl: requiredFlag(context.parsed, "remote-url"),
      localToken: stringFlag(context.parsed, "local-token"),
      remoteToken: stringFlag(context.parsed, "remote-token"),
      role,
      cursor: 0,
      paused: false,
      provider,
      projectId: stringFlag(context.parsed, "project-id"),
      instanceSlug: stringFlag(context.parsed, "instance-slug"),
      sourceId: stringFlag(context.parsed, "source-id"),
      allowCloudTarget,
      targetConfirmed,
    };
    writeSyncConfig(config);
    writeJson(context.stdout, { configured: true, role: config.role, provider }, context.pretty);
    return;
  }

  if (subcommand === "status") {
    const config = readSyncConfig();
    if (!config) {
      writeJson(context.stdout, { configured: false }, context.pretty);
      return;
    }
    if (!config.role) {
      writeJson(
        context.stdout,
        {
          configured: true,
          needsReconfigure: true,
          message: "Sync role is missing; re-run configure with --role.",
        },
        context.pretty
      );
      return;
    }
    const normalized = config;
    const { sourceUrl, sourceToken, targetUrl, targetToken } = sourceAndTarget(normalized);
    const [source, target] = await Promise.all([
      request(endpoint(sourceUrl, "/v1/replication/status"), sourceToken, {
        headers: syncHeaders(normalized),
      }),
      request(endpoint(targetUrl, "/v1/replication/status"), targetToken, {
        headers: syncHeaders(normalized),
      }),
    ]);
    writeJson(
      context.stdout,
      { configured: true, config: normalized, source, target },
      context.pretty
    );
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
    if (!config.role) {
      throw new Error("Sync configuration has no role. Re-run `thingd sync configure --role ...`.");
    }
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

  if (subcommand === "bootstrap") {
    const config = requireConfig();
    const next = await runBootstrap(config, hasFlag(context.parsed, "replace"));
    writeSyncConfig(next);
    writeJson(
      context.stdout,
      {
        bootstrapped: true,
        cursor: next.cursor,
        sourceId: next.sourceId,
        replaced: hasFlag(context.parsed, "replace"),
      },
      context.pretty
    );
    return;
  }

  throw new Error(
    `${pc.red(`Unknown sync command: ${subcommand}`)}. Use configure, status, push, pull, bootstrap, pause, resume, or reset.`
  );
}
