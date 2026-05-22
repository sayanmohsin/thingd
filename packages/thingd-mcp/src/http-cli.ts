#!/usr/bin/env node

import { readClusterOptionsFromEnv } from "./cluster.js";
import {
  type ThingDStorageDriver,
  parseBooleanFlag,
  parseThingdDriver,
  parsePort,
  readCliValue,
  readMcpAuditOptionsFromEnv,
} from "./config.js";
import { startThingdHttpServer } from "./http.js";

type HttpCliOptions = {
  path: string;
  driver?: ThingDStorageDriver;
  host: string;
  port: number;
  authToken?: string;
  allowUnauthenticated: boolean;
};

const options = parseHttpCliOptions(process.argv.slice(2));

if (!options.authToken) {
  console.error("thingd-mcp HTTP runtime is starting without THINGD_AUTH_TOKEN.");
}

const runtime = await startThingdHttpServer({
  path: options.path,
  driver: options.driver,
  host: options.host,
  port: options.port,
  authToken: options.authToken,
  allowUnauthenticated: options.allowUnauthenticated,
  audit: readMcpAuditOptionsFromEnv(process.env),
  cluster: readClusterOptionsFromEnv(process.env),
});

console.error(`thingd MCP HTTP runtime listening at ${runtime.mcpUrl}`);

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.once(signal, () => {
    void runtime.close().finally(() => process.exit(0));
  });
}

function parseHttpCliOptions(args: string[]): HttpCliOptions {
  const options: HttpCliOptions = {
    path: process.env.THINGD_PATH ?? ":memory:",
    driver: parseThingdDriver(process.env.THINGD_DRIVER),
    host: process.env.THINGD_HOST ?? "127.0.0.1",
    port: parsePort(process.env.THINGD_PORT, 8757),
    authToken: process.env.THINGD_AUTH_TOKEN,
    allowUnauthenticated: parseBooleanFlag(
      process.env.THINGD_ALLOW_UNAUTHENTICATED,
      "THINGD_ALLOW_UNAUTHENTICATED",
    ),
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];

    if (arg === "--path") {
      options.path = readCliValue(args, index, "--path");
      index += 1;
      continue;
    }

    if (arg === "--driver") {
      options.driver = parseThingdDriver(readCliValue(args, index, "--driver"));
      index += 1;
      continue;
    }

    if (arg === "--host") {
      options.host = readCliValue(args, index, "--host");
      index += 1;
      continue;
    }

    if (arg === "--port") {
      options.port = parsePort(readCliValue(args, index, "--port"), options.port);
      index += 1;
      continue;
    }

    if (arg === "--auth-token") {
      options.authToken = readCliValue(args, index, "--auth-token");
      index += 1;
      continue;
    }

    if (arg === "--allow-unauthenticated") {
      options.allowUnauthenticated = true;
      continue;
    }

    if (arg === "--help" || arg === "-h") {
      printHelp();
      process.exit(0);
    }

    throw new Error(`Unknown argument: ${arg}`);
  }

  return options;
}

function printHelp(): void {
  console.error(`thingd-mcp-http

Runs the thingd MCP server over Streamable HTTP.

Options:
  --path <path>          thingd database path. Defaults to THINGD_PATH or :memory:
  --driver <driver>     memory or native. Defaults to THINGD_DRIVER or memory
  --host <host>         bind host. Defaults to THINGD_HOST or 127.0.0.1
  --port <port>         bind port. Defaults to THINGD_PORT or 8757
  --auth-token <token>  bearer token. Defaults to THINGD_AUTH_TOKEN
  --allow-unauthenticated
                        allow tokenless non-loopback binding
  -h, --help            show this help

Cluster env:
  THINGD_CLUSTER_MODE=single|leader|follower
  THINGD_CLUSTER_LEADER_URL=http://thingd-leader:8757
  THINGD_CLUSTER_PEERS=http://thingd-0:8757,http://thingd-1:8757
  THINGD_CLUSTER_DISCOVERY=none|static|kubernetes
  THINGD_ADVERTISE_URL=http://pod-ip:8757
  THINGD_CLUSTER_FORWARD_AUTH_TOKEN=<leader-token>
`);
}
