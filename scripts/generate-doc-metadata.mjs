import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const resolveRepoPath = (...parts) => path.join(repoRoot, ...parts);

const ts = await readFile(resolveRepoPath("packages/thingd/src/mcp/tools.ts"), "utf8");
const rust = await readFile(resolveRepoPath("crates/thingd-server/src/mcp.rs"), "utf8");
const unique = (values) => [...new Set(values)].sort();
const sdkTools = unique(
  [...ts.matchAll(/registerTool\(\s*["'](thing_[a-z_]+)["']/g)].map((match) => match[1])
);
const sidecarTools = unique(
  [...rust.matchAll(/name:\s*"(thing_[a-z_]+)"/g)].map((match) => match[1])
);
const metadata = {
  generatedFrom: ["packages/thingd/src/mcp/tools.ts", "crates/thingd-server/src/mcp.rs"],
  sdkToolCount: sdkTools.length,
  sidecarToolCount: sidecarTools.length,
  schedulerToolCount: sdkTools.filter((tool) => tool.startsWith("thing_scheduler_")).length,
  sdkTools,
  sidecarTools,
};

await mkdir(resolveRepoPath("docs/.generated"), { recursive: true });
const generated = JSON.stringify(metadata, null, 2).replace(
  '  "generatedFrom": [\n    "packages/thingd/src/mcp/tools.ts",\n    "crates/thingd-server/src/mcp.rs"\n  ],',
  '  "generatedFrom": ["packages/thingd/src/mcp/tools.ts", "crates/thingd-server/src/mcp.rs"],'
);
await writeFile(resolveRepoPath("docs/.generated/mcp-metadata.json"), `${generated}\n`);
console.log(`Generated MCP metadata: ${sdkTools.length} SDK, ${sidecarTools.length} sidecar tools.`);
