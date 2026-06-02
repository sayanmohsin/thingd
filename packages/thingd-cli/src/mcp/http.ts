import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { PassThrough } from "node:stream";
import { StreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/streamableHttp.js";
import { ThingD } from "thingd";
import type { ThingdMcpAuditOptions } from "./audit.js";
import {
  clusterStatus,
  forwardMcpRequestToLeader,
  type ResolvedThingdClusterOptions,
  resolveClusterOptions,
  type ThingdClusterOptions,
} from "./cluster.js";
import {
  ensureHttpRuntimeIsSafe,
  type ThingDStorageDriver,
  type ThingdMcpHardeningOptions,
} from "./config.js";
import { createThingdMcpServer } from "./server.js";

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
};

export type RunningThingdHttpServer = {
  server: Server;
  url: string;
  mcpUrl: string;
  close(): Promise<void>;
};

type RuntimeState = {
  db: ThingD;
  authToken?: string;
  mcpPath: string;
  healthPath: string;
  driver: ThingDStorageDriver | "memory";
  audit?: ThingdMcpAuditOptions | false;
  cluster: ResolvedThingdClusterOptions;
  hardening?: ThingdMcpHardeningOptions;
};

export async function startThingdHttpServer(
  options: ThingdHttpServerOptions,
): Promise<RunningThingdHttpServer> {
  const host = options.host ?? "127.0.0.1";
  const port = options.port ?? 8757;
  ensureHttpRuntimeIsSafe({
    host,
    authToken: options.authToken,
    allowUnauthenticated: options.allowUnauthenticated,
  });

  const db = await ThingD.open({
    path: options.path,
    driver: options.driver,
  });
  const cluster = resolveClusterOptions(options.cluster);
  const state: RuntimeState = {
    db,
    authToken: options.authToken,
    mcpPath: options.mcpPath ?? "/mcp",
    healthPath: options.healthPath ?? "/healthz",
    driver: options.driver ?? "memory",
    audit: options.audit,
    cluster,
    hardening: options.hardening,
  };
  const server = createServer((request, response) => {
    void handleRequest(state, request, response);
  });

  await listen(server, port, host);

  const address = server.address();
  const resolvedPort = typeof address === "object" && address ? address.port : port;
  const displayHost = host === "0.0.0.0" ? "127.0.0.1" : host;
  const url = `http://${displayHost}:${resolvedPort}`;

  return {
    server,
    url,
    mcpUrl: `${url}${state.mcpPath}`,
    close: () => close(server),
  };
}

async function handleRequest(
  state: RuntimeState,
  request: IncomingMessage,
  response: ServerResponse,
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
      handleHealth(state, request, response);
      return;
    }

    if (path === state.cluster.statusPath) {
      handleClusterStatus(state, request, response);
      return;
    }

    if (path === state.cluster.peersPath) {
      handleClusterPeers(state, request, response);
      return;
    }

    if (path !== state.mcpPath) {
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
    if (!response.headersSent) {
      writeJson(response, 500, {
        jsonrpc: "2.0",
        error: {
          code: -32_603,
          message: error instanceof Error ? error.message : "Internal server error",
        },
        id: null,
      });
      return;
    }

    response.end();
  }
}

function handleHealth(
  state: RuntimeState,
  request: IncomingMessage,
  response: ServerResponse,
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
      ok: true,
      service: "thingd-mcp",
      driver: state.driver,
      mcpPath: state.mcpPath,
      cluster: clusterStatus(state.cluster),
    },
    request.method === "HEAD",
  );
}

function handleClusterStatus(
  state: RuntimeState,
  request: IncomingMessage,
  response: ServerResponse,
): void {
  if (request.method !== "GET" && request.method !== "HEAD") {
    response.setHeader("Allow", "GET, HEAD, OPTIONS");
    writeJson(response, 405, {
      error: "method_not_allowed",
    });
    return;
  }

  writeJson(response, 200, clusterStatus(state.cluster), request.method === "HEAD");
}

function handleClusterPeers(
  state: RuntimeState,
  request: IncomingMessage,
  response: ServerResponse,
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
    request.method === "HEAD",
  );
}

async function handleMcpRequest(
  state: RuntimeState,
  request: IncomingMessage,
  response: ServerResponse,
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
    "Authorization, Content-Type, MCP-Protocol-Version",
  );
  response.setHeader("Access-Control-Allow-Methods", "POST, GET, HEAD, OPTIONS");
}

function writeJson(
  response: ServerResponse,
  statusCode: number,
  body: unknown,
  headersOnly = false,
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
  if (!header) return null;
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
  maxBytes: number,
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
    if (aborted) return;
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
    if (!aborted) pass.end();
  });
  request.on("error", (err) => {
    if (!aborted) pass.destroy(err);
  });

  return pass as unknown as IncomingMessage;
}
