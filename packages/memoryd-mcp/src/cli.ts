#!/usr/bin/env node

import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { MemoryD, type MemoryDDriver } from "@sayanmohsin/memoryd";
import { parseMemorydDriver, readCliValue } from "./config.js";
import { createMemorydMcpServer } from "./server.js";

type CliOptions = {
  path: string;
  driver?: MemoryDDriver;
};

const options = parseCliOptions(process.argv.slice(2));
const db = await MemoryD.open({
  path: options.path,
  driver: options.driver,
});
const server = createMemorydMcpServer(db);
const transport = new StdioServerTransport();

await server.connect(transport);

function parseCliOptions(args: string[]): CliOptions {
  const options: CliOptions = {
    path: process.env.MEMORYD_PATH ?? ":memory:",
    driver: parseMemorydDriver(process.env.MEMORYD_DRIVER),
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

    if (arg === "--help" || arg === "-h") {
      printHelp();
      process.exit(0);
    }

    throw new Error(`Unknown argument: ${arg}`);
  }

  return options;
}

function printHelp(): void {
  console.error(`memoryd-mcp

Runs the memoryd MCP server over stdio.

Options:
  --path <path>       memoryd database path. Defaults to MEMORYD_PATH or :memory:
  --driver <driver>  memory or native. Defaults to MEMORYD_DRIVER or memory
  -h, --help         show this help
`);
}
