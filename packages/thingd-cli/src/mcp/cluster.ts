import type { IncomingMessage, ServerResponse } from "node:http";
import type { ThingD } from "thingd";
import { parseBooleanFlag, parsePort } from "./config.js";

export type ThingdClusterMode = "single" | "leader" | "follower";
export type ThingdClusterDiscovery = "none" | "static" | "kubernetes";

export type ThingdClusterOptions = {
  mode?: ThingdClusterMode;
  advertiseUrl?: string;
  leaderUrl?: string;
  fallbackLeaderUrl?: string;
  peers?: string[];
  discovery?: ThingdClusterDiscovery;
  service?: string;
  namespace?: string;
  port?: number;
  forwardAuthToken?: string;
  statusPath?: string;
  peersPath?: string;
  /** Enable automatic leader failover. Default: false. */
  leaderElection?: boolean;
  /** Consecutive replication failures before triggering election. Default: 3. */
  electionMaxFailures?: number;
};

export type ResolvedThingdClusterOptions = {
  mode: ThingdClusterMode;
  advertiseUrl?: string;
  leaderUrl?: string;
  fallbackLeaderUrl?: string;
  activeLeaderUrl?: string;
  peers: string[];
  discovery: ThingdClusterDiscovery;
  service?: string;
  namespace?: string;
  port: number;
  forwardAuthToken?: string;
  statusPath: string;
  peersPath: string;
  leaderElection: boolean;
  electionMaxFailures: number;
};

export type ThingdClusterStatus = {
  mode: ThingdClusterMode;
  writable: boolean;
  forwarding: boolean;
  leaderUrl?: string;
  fallbackLeaderUrl?: string;
  activeLeaderUrl?: string;
  advertiseUrl?: string;
  discovery: ThingdClusterDiscovery;
  peers: string[];
  leaderElection: boolean;
  electionMaxFailures: number;
  replication:
    | {
        lastReplicatedSequence: number;
        status: string;
        lag?: number;
      }
    | "not-implemented";
};

const DEFAULT_CLUSTER_PORT = 8757;

export function readClusterOptionsFromEnv(
  env: Record<string, string | undefined>
): ThingdClusterOptions {
  return {
    mode: parseClusterMode(env.THINGD_CLUSTER_MODE),
    advertiseUrl: env.THINGD_ADVERTISE_URL,
    leaderUrl: env.THINGD_CLUSTER_LEADER_URL,
    fallbackLeaderUrl: env.THINGD_CLUSTER_LEADER_FALLBACK_URL,
    peers: parsePeers(env.THINGD_CLUSTER_PEERS),
    discovery: parseClusterDiscovery(env.THINGD_CLUSTER_DISCOVERY),
    service: env.THINGD_CLUSTER_SERVICE,
    namespace: env.THINGD_CLUSTER_NAMESPACE,
    port: parsePort(env.THINGD_CLUSTER_PORT, DEFAULT_CLUSTER_PORT),
    forwardAuthToken: env.THINGD_CLUSTER_FORWARD_AUTH_TOKEN ?? env.THINGD_AUTH_TOKEN,
    leaderElection: parseBooleanFlag(
      env.THINGD_CLUSTER_LEADER_ELECTION,
      "THINGD_CLUSTER_LEADER_ELECTION"
    ),
    electionMaxFailures: parsePositiveInt(env.THINGD_CLUSTER_LEADER_ELECTION_MAX_FAILURES, 3),
  };
}

export function resolveClusterOptions(
  options: ThingdClusterOptions | undefined
): ResolvedThingdClusterOptions {
  const mode = options?.mode ?? "single";
  const discovery = options?.discovery ?? (options?.peers?.length ? "static" : "none");
  const port = options?.port ?? DEFAULT_CLUSTER_PORT;
  const peers = resolvePeers({
    discovery,
    peers: options?.peers ?? [],
    service: options?.service,
    namespace: options?.namespace,
    port,
  });

  const leaderElection = options?.leaderElection ?? false;
  const electionMaxFailures = options?.electionMaxFailures ?? 3;

  // When leader election is enabled, derive leaderUrl from peer list if not explicitly set.
  if (mode === "follower" && !options?.leaderUrl && leaderElection && peers.length > 0) {
    options = { ...options, leaderUrl: peers[0] };
  }

  if (mode === "follower" && !options?.leaderUrl) {
    throw new Error("THINGD_CLUSTER_LEADER_URL is required when THINGD_CLUSTER_MODE=follower");
  }

  return {
    mode,
    advertiseUrl: options?.advertiseUrl,
    leaderUrl: options?.leaderUrl,
    fallbackLeaderUrl: options?.fallbackLeaderUrl,
    activeLeaderUrl: undefined,
    peers,
    discovery,
    service: options?.service,
    namespace: options?.namespace,
    port,
    forwardAuthToken: options?.forwardAuthToken,
    statusPath: options?.statusPath ?? "/cluster/status",
    peersPath: options?.peersPath ?? "/cluster/peers",
    leaderElection,
    electionMaxFailures,
  };
}

export async function getClusterStatus(
  cluster: ResolvedThingdClusterOptions,
  db: ThingD
): Promise<ThingdClusterStatus> {
  let replication: ThingdClusterStatus["replication"] = "not-implemented";

  if (cluster.mode === "follower") {
    try {
      const status = await db.get("__thingd_meta", "replication_status");
      const lastSeq =
        status && typeof status.lastReplicatedSequence === "number"
          ? status.lastReplicatedSequence
          : 0;

      let lag = 0;
      let statusStr = "syncing";
      if (cluster.leaderUrl) {
        try {
          const leaderStatusUrl = new URL("/cluster/status", cluster.leaderUrl).toString();
          const headers: Record<string, string> = { Accept: "application/json" };
          if (cluster.forwardAuthToken) {
            headers.Authorization = `Bearer ${cluster.forwardAuthToken}`;
          }
          const leaderRes = await fetch(leaderStatusUrl, {
            headers,
            signal: AbortSignal.timeout(1000),
          });
          if (leaderRes.ok) {
            const leaderStatus = (await leaderRes.json()) as {
              replication?: { lastReplicatedSequence?: number };
            };
            const leaderSeq = leaderStatus?.replication?.lastReplicatedSequence;
            if (typeof leaderSeq === "number") {
              lag = Math.max(0, leaderSeq - lastSeq);
            }
          } else {
            statusStr = "error";
          }
        } catch {
          statusStr = "error";
        }
      }

      replication = {
        lastReplicatedSequence: lastSeq,
        status: statusStr,
        lag,
      };
    } catch {
      replication = { lastReplicatedSequence: 0, status: "error" };
    }
  } else if (cluster.mode === "leader" || cluster.mode === "single") {
    try {
      const list = await db.events.list("__thingd:system:replication");
      const lastEvent = list[list.length - 1];
      const lastSeq = lastEvent ? Number.parseInt(lastEvent.id, 10) : 0;
      replication = {
        lastReplicatedSequence: lastSeq,
        status: "active",
      };
    } catch {
      replication = { lastReplicatedSequence: 0, status: "error" };
    }
  }

  return {
    mode: cluster.mode,
    writable: cluster.mode !== "follower",
    forwarding: cluster.mode === "follower",
    leaderUrl: cluster.leaderUrl,
    fallbackLeaderUrl: cluster.fallbackLeaderUrl,
    activeLeaderUrl: cluster.activeLeaderUrl,
    advertiseUrl: cluster.advertiseUrl,
    discovery: cluster.discovery,
    peers: cluster.peers,
    leaderElection: cluster.leaderElection,
    electionMaxFailures: cluster.electionMaxFailures,
    replication,
  };
}

export async function forwardMcpRequestToLeader(
  cluster: ResolvedThingdClusterOptions,
  mcpPath: string,
  request: IncomingMessage,
  response: ServerResponse
): Promise<void> {
  if (!cluster.leaderUrl) {
    writeForwardError(response, 503, "cluster_leader_unavailable");
    return;
  }

  const body = await readRequestBody(request);
  const urls = cluster.fallbackLeaderUrl
    ? [cluster.leaderUrl, cluster.fallbackLeaderUrl]
    : [cluster.leaderUrl];

  let lastError: Error | undefined;

  for (const url of urls) {
    try {
      const upstreamUrl = leaderMcpUrl(url, mcpPath);
      const upstream = await fetch(upstreamUrl, {
        method: "POST",
        headers: forwardedHeaders(request, cluster.forwardAuthToken),
        body: new Uint8Array(body),
        signal: AbortSignal.timeout(30_000),
      });

      cluster.activeLeaderUrl = url;

      response.writeHead(upstream.status, responseHeaders(upstream.headers));
      response.end(Buffer.from(await upstream.arrayBuffer()));
      return;
    } catch (error) {
      lastError = error instanceof Error ? error : new Error(String(error));
      console.error(`Forward to leader ${url} failed:`, lastError.message);
    }
  }

  if (lastError) {
    console.error("All leader URLs exhausted for forward, last error:", lastError.message);
  }
  writeForwardError(response, 503, "cluster_leader_unavailable");
}

export function findNextLeaderCandidate(
  cluster: ResolvedThingdClusterOptions
): { url: string; isSelf: boolean } | undefined {
  const { advertiseUrl, peers } = cluster;
  const currentLeaderUrl = cluster.activeLeaderUrl ?? cluster.leaderUrl;
  if (!currentLeaderUrl || peers.length === 0) {
    return undefined;
  }

  const leaderIndex = findPeerIndex(peers, currentLeaderUrl);
  if (leaderIndex === -1) {
    return undefined;
  }

  // Scan from the next peer after the current leader.
  for (let i = leaderIndex + 1; i < peers.length; i++) {
    const candidate = peers[i];
    if (candidate) {
      const isSelf = typeof advertiseUrl === "string" && sameOrigin(advertiseUrl, candidate);
      return { url: candidate, isSelf };
    }
  }

  // Wrap around — all peers after leader's index exhausted, try from the beginning.
  for (let i = 0; i < leaderIndex; i++) {
    const candidate = peers[i];
    if (candidate) {
      const isSelf = typeof advertiseUrl === "string" && sameOrigin(advertiseUrl, candidate);
      return { url: candidate, isSelf };
    }
  }

  return undefined;
}

function findPeerIndex(peers: string[], targetUrl: string): number {
  return peers.findIndex((peer) => sameOrigin(peer, targetUrl));
}

/** Compare two URLs by origin (scheme + host + port). */
function sameOrigin(a: string, b: string): boolean {
  try {
    return new URL(a).origin === new URL(b).origin;
  } catch {
    return a === b;
  }
}

function parsePositiveInt(value: string | undefined, fallback: number): number {
  if (!value) {
    return fallback;
  }

  const n = Number.parseInt(value, 10);
  if (!Number.isInteger(n) || n <= 0) {
    throw new Error(`Invalid positive integer: ${value}`);
  }

  return n;
}

function parseClusterMode(value: string | undefined): ThingdClusterMode | undefined {
  if (!value) {
    return undefined;
  }

  if (value === "single" || value === "leader" || value === "follower") {
    return value;
  }

  throw new Error(`Unsupported THINGD_CLUSTER_MODE: ${value}`);
}

function parseClusterDiscovery(value: string | undefined): ThingdClusterDiscovery | undefined {
  if (!value) {
    return undefined;
  }

  if (value === "none" || value === "static" || value === "kubernetes") {
    return value;
  }

  throw new Error(`Unsupported THINGD_CLUSTER_DISCOVERY: ${value}`);
}

function parsePeers(value: string | undefined): string[] | undefined {
  if (!value) {
    return undefined;
  }

  return value
    .split(",")
    .map((peer) => peer.trim())
    .filter(Boolean);
}

function resolvePeers(options: {
  discovery: ThingdClusterDiscovery;
  peers: string[];
  service?: string;
  namespace?: string;
  port: number;
}): string[] {
  if (options.discovery === "static") {
    return options.peers;
  }

  if (options.discovery !== "kubernetes" || !options.service) {
    return [];
  }

  const namespace = options.namespace ?? "default";
  return [`http://${options.service}.${namespace}.svc.cluster.local:${options.port}`];
}

function leaderMcpUrl(leaderUrl: string, mcpPath: string): string {
  const url = new URL(leaderUrl);
  if (url.pathname === "/" || url.pathname === "") {
    url.pathname = mcpPath;
  }

  return url.toString();
}

function forwardedHeaders(request: IncomingMessage, forwardAuthToken: string | undefined): Headers {
  const headers = new Headers();
  const contentType = request.headers["content-type"];
  const protocolVersion = request.headers["mcp-protocol-version"];
  const accept = request.headers.accept;

  if (typeof contentType === "string") {
    headers.set("Content-Type", contentType);
  }

  if (typeof accept === "string") {
    headers.set("Accept", accept);
  }

  if (typeof protocolVersion === "string") {
    headers.set("MCP-Protocol-Version", protocolVersion);
  }

  if (forwardAuthToken) {
    headers.set("Authorization", `Bearer ${forwardAuthToken}`);
  } else if (typeof request.headers.authorization === "string") {
    headers.set("Authorization", request.headers.authorization);
  }

  return headers;
}

function responseHeaders(headers: Headers): Record<string, string> {
  const result: Record<string, string> = {};

  for (const [key, value] of headers) {
    if (!isHopByHopHeader(key)) {
      result[key] = value;
    }
  }

  return result;
}

function isHopByHopHeader(header: string): boolean {
  return [
    "connection",
    "content-length",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
  ].includes(header.toLowerCase());
}

function readRequestBody(request: IncomingMessage): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];

    request.on("data", (chunk: Buffer) => {
      chunks.push(chunk);
    });
    request.on("end", () => {
      resolve(Buffer.concat(chunks));
    });
    request.on("error", reject);
  });
}

function writeForwardError(response: ServerResponse, statusCode: number, error: string): void {
  response.writeHead(statusCode, {
    "Content-Type": "application/json",
  });
  response.end(
    JSON.stringify({
      jsonrpc: "2.0",
      error: {
        code: -32_603,
        message: error,
      },
      id: null,
    })
  );
}
