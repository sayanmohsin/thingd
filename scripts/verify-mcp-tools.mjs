import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");

const [nodeSource, rustSource] = await Promise.all([
  readFile(resolve(root, "packages/thingd/src/mcp/tools.ts"), "utf8"),
  readFile(resolve(root, "crates/thingd-server/src/mcp.rs"), "utf8"),
]);

function toolNames(source, pattern) {
  return new Set([...source.matchAll(pattern)].map((match) => match[1]));
}

const nodeTools = toolNames(nodeSource, /"(thing_[a-z0-9_]+)"/g);
const rustTools = toolNames(rustSource, /name: "(thing_[a-z0-9_]+)"/g);
const schedulerTools = new Set([...nodeTools].filter((name) => name.startsWith("thing_scheduler_")));
const nodeOnly = [...nodeTools].filter((name) => !rustTools.has(name)).sort();
const rustOnly = [...rustTools].filter((name) => !nodeTools.has(name)).sort();

if (nodeTools.size !== 46) {
  throw new Error(`Node SDK MCP tool count changed: expected 46, got ${nodeTools.size}`);
}
if (rustTools.size !== 36) {
  throw new Error(`Rust sidecar MCP tool count changed: expected 36, got ${rustTools.size}`);
}
if (schedulerTools.size !== 10 || nodeOnly.join("\n") !== [...schedulerTools].sort().join("\n")) {
  throw new Error("Expected the ten scheduler tools to be the only Node-only MCP tools");
}
if (rustOnly.length > 0) {
  throw new Error(`Rust sidecar has unexpected tools not exposed by Node SDK: ${rustOnly.join(", ")}`);
}

console.log(`MCP tool surfaces verified: Node SDK=${nodeTools.size}, Rust sidecar=${rustTools.size}`);
console.log(`Node-only scheduler tools=${schedulerTools.size}`);
