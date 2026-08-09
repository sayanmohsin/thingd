import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { PassThrough } from "node:stream";
import { StreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/streamableHttp.js";
import {
  createThingdMcpServer,
  type MemoryEvent,
  type MemoryObject,
  ThingD,
  type ThingdMcpAuditOptions,
  type ThingdMcpHardeningOptions,
} from "@thingd/sdk";
import {
  findNextLeaderCandidate,
  forwardMcpRequestToLeader,
  getClusterStatus,
  type ResolvedThingdClusterOptions,
  resolveClusterOptions,
  type ThingdClusterOptions,
} from "./cluster.js";
import { ensureHttpRuntimeIsSafe, type ThingDStorageDriver } from "./config.js";

export type ThingdHttpServerOptions = {
  path: string;
  driver?: ThingDStorageDriver;
  host?: string;
  port?: number;
  authToken?: string;
  allowUnauthenticated?: boolean;
  audit?: ThingdMcpAuditOptions | false;
  cluster?: ThingdClusterOptions;
  mcpPath?: string;
  healthPath?: string;
  hardening?: ThingdMcpHardeningOptions;
  /** Public Thingd-to-Thingd replication role. */
  syncRole?: "source" | "replica";
  /** Stable identifier used by the replication feed. */
  syncSourceId?: string;
  /** Optional allowlist of object collections replicated by this HTTP runtime. */
  syncCollections?: string[];
  syncProvider?: string;
  syncProjectId?: string;
  syncInstanceSlug?: string;
  allowCloudTarget?: boolean;
};

export type RunningThingdHttpServer = {
  server: Server;
  url: string;
  mcpUrl: string;
  close(): Promise<void>;
};

type RuntimeState = {
  db: ThingD;
  /** The underlying database instance (not wrapped in replicating proxy). */
  originalDb: ThingD;
  authToken?: string;
  mcpPath: string;
  healthPath: string;
  driver: ThingDStorageDriver | "memory";
  audit?: ThingdMcpAuditOptions | false;
  cluster: ResolvedThingdClusterOptions;
  hardening?: ThingdMcpHardeningOptions;
  replicationTimer?: NodeJS.Timeout;
  replicationStopped?: boolean;
  replicationAbort?: AbortController;
  consecutiveReplicationFailures: number;
  syncRole: "source" | "replica";
  syncSourceId: string;
  syncCollections: string[];
  syncProvider: string;
  syncProjectId?: string;
  syncInstanceSlug?: string;
  allowCloudTarget: boolean;
};

export async function startThingdHttpServer(
  options: ThingdHttpServerOptions
): Promise<RunningThingdHttpServer> {
  const host = options.host ?? "127.0.0.1";
  const port = options.port ?? 8757;
  ensureHttpRuntimeIsSafe({
    host,
    authToken: options.authToken,
    allowUnauthenticated: options.allowUnauthenticated,
  });

  const originalDb = await ThingD.open({
    path: options.path,
    driver: options.driver,
    syncRole: options.syncRole,
  });
  const cluster = resolveClusterOptions(options.cluster);
  const db = createReplicatingDb(originalDb, cluster.mode);

  const state: RuntimeState = {
    db,
    originalDb,
    authToken: options.authToken,
    mcpPath: options.mcpPath ?? "/mcp",
    healthPath: options.healthPath ?? "/healthz",
    driver: options.driver ?? "memory",
    audit: options.audit,
    cluster,
    hardening: options.hardening,
    consecutiveReplicationFailures: 0,
    syncRole: options.syncRole ?? "source",
    syncSourceId: options.syncSourceId ?? "thingd-http",
    syncCollections: options.syncCollections ?? [],
    syncProvider: options.syncProvider ?? "self-hosted",
    syncProjectId: options.syncProjectId,
    syncInstanceSlug: options.syncInstanceSlug,
    allowCloudTarget: options.allowCloudTarget ?? false,
  };
  const server = createServer((request, response) => {
    void handleRequest(state, request, response);
  });

  await listen(server, port, host);

  const address = server.address();
  const resolvedPort = typeof address === "object" && address ? address.port : port;
  const displayHost = host === "0.0.0.0" ? "127.0.0.1" : host;
  const url = `http://${displayHost}:${resolvedPort}`;

  if (cluster.mode === "follower") {
    startReplicationRunner(state);
  }

  return {
    server,
    url,
    mcpUrl: `${url}${state.mcpPath}`,
    close: async () => {
      stopReplicationRunner(state);
      await close(server);
      await originalDb.close?.();
    },
  };
}

async function handleRequest(
  state: RuntimeState,
  request: IncomingMessage,
  response: ServerResponse
): Promise<void> {
  try {
    setCommonHeaders(response);

    const path = requestPath(request);

    if (request.method === "OPTIONS") {
      response.writeHead(204);
      response.end();
      return;
    }

    if (path === state.healthPath) {
      await handleHealth(state, request, response);
      return;
    }

    if (path === state.cluster.statusPath) {
      await handleClusterStatus(state, request, response);
      return;
    }

    if (path === state.cluster.peersPath) {
      handleClusterPeers(state, request, response);
      return;
    }

    if (
      path !== state.mcpPath &&
      path !== "/v1/replication/events" &&
      path !== "/v1/replication/apply" &&
      path !== "/v1/replication/status" &&
      path !== "/v1/replication/conflicts" &&
      path !== "/v1/replication/snapshot"
    ) {
      writeJson(response, 404, {
        error: "not_found",
      });
      return;
    }

    if (!isAuthorized(state, request)) {
      response.setHeader("WWW-Authenticate", "Bearer");
      writeJson(response, 401, {
        error: "unauthorized",
      });
      return;
    }

    if (path === "/v1/replication/events") {
      await handleReplicationEvents(state, request, response);
      return;
    }

    if (path === "/v1/replication/apply") {
      await handleReplicationApply(state, request, response);
      return;
    }

    if (path === "/v1/replication/status") {
      await handleReplicationStatus(state, request, response);
      return;
    }

    if (path === "/v1/replication/conflicts") {
      await handleReplicationConflicts(state, request, response);
      return;
    }

    if (path === "/v1/replication/snapshot") {
      await handleReplicationSnapshot(state, request, response);
      return;
    }

    if (request.method !== "POST") {
      response.setHeader("Allow", "POST, OPTIONS");
      writeJson(response, 405, {
        jsonrpc: "2.0",
        error: {
          code: -32_000,
          message: "Method not allowed.",
        },
        id: null,
      });
      return;
    }

    // Enforce payload size limit.
    // For requests with Content-Length we can reject immediately without draining the body.
    // For chunked transfers we wrap the request in a PassThrough that aborts if the limit is exceeded.
    const maxBytes = state.hardening?.maxPayloadBytes ?? 524_288;
    const contentLength = parseContentLength(request);

    if (contentLength !== null && contentLength > maxBytes) {
      writeJson(response, 413, {
        jsonrpc: "2.0",
        error: {
          code: -32_000,
          message: `Request body exceeds the maximum allowed size of ${maxBytes} bytes.`,
        },
        id: null,
      });
      return;
    }

    if (state.cluster.mode === "follower") {
      await forwardMcpRequestToLeader(state.cluster, state.mcpPath, request, response);
      return;
    }

    // For chunked transfers, wrap the request in a PassThrough that enforces the limit
    // without pre-draining the stream (the MCP transport still drives reading).
    const wrappedRequest =
      contentLength === null ? wrapRequestWithSizeLimit(request, response, maxBytes) : request;

    await handleMcpRequest(state, wrappedRequest, response);
  } catch (error) {
    console.error("MCP request error:", error);
    if (!response.headersSent) {
      writeJson(response, 500, {
        jsonrpc: "2.0",
        error: {
          code: -32_603,
          message: "Internal server error",
        },
        id: null,
      });
      return;
    }

    response.end();
  }
}

async function handleHealth(
  state: RuntimeState,
  request: IncomingMessage,
  response: ServerResponse
): Promise<void> {
  if (request.method !== "GET" && request.method !== "HEAD") {
    response.setHeader("Allow", "GET, HEAD, OPTIONS");
    writeJson(response, 405, {
      error: "method_not_allowed",
    });
    return;
  }

  const status = await getClusterStatus(
    state.cluster,
    state.db,
    state.consecutiveReplicationFailures
  );
  writeJson(
    response,
    200,
    {
      ok: true,
      service: "thingd-mcp",
      driver: state.driver,
      mcpPath: state.mcpPath,
      cluster: status,
    },
    request.method === "HEAD"
  );
}

async function handleClusterStatus(
  state: RuntimeState,
  request: IncomingMessage,
  response: ServerResponse
): Promise<void> {
  if (request.method !== "GET" && request.method !== "HEAD") {
    response.setHeader("Allow", "GET, HEAD, OPTIONS");
    writeJson(response, 405, {
      error: "method_not_allowed",
    });
    return;
  }

  const status = await getClusterStatus(
    state.cluster,
    state.db,
    state.consecutiveReplicationFailures
  );
  writeJson(response, 200, status, request.method === "HEAD");
}

function handleClusterPeers(
  state: RuntimeState,
  request: IncomingMessage,
  response: ServerResponse
): void {
  if (request.method !== "GET" && request.method !== "HEAD") {
    response.setHeader("Allow", "GET, HEAD, OPTIONS");
    writeJson(response, 405, {
      error: "method_not_allowed",
    });
    return;
  }

  writeJson(
    response,
    200,
    {
      peers: state.cluster.peers,
      discovery: state.cluster.discovery,
    },
    request.method === "HEAD"
  );
}

async function handleMcpRequest(
  state: RuntimeState,
  request: IncomingMessage,
  response: ServerResponse
): Promise<void> {
  const server = createThingdMcpServer(state.db, {
    audit: state.audit,
    hardening: state.hardening,
  });
  const transport = new StreamableHTTPServerTransport({
    sessionIdGenerator: undefined,
  });

  response.on("close", () => {
    void transport.close();
    void server.close();
  });

  await server.connect(transport);
  await transport.handleRequest(request, response);
}

function requestPath(request: IncomingMessage): string {
  return new URL(request.url ?? "/", "http://thingd.local").pathname;
}

function isAuthorized(state: RuntimeState, request: IncomingMessage): boolean {
  if (!state.authToken) {
    return true;
  }

  return request.headers.authorization === `Bearer ${state.authToken}`;
}

function setCommonHeaders(response: ServerResponse): void {
  response.setHeader("Access-Control-Allow-Origin", "*");
  response.setHeader(
    "Access-Control-Allow-Headers",
    "Authorization, Content-Type, MCP-Protocol-Version"
  );
  response.setHeader("Access-Control-Allow-Methods", "POST, GET, HEAD, OPTIONS");
}

function writeJson(
  response: ServerResponse,
  statusCode: number,
  body: unknown,
  headersOnly = false
): void {
  response.writeHead(statusCode, {
    "Content-Type": "application/json",
  });

  if (headersOnly) {
    response.end();
    return;
  }

  response.end(JSON.stringify(body));
}

function listen(server: Server, port: number, host: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const onError = (error: Error) => {
      server.off("listening", onListening);
      reject(error);
    };
    const onListening = () => {
      server.off("error", onError);
      resolve();
    };

    server.once("error", onError);
    server.once("listening", onListening);
    server.listen(port, host);
  });
}

function close(server: Server): Promise<void> {
  return new Promise((resolve, reject) => {
    server.close((error) => {
      if (error) {
        reject(error);
        return;
      }

      resolve();
    });
  });
}

/** Parse Content-Length header. Returns null if absent or invalid. */
function parseContentLength(request: IncomingMessage): number | null {
  const header = request.headers["content-length"];
  if (!header) {
    return null;
  }
  const n = Number.parseInt(header, 10);
  return Number.isInteger(n) && n >= 0 ? n : null;
}

/**
 * Wrap a request in a PassThrough that enforces maxBytes for chunked transfers.
 * The MCP transport still drives reading — we just count bytes as they flow through
 * and destroy the stream (causing the transport to error) if the limit is exceeded.
 * The response is also aborted with HTTP 413 immediately on overflow.
 */
function wrapRequestWithSizeLimit(
  request: IncomingMessage,
  response: ServerResponse,
  maxBytes: number
): IncomingMessage {
  const pass = new PassThrough();
  let total = 0;
  let aborted = false;

  Object.defineProperties(pass, {
    method: { get: () => request.method },
    url: { get: () => request.url },
    headers: { get: () => request.headers },
  });

  request.on("data", (chunk: Buffer) => {
    if (aborted) {
      return;
    }
    total += chunk.length;
    if (total > maxBytes) {
      aborted = true;
      if (!response.headersSent) {
        writeJson(response, 413, {
          jsonrpc: "2.0",
          error: {
            code: -32_000,
            message: `Request body exceeds the maximum allowed size of ${maxBytes} bytes.`,
          },
          id: null,
        });
      }
      pass.destroy();
      request.destroy();
      return;
    }
    pass.push(chunk);
  });

  request.on("end", () => {
    if (!aborted) {
      pass.end();
    }
  });
  request.on("error", (err) => {
    if (!aborted) {
      pass.destroy(err);
    }
  });

  return pass as unknown as IncomingMessage;
}

function createReplicatingDb(originalDb: ThingD, mode: string): ThingD {
  if (mode !== "leader" && mode !== "single") {
    return originalDb;
  }

  const proxy = Object.create(originalDb) as ThingD;

  proxy.put = async (collection: string, object: MemoryObject) => {
    const stored = await originalDb.put(collection, object);
    if (!collection.startsWith("__thingd")) {
      try {
        await originalDb.events.append("__thingd:system:replication", {
          type: "replication.objects.put",
          collection,
          id: stored.id,
          object: stored,
        } as unknown as MemoryEvent);
      } catch (err) {
        console.error("Replication event append failed:", err);
      }
    }
    return stored;
  };

  proxy.delete = async (collection: string, id: string) => {
    const result = await originalDb.delete(collection, id);
    if (!collection.startsWith("__thingd")) {
      try {
        await originalDb.events.append("__thingd:system:replication", {
          type: "replication.objects.delete",
          collection,
          id,
        } as unknown as MemoryEvent);
      } catch (err) {
        console.error("Replication event append failed:", err);
      }
    }
    return result;
  };

  const originalEventsAppend = originalDb.events.append.bind(originalDb.events);
  Object.defineProperty(proxy, "events", {
    value: {
      ...originalDb.events,
      append: async (stream: string, event: MemoryEvent) => {
        const stored = await originalEventsAppend(stream, event);
        if (stream !== "__thingd:system:replication") {
          try {
            await originalEventsAppend("__thingd:system:replication", {
              type: "replication.events.append",
              stream,
              event: stored,
            } as unknown as MemoryEvent);
          } catch (err) {
            console.error("Replication event append failed:", err);
          }
        }
        return stored;
      },
    },
    writable: true,
    configurable: true,
  });

  return proxy;
}

function* resolveLeaderUrls(cluster: ResolvedThingdClusterOptions): Generator<string> {
  if (cluster.leaderUrl) {
    yield cluster.leaderUrl;
  }
  if (cluster.fallbackLeaderUrl) {
    yield cluster.fallbackLeaderUrl;
  }
}

function startReplicationRunner(state: RuntimeState) {
  const leaderUrl = state.cluster.leaderUrl;
  if (!leaderUrl) {
    return;
  }

  const pullInterval = parseInt(process.env.THINGD_CLUSTER_REPLICATION_INTERVAL_MS ?? "500", 10);
  const abort = new AbortController();
  state.replicationAbort = abort;
  const signal = abort.signal;

  async function runSync() {
    if (signal.aborted || state.replicationStopped) {
      return;
    }

    const status = await state.db.get("__thingd_meta", "replication_status");
    const lastSeq =
      status && typeof status.lastReplicatedSequence === "number"
        ? status.lastReplicatedSequence
        : 0;

    let fetched = false;

    for (const url of resolveLeaderUrls(state.cluster)) {
      if (signal.aborted || state.replicationStopped) {
        return;
      }
      try {
        const fetchUrl = new URL("/v1/replication/events", url);
        fetchUrl.searchParams.set("after", String(lastSeq));

        const headers: Record<string, string> = {
          Accept: "application/json",
        };
        if (state.cluster.forwardAuthToken) {
          headers.Authorization = `Bearer ${state.cluster.forwardAuthToken}`;
        }

        const response = await fetch(fetchUrl.toString(), {
          headers,
          signal: AbortSignal.timeout(10_000),
        });
        if (!response.ok) {
          throw new Error(`Leader replication returned HTTP ${response.status}`);
        }

        state.cluster.activeLeaderUrl = url;

        const resData = (await response.json()) as {
          success: boolean;
          events: {
            id: string;
            type: string;
            collection?: string;
            object?: Record<string, unknown>;
            stream?: string;
            event?: Record<string, unknown>;
          }[];
        };
        if (resData.success && Array.isArray(resData.events) && resData.events.length > 0) {
          for (const ev of resData.events) {
            if (signal.aborted || state.replicationStopped) {
              return;
            }

            const type = ev.type;

            if (type === "replication.objects.put" && ev.collection) {
              const { collection, object } = ev;
              const cleanObj = { ...object } as Record<string, unknown>;
              delete cleanObj.collection;
              delete cleanObj.createdAt;
              delete cleanObj.updatedAt;
              delete cleanObj.version;
              await state.db.put(collection, cleanObj as unknown as MemoryObject);
            } else if (type === "replication.objects.delete" && ev.collection && ev.id) {
              const { collection, id } = ev;
              await state.db.delete(collection, id);
            } else if (type === "replication.events.append" && ev.stream) {
              const { stream, event } = ev;
              const cleanEv = { ...event } as Record<string, unknown>;
              delete cleanEv.id;
              delete cleanEv.createdAt;
              delete cleanEv.stream;
              await state.db.events.append(stream, cleanEv as unknown as MemoryEvent);
            }

            const seq = Number.parseInt(ev.id, 10);
            await state.db.put("__thingd_meta", {
              id: "replication_status",
              lastReplicatedSequence: seq,
              updatedAt: new Date().toISOString(),
            } as unknown as MemoryObject);
          }
        }

        fetched = true;
        // Asynchronously update the cached replication lag so /healthz never
        // blocks on an outbound request. Fire-and-forget; errors are non-fatal.
        void updateCachedLag(state, url, lastSeq);
        break;
      } catch (error) {
        // Log replication failures periodically (once per electionMaxFailures)
        // to avoid flooding logs when the leader is unreachable.
        if (state.consecutiveReplicationFailures % state.cluster.electionMaxFailures === 0) {
          console.error(
            `Replication from ${url} failed:`,
            error instanceof Error ? error.message : String(error)
          );
        }
      }
    }

    if (signal.aborted || state.replicationStopped) {
      return;
    }

    if (!fetched) {
      state.consecutiveReplicationFailures++;

      // Print "All leader URLs exhausted" only once per failure threshold
      if (state.consecutiveReplicationFailures === state.cluster.electionMaxFailures) {
        console.error("All leader URLs exhausted for replication");
      }

      if (
        state.cluster.leaderElection &&
        state.consecutiveReplicationFailures >= state.cluster.electionMaxFailures
      ) {
        state.consecutiveReplicationFailures = 0;
        const promoted = attemptFollowerFailover(state);
        if (promoted) {
          // This node is now leader. Stop the replication runner.
          return;
        }
        // leaderUrl was updated by failover; retry immediately.
      }
    } else {
      state.consecutiveReplicationFailures = 0;
    }

    if (!signal.aborted && !state.replicationStopped) {
      state.replicationTimer = setTimeout(() => {
        void runSync();
      }, pullInterval);
    }
  }

  state.replicationTimer = setTimeout(() => {
    void runSync();
  }, pullInterval);
}

function stopReplicationRunner(state: RuntimeState) {
  state.replicationAbort?.abort();
  state.replicationStopped = true;
  if (state.replicationTimer) {
    clearTimeout(state.replicationTimer);
    state.replicationTimer = undefined;
  }
}

/**
 * Asynchronously fetch the leader's last replicated sequence and update
 * the cached lag on the cluster state. Called after each successful sync
 * so that /healthz can return the lag without making an outbound request.
 */
async function updateCachedLag(
  state: RuntimeState,
  leaderUrl: string,
  localLastSeq: number
): Promise<void> {
  try {
    const leaderStatusUrl = new URL("/cluster/status", leaderUrl).toString();
    const headers: Record<string, string> = { Accept: "application/json" };
    if (state.cluster.forwardAuthToken) {
      headers.Authorization = `Bearer ${state.cluster.forwardAuthToken}`;
    }
    const res = await fetch(leaderStatusUrl, {
      headers,
      signal: AbortSignal.timeout(5_000),
    });
    if (!res.ok) {
      return;
    }
    const leaderStatus = (await res.json()) as {
      replication?: { lastReplicatedSequence?: number };
    };
    const leaderSeq = leaderStatus?.replication?.lastReplicatedSequence;
    if (typeof leaderSeq === "number") {
      state.cluster.cachedReplicationLag = Math.max(0, leaderSeq - localLastSeq);
    }
  } catch {
    // Non-fatal — lag will remain at the last known value.
  }
}

/**
 * Attempt a static-config leader failover from this follower.
 * Returns true if this node promoted itself to leader.
 */
function attemptFollowerFailover(state: RuntimeState): boolean {
  if (state.replicationStopped) {
    return false;
  }

  const candidate = findNextLeaderCandidate(state.cluster);
  if (!candidate) {
    console.error("Leader failover: no candidate found in peer list");
    return false;
  }

  if (candidate.isSelf) {
    // Promote this node to leader.
    console.error(`Leader failover: promoting self (${candidate.url}) to leader`);
    state.db = createReplicatingDb(state.originalDb, "leader");
    state.cluster.mode = "leader";
    state.cluster.leaderUrl = candidate.url;
    state.cluster.activeLeaderUrl = candidate.url;
    state.cluster.fallbackLeaderUrl = undefined;
    stopReplicationRunner(state);
    state.consecutiveReplicationFailures = 0;
    return true;
  }

  // Redirect to the next candidate leader.
  console.error(`Leader failover: redirecting to ${candidate.url}`);
  state.cluster.leaderUrl = candidate.url;
  state.cluster.activeLeaderUrl = undefined;
  state.cluster.fallbackLeaderUrl = undefined;
  state.consecutiveReplicationFailures = 0;
  return false;
}

async function handleReplicationEvents(
  state: RuntimeState,
  request: IncomingMessage,
  response: ServerResponse
): Promise<void> {
  if (request.method !== "GET" && request.method !== "HEAD") {
    response.setHeader("Allow", "GET, HEAD, OPTIONS");
    writeJson(response, 405, { error: "method_not_allowed" });
    return;
  }

  const url = new URL(request.url ?? "", "http://localhost");
  const afterStr = url.searchParams.get("after");
  const afterSeq = afterStr ? Number.parseInt(afterStr, 10) : 0;
  const requestedLimit = Number.parseInt(url.searchParams.get("limit") ?? "500", 10);
  const limit = Number.isFinite(requestedLimit) ? Math.min(1000, Math.max(1, requestedLimit)) : 500;

  try {
    const filteredEvents = await state.db.events.list("__thingd:system:replication", {
      fromSequence: afterSeq,
      limit,
    });
    const changes = filteredEvents
      .map((event) => replicationChangeFromEvent(state.syncSourceId, event))
      .filter((change) => syncCollectionAllowed(state, change.collection));
    // Advance over the complete source page, not the last retained event.
    // Otherwise an excluded collection can be fetched forever when filters
    // are applied after the source event log has been read.
    const next = filteredEvents.at(-1)?.sequence ?? afterSeq;

    writeJson(
      response,
      200,
      {
        success: true,
        events: filteredEvents,
        data: {
          sourceId: state.syncSourceId,
          after: afterSeq,
          next,
          changes,
        },
      },
      request.method === "HEAD"
    );
  } catch (error) {
    writeJson(response, 500, {
      error: error instanceof Error ? error.message : String(error),
    });
  }
}

type PublicReplicationChange = {
  cursor: number;
  sourceId: string;
  idempotencyKey: string;
  operation: "object.upsert" | "object.delete" | "event.append";
  collection?: string;
  id?: string;
  payload: Record<string, unknown> | null;
};

function replicationChangeFromEvent(
  sourceId: string,
  event: StoredReplicationEvent
): PublicReplicationChange {
  const raw = parseReplicationBody(event);
  const type = typeof raw.type === "string" ? raw.type : "";
  if (type === "replication.objects.put") {
    return {
      cursor: event.sequence,
      sourceId,
      idempotencyKey: `${sourceId}:${event.sequence}`,
      operation: "object.upsert",
      collection: typeof raw.collection === "string" ? raw.collection : undefined,
      id: typeof raw.id === "string" ? raw.id : undefined,
      payload: { body: raw.object ?? {} },
    };
  }
  if (type === "replication.objects.delete") {
    return {
      cursor: event.sequence,
      sourceId,
      idempotencyKey: `${sourceId}:${event.sequence}`,
      operation: "object.delete",
      collection: typeof raw.collection === "string" ? raw.collection : undefined,
      id: typeof raw.id === "string" ? raw.id : undefined,
      payload: null,
    };
  }
  const appended = (raw.event ?? {}) as Record<string, unknown>;
  return {
    cursor: event.sequence,
    sourceId,
    idempotencyKey: `${sourceId}:${event.sequence}`,
    operation: "event.append",
    payload: {
      stream: typeof raw.stream === "string" ? raw.stream : "",
      type: typeof appended.type === "string" ? appended.type : "event",
      body: appended.body ?? appended,
    },
  };
}

type StoredReplicationEvent = MemoryEvent & { sequence: number };

function parseReplicationBody(event: StoredReplicationEvent): Record<string, unknown> {
  if (typeof event.body === "string") {
    try {
      const parsed = JSON.parse(event.body) as unknown;
      return parsed && typeof parsed === "object" ? (parsed as Record<string, unknown>) : {};
    } catch {
      return {};
    }
  }
  return event as unknown as Record<string, unknown>;
}

async function handleReplicationApply(
  state: RuntimeState,
  request: IncomingMessage,
  response: ServerResponse
): Promise<void> {
  if (request.method !== "POST") {
    response.setHeader("Allow", "POST, OPTIONS");
    writeJson(response, 405, { error: "method_not_allowed" });
    return;
  }
  if (state.syncRole !== "replica") {
    writeJson(response, 409, { error: "replica_required" });
    return;
  }
  if (state.syncProvider === "thingd.cloud" && !state.allowCloudTarget) {
    writeJson(response, 409, { error: "cloud_target_protected" });
    return;
  }

  try {
    const body = (await readJson(request)) as {
      sourceId?: string;
      changes?: PublicReplicationChange[];
    };
    const changes = Array.isArray(body.changes) ? body.changes : [];
    if (changes.some((change) => change.sourceId !== changes[0]?.sourceId)) {
      writeJson(response, 400, { error: "mixed_source_ids" });
      return;
    }
    const sourceId = body.sourceId ?? changes[0]?.sourceId;
    if (!sourceId) {
      writeJson(response, 400, { error: "source_id_required" });
      return;
    }
    const checkpointId = `source:${sourceId}`;
    const checkpoint = await state.originalDb.get<{ lastAppliedCursor?: number }>(
      "__thingd:sync_state",
      checkpointId
    );
    let lastAppliedCursor = checkpoint?.lastAppliedCursor ?? 0;
    let applied = 0;
    let skipped = 0;
    for (const change of changes) {
      if (change.cursor <= lastAppliedCursor) {
        skipped++;
        continue;
      }
      if (change.operation === "object.upsert" && change.collection && change.payload?.body) {
        if (!syncCollectionAllowed(state, change.collection)) {
          skipped++;
          continue;
        }
        const incoming = change.payload.body as unknown as MemoryObject;
        const existing = await state.originalDb.get<MemoryObject>(
          change.collection,
          change.id ?? incoming.id
        );
        const provenanceId = `${sourceId}:${change.collection}:${change.id ?? incoming.id}`;
        const provenance = await state.originalDb.get<{ sourceVersion?: number }>(
          "__thingd:sync_provenance",
          provenanceId
        );
        if (existing && !provenance) {
          await state.originalDb.putFromReplication("__thingd:sync_conflicts", {
            id: `${sourceId}:${change.cursor}`,
            sourceId,
            cursor: change.cursor,
            status: "quarantined",
            collection: change.collection,
            objectId: change.id ?? incoming.id,
          });
          writeJson(response, 409, { error: "replication_conflict_quarantined" });
          return;
        }
        await state.originalDb.putFromReplication(change.collection, incoming);
        await state.originalDb.putFromReplication("__thingd:sync_provenance", {
          id: provenanceId,
          sourceId,
          cursor: change.cursor,
          sourceVersion: incoming.version,
          createdAt: incoming.createdAt,
          updatedAt: incoming.updatedAt,
        });
      } else if (change.operation === "object.delete" && change.collection && change.id) {
        if (!syncCollectionAllowed(state, change.collection)) {
          skipped++;
          continue;
        }
        await state.originalDb.deleteFromReplication(change.collection, change.id);
        await state.originalDb.putFromReplication("__thingd:sync_tombstones", {
          id: `${sourceId}:${change.collection}:${change.id}`,
          sourceId,
          cursor: change.cursor,
          collection: change.collection,
          objectId: change.id,
          deleted: true,
        });
      } else if (change.operation === "event.append" && change.payload) {
        await state.originalDb.appendEventFromReplication(String(change.payload.stream ?? ""), {
          type: String(change.payload.type ?? "event"),
          body: change.payload.body,
        } as MemoryEvent);
      }
      lastAppliedCursor = change.cursor;
      applied++;
    }
    await state.originalDb.putFromReplication("__thingd:sync_state", {
      id: checkpointId,
      sourceId,
      lastAppliedCursor,
    });
    writeJson(response, 200, {
      success: true,
      data: { applied, skipped, lastAppliedCursor },
    });
  } catch (error) {
    writeJson(response, 400, { error: error instanceof Error ? error.message : String(error) });
  }
}

function syncCollectionAllowed(state: RuntimeState, collection?: string): boolean {
  if (!collection || collection.startsWith("__thingd")) {
    return true;
  }
  return state.syncCollections.length === 0 || state.syncCollections.includes(collection);
}

async function handleReplicationStatus(
  state: RuntimeState,
  request: IncomingMessage,
  response: ServerResponse
): Promise<void> {
  if (request.method !== "GET" && request.method !== "HEAD") {
    response.setHeader("Allow", "GET, HEAD, OPTIONS");
    writeJson(response, 405, { error: "method_not_allowed" });
    return;
  }
  const events = await state.originalDb.events.list("__thingd:system:replication", {
    limit: 1_000_000,
  });
  const conflicts = await state.originalDb.listObjects("__thingd:sync_conflicts", {
    limit: 10_000,
  });
  const last = events.at(-1) as (MemoryEvent & { sequence?: number }) | undefined;
  writeJson(
    response,
    200,
    {
      success: true,
      data: {
        sourceId: state.syncSourceId,
        role: state.syncRole,
        provider: state.syncProvider,
        projectId: state.syncProjectId,
        instanceSlug: state.syncInstanceSlug,
        latestCursor: last?.sequence ?? 0,
        quarantinedConflicts: conflicts.length,
      },
    },
    request.method === "HEAD"
  );
}

async function handleReplicationConflicts(
  state: RuntimeState,
  request: IncomingMessage,
  response: ServerResponse
): Promise<void> {
  if (request.method !== "GET" && request.method !== "HEAD") {
    response.setHeader("Allow", "GET, HEAD, OPTIONS");
    writeJson(response, 405, { error: "method_not_allowed" });
    return;
  }
  const conflicts = await state.originalDb.listObjects("__thingd:sync_conflicts", {
    limit: 10_000,
  });
  writeJson(
    response,
    200,
    { success: true, data: { sourceId: state.syncSourceId, conflicts } },
    request.method === "HEAD"
  );
}

async function handleReplicationSnapshot(
  state: RuntimeState,
  request: IncomingMessage,
  response: ServerResponse
): Promise<void> {
  if (request.method === "GET") {
    const collections = await state.originalDb.listCollections();
    const objects = (
      await Promise.all(
        collections
          .filter((collection) => syncCollectionAllowed(state, collection))
          .map((collection) => state.originalDb.listObjects(collection, { limit: 100_000 }))
      )
    )
      .flat()
      .map((object) => ({
        id: object.id,
        collection: object.collection,
        body: object,
        version: object.version,
        createdAt: object.createdAt,
        updatedAt: object.updatedAt,
      }));
    const events = (await state.originalDb.events.list(undefined, { limit: 100_000 })).filter(
      (event) => !event.stream.startsWith("__thingd")
    );
    const replicationEvents = await state.originalDb.events.list("__thingd:system:replication", {
      limit: 100_000,
    });
    writeJson(response, 200, {
      success: true,
      data: {
        sourceId: state.syncSourceId,
        cursor: replicationEvents.at(-1)?.sequence ?? 0,
        objects,
        events,
      },
    });
    return;
  }
  if (request.method !== "POST") {
    response.setHeader("Allow", "GET, POST, OPTIONS");
    writeJson(response, 405, { error: "method_not_allowed" });
    return;
  }
  if (state.syncRole !== "replica") {
    writeJson(response, 409, { error: "replica_required" });
    return;
  }
  if (state.syncProvider === "thingd.cloud" && !state.allowCloudTarget) {
    writeJson(response, 409, { error: "cloud_target_protected" });
    return;
  }
  try {
    const body = (await readJson(request)) as {
      sourceId?: string;
      replace?: boolean;
      snapshot?: {
        sourceId?: string;
        cursor?: number;
        objects?: Array<MemoryObject & { collection: string }>;
        events?: Array<MemoryEvent & { stream?: string }>;
      };
    };
    const snapshot = body.snapshot ?? {};
    const sourceId = body.sourceId ?? snapshot.sourceId;
    const objects = snapshot.objects ?? [];
    const events = snapshot.events ?? [];
    if (!sourceId) {
      writeJson(response, 400, { error: "source_id_required" });
      return;
    }
    if (body.replace) {
      for (const collection of await state.originalDb.listCollections()) {
        if (!syncCollectionAllowed(state, collection)) {
          continue;
        }
        for (const object of await state.originalDb.listObjects(collection, { limit: 100_000 })) {
          await state.originalDb.deleteFromReplication(collection, object.id);
        }
      }
    }
    for (const object of objects) {
      const incoming =
        object.body && typeof object.body === "object" ? { ...object.body, id: object.id } : object;
      await state.originalDb.putFromReplication(object.collection, incoming);
    }
    for (const event of events) {
      if (event.stream?.startsWith("__thingd")) {
        continue;
      }
      await state.originalDb.appendEventFromReplication(event.stream ?? "", {
        type: event.type,
        body: event.body ?? event.text,
        idempotencyKey: event.idempotencyKey,
      } as MemoryEvent);
    }
    const cursor = snapshot.cursor ?? 0;
    await state.originalDb.putFromReplication("__thingd:sync_state", {
      id: `source:${sourceId}`,
      sourceId,
      lastAppliedCursor: cursor,
    });
    writeJson(response, 200, {
      success: true,
      data: {
        sourceId,
        applied: objects.length,
        eventsApplied: events.length,
        lastAppliedCursor: cursor,
        verified: true,
      },
    });
  } catch (error) {
    writeJson(response, 400, { error: error instanceof Error ? error.message : String(error) });
  }
}

function readJson(request: IncomingMessage): Promise<unknown> {
  return new Promise((resolve, reject) => {
    let body = "";
    request.setEncoding("utf8");
    request.on("data", (chunk: string) => {
      body += chunk;
    });
    request.on("end", () => {
      try {
        resolve(body ? JSON.parse(body) : {});
      } catch (error) {
        reject(error);
      }
    });
    request.on("error", reject);
  });
}
