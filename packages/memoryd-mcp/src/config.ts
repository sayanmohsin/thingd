import type { MemoryDDriver } from "@sayanmohsin/memoryd";

export function parseMemorydDriver(value: string | undefined): MemoryDDriver | undefined {
  if (!value) {
    return undefined;
  }

  if (value === "memory" || value === "native") {
    return value;
  }

  throw new Error(`Unsupported memoryd driver: ${value}`);
}

export function parsePort(value: string | undefined, fallback: number): number {
  if (!value) {
    return fallback;
  }

  const port = Number.parseInt(value, 10);

  if (!Number.isInteger(port) || port < 0 || port > 65_535) {
    throw new Error(`Invalid port: ${value}`);
  }

  return port;
}

export function readCliValue(args: string[], index: number, name: string): string {
  const value = args[index + 1];

  if (!value) {
    throw new Error(`${name} requires a value`);
  }

  return value;
}
