import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { StreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/streamableHttp.js";
import { ThingD } from "thingd";
import type { ThingdMcpAuditOptions } from "./audit.js";
import {
  clusterStatus,
  forwardMcpRequestToLeader,
  type ThingdClusterOptions,
  type ResolvedThingdClusterOptions,
  resolveClusterOptions,
} from "./cluster.js";
import { ensureHttpRuntimeIsSafe, type ThingDStorageDriver } from "./config.js";
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

    if (state.cluster.mode === "follower") {
      await forwardMcpRequestToLeader(state.cluster, state.mcpPath, request, response);
      return;
    }

    await handleMcpRequest(state, request, response);
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
