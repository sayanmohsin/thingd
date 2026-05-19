#!/usr/bin/env node

import { parseMemorydDriver, parsePort, readCliValue } from "./config.js";
import { startMemorydHttpServer } from "./http.js";

type HttpCliOptions = {
  path: string;
  driver?: "memory" | "native";
  host: string;
  port: number;
  authToken?: string;
};

const options = parseHttpCliOptions(process.argv.slice(2));

if (!options.authToken) {
  console.error("memoryd-mcp HTTP runtime is starting without MEMORYD_AUTH_TOKEN.");
}

const runtime = await startMemorydHttpServer({
  path: options.path,
  driver: options.driver,
  host: options.host,
  port: options.port,
  authToken: options.authToken,
});

console.error(`memoryd MCP HTTP runtime listening at ${runtime.mcpUrl}`);

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.once(signal, () => {
    void runtime.close().finally(() => process.exit(0));
  });
}

function parseHttpCliOptions(args: string[]): HttpCliOptions {
  const options: HttpCliOptions = {
    path: process.env.MEMORYD_PATH ?? ":memory:",
    driver: parseMemorydDriver(process.env.MEMORYD_DRIVER),
    host: process.env.MEMORYD_HOST ?? "127.0.0.1",
    port: parsePort(process.env.MEMORYD_PORT, 8757),
    authToken: process.env.MEMORYD_AUTH_TOKEN,
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];

    if (arg === "--path") {
      options.path = readCliValue(args, index, "--path");
      index += 1;
      continue;
    }

    if (arg === "--driver") {
      options.driver = parseMemorydDriver(readCliValue(args, index, "--driver"));
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

    if (arg === "--help" || arg === "-h") {
      printHelp();
      process.exit(0);
    }

    throw new Error(`Unknown argument: ${arg}`);
  }

  return options;
}

function printHelp(): void {
  console.error(`memoryd-mcp-http

Runs the memoryd MCP server over Streamable HTTP.

Options:
  --path <path>          memoryd database path. Defaults to MEMORYD_PATH or :memory:
  --driver <driver>     memory or native. Defaults to MEMORYD_DRIVER or memory
  --host <host>         bind host. Defaults to MEMORYD_HOST or 127.0.0.1
  --port <port>         bind port. Defaults to MEMORYD_PORT or 8757
  --auth-token <token>  bearer token. Defaults to MEMORYD_AUTH_TOKEN
  -h, --help            show this help
`);
}
