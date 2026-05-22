import type { IncomingMessage, ServerResponse } from "node:http";
import { parsePort } from "./config.js";

export type ThingdClusterMode = "single" | "leader" | "follower";
export type ThingdClusterDiscovery = "none" | "static" | "kubernetes";

export type ThingdClusterOptions = {
  mode?: ThingdClusterMode;
  advertiseUrl?: string;
  leaderUrl?: string;
  peers?: string[];
  discovery?: ThingdClusterDiscovery;
  service?: string;
  namespace?: string;
  port?: number;
  forwardAuthToken?: string;
  statusPath?: string;
  peersPath?: string;
};

export type ResolvedThingdClusterOptions = {
  mode: ThingdClusterMode;
  advertiseUrl?: string;
  leaderUrl?: string;
  peers: string[];
  discovery: ThingdClusterDiscovery;
  service?: string;
  namespace?: string;
  port: number;
  forwardAuthToken?: string;
  statusPath: string;
  peersPath: string;
};

export type ThingdClusterStatus = {
  mode: ThingdClusterMode;
  writable: boolean;
  forwarding: boolean;
  leaderUrl?: string;
  advertiseUrl?: string;
  discovery: ThingdClusterDiscovery;
  peers: string[];
  replication: "not-implemented";
};

const DEFAULT_CLUSTER_PORT = 8757;

export function readClusterOptionsFromEnv(
  env: Record<string, string | undefined>,
): ThingdClusterOptions {
  return {
    mode: parseClusterMode(env.THINGD_CLUSTER_MODE),
    advertiseUrl: env.THINGD_ADVERTISE_URL,
    leaderUrl: env.THINGD_CLUSTER_LEADER_URL,
    peers: parsePeers(env.THINGD_CLUSTER_PEERS),
    discovery: parseClusterDiscovery(env.THINGD_CLUSTER_DISCOVERY),
    service: env.THINGD_CLUSTER_SERVICE,
    namespace: env.THINGD_CLUSTER_NAMESPACE,
    port: parsePort(env.THINGD_CLUSTER_PORT, DEFAULT_CLUSTER_PORT),
    forwardAuthToken: env.THINGD_CLUSTER_FORWARD_AUTH_TOKEN ?? env.THINGD_AUTH_TOKEN,
  };
}

export function resolveClusterOptions(
  options: ThingdClusterOptions | undefined,
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

  if (mode === "follower" && !options?.leaderUrl) {
    throw new Error("THINGD_CLUSTER_LEADER_URL is required when THINGD_CLUSTER_MODE=follower");
  }

  return {
    mode,
    advertiseUrl: options?.advertiseUrl,
    leaderUrl: options?.leaderUrl,
    peers,
    discovery,
    service: options?.service,
    namespace: options?.namespace,
    port,
    forwardAuthToken: options?.forwardAuthToken,
    statusPath: options?.statusPath ?? "/cluster/status",
    peersPath: options?.peersPath ?? "/cluster/peers",
  };
}

export function clusterStatus(cluster: ResolvedThingdClusterOptions): ThingdClusterStatus {
  return {
    mode: cluster.mode,
    writable: cluster.mode !== "follower",
    forwarding: cluster.mode === "follower",
    leaderUrl: cluster.leaderUrl,
    advertiseUrl: cluster.advertiseUrl,
    discovery: cluster.discovery,
    peers: cluster.peers,
    replication: "not-implemented",
  };
}

export async function forwardMcpRequestToLeader(
  cluster: ResolvedThingdClusterOptions,
  mcpPath: string,
  request: IncomingMessage,
  response: ServerResponse,
): Promise<void> {
  if (!cluster.leaderUrl) {
    writeForwardError(response, 503, "cluster_leader_unavailable");
    return;
  }

  const upstreamUrl = leaderMcpUrl(cluster.leaderUrl, mcpPath);
  const body = await readRequestBody(request);
  const upstream = await fetch(upstreamUrl, {
    method: "POST",
    headers: forwardedHeaders(request, cluster.forwardAuthToken),
    body: new Uint8Array(body),
  });

  response.writeHead(upstream.status, responseHeaders(upstream.headers));
  response.end(Buffer.from(await upstream.arrayBuffer()));
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
    }),
  );
}
