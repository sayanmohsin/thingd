#!/usr/bin/env node

import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { ThingD } from "thingd";
import {
  parseThingdDriver,
  readCliValue,
  readMcpAuditOptionsFromEnv,
  type ThingDStorageDriver,
} from "./config.js";
import { createThingdMcpServer } from "./server.js";

type CliOptions = {
  path: string;
  driver?: ThingDStorageDriver;
};

const options = parseCliOptions(process.argv.slice(2));
const db = await ThingD.open({
  path: options.path,
  driver: options.driver,
});
const server = createThingdMcpServer(db, {
  audit: readMcpAuditOptionsFromEnv(process.env),
});
const transport = new StdioServerTransport();

await server.connect(transport);

function parseCliOptions(args: string[]): CliOptions {
  const options: CliOptions = {
    path: process.env.THINGD_PATH ?? ":memory:",
    driver: parseThingdDriver(process.env.THINGD_DRIVER),
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

    if (arg === "--help" || arg === "-h") {
      printHelp();
      process.exit(0);
    }

    throw new Error(`Unknown argument: ${arg}`);
  }

  return options;
}

function printHelp(): void {
  console.error(`thingd-mcp

Runs the thingd MCP server over stdio.

Options:
  --path <path>       thingd database path. Defaults to THINGD_PATH or :memory:
  --driver <driver>  memory or native. Defaults to THINGD_DRIVER or memory
  -h, --help         show this help
`);
}
