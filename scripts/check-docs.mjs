import { readFile, readdir, stat } from "node:fs/promises";
import path from "node:path";

const files = {
  readme: "README.md",
  mcp: "docs/api-spec/mcp-tools.md",
  apiIndex: "docs/api-spec/index.md",
  serverReadme: "crates/thingd-server/README.md",
  release: "docs/release.md",
};

const source = await readFile("packages/thingd/src/mcp/tools.ts", "utf8");
const rust = await readFile("crates/thingd-server/src/mcp.rs", "utf8");
const server = await readFile("crates/thingd-server/src/server.rs", "utf8");
const metadata = JSON.parse(await readFile("docs/.generated/mcp-metadata.json", "utf8"));
const docs = Object.fromEntries(
  await Promise.all(
    Object.entries(files).map(async ([key, path]) => [key, await readFile(path, "utf8")])
  )
);

const tsTools = [...source.matchAll(/registerTool\(\s*["'](thing_[a-z_]+)["']/g)].map((m) => m[1]);
const rustTools = [...rust.matchAll(/name:\s*"(thing_[a-z_]+)"/g)].map((m) => m[1]);
const unique = (values) => [...new Set(values)].sort();
const errors = [];

if (unique(tsTools).length !== 49) {
  errors.push(`TypeScript MCP registry has ${unique(tsTools).length} tools; update this check if intentional.`);
}
if (unique(rustTools).length !== 39) {
  errors.push(`Rust MCP registry has ${unique(rustTools).length} core tools; update this check if intentional.`);
}
if (metadata.sdkToolCount !== unique(tsTools).length || metadata.sidecarToolCount !== unique(rustTools).length) {
  errors.push("Generated MCP metadata is stale; run pnpm docs:metadata.");
}
if (!docs.mcp.includes("49 SDK tools") || !docs.mcp.includes("39 core tools")) {
  errors.push("MCP API reference is missing the SDK/sidecar tool-count distinction.");
}
if (docs.release.includes('version = "0.41"')) {
  errors.push("Release documentation contains the stale 0.41 crate example.");
}
if (docs.readme.includes("### What's next\n\n- In-process vector search")) {
  errors.push("README still lists shipped vector/WASM/cluster features as future work.");
}
if (docs.release.includes("v0.19.0")) {
  errors.push("Release documentation contains the stale v0.19.0 image example.");
}
const normalizeRoute = (value) => value.replace(/:([a-zA-Z_]+)/g, "{$1}");
const restRoutes = [...server.matchAll(/\.route\("(\/v1\/[^"?]+)"/g)].map((match) => normalizeRoute(match[1]));
const restDocs = normalizeRoute(
  docs.apiIndex + docs.mcp + (await readFile("docs/api-spec/rest-api.md", "utf8"))
);
for (const route of [...new Set(restRoutes)]) {
  if (!restDocs.includes(route)) {
    errors.push(`REST route ${route} is missing from the API documentation.`);
  }
}

async function markdownFilesUnder(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const found = [];
  for (const entry of entries) {
    const relative = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (![".vitepress", "node_modules", "dist", ".cache"].includes(entry.name)) {
        found.push(...(await markdownFilesUnder(relative)));
      }
    } else if (entry.isFile() && /\.mdx?$/.test(entry.name)) {
      found.push(relative);
    }
  }
  return found;
}

const allMarkdown = ["README.md", ...(await markdownFilesUnder("docs"))];
const routeExists = async (candidate, allowDirectory = false) => {
  const choices = [candidate, `${candidate}.md`, `${candidate}.mdx`, path.join(candidate, "index.md")];
  for (const choice of choices) {
    try {
      const details = await stat(choice);
      if (details.isFile() || (allowDirectory && details.isDirectory() && choice === candidate)) {
        return true;
      }
    } catch {
      continue;
    }
  }
  return false;
};

for (const file of allMarkdown) {
  const raw = await readFile(file, "utf8");
  const text = raw.replace(/^\s*(```|~~~)[^\n]*\n[\s\S]*?^\s*\1\s*$/gm, "");
  for (const match of text.matchAll(/!?\[[^\]]*\]\((<[^>]+>|[^)]+)\)/g)) {
    const destination = match[1].replace(/^<|>$/g, "").trim().split(/\s+/, 1)[0];
    if (!destination || /^(?:[a-z][a-z\d+.-]*:|\/\/)/i.test(destination)) {
      continue;
    }
    const pathname = decodeURIComponent(destination.split(/[?#]/, 1)[0]);
    const target = pathname.startsWith("/")
      ? path.resolve("docs", `.${pathname}`)
      : path.resolve(path.dirname(file), pathname || path.basename(file));
    if (!(await routeExists(target, true))) {
      errors.push(`${file} contains a broken local link: ${destination}`);
    }
  }
}

const vitepressConfig = await readFile("docs/.vitepress/config.ts", "utf8");
for (const match of vitepressConfig.matchAll(/\blink\s*:\s*["']([^"']+)["']/g)) {
  const route = match[1];
  if (/^(?:[a-z][a-z\d+.-]*:|\/\/)/i.test(route)) {
    continue;
  }
  const pathname = decodeURIComponent(route.split(/[?#]/, 1)[0]);
  const target = path.resolve("docs", `.${pathname.startsWith("/") ? pathname : `/${pathname}`}`);
  if (!(await routeExists(target))) {
    errors.push(`VitePress navigation contains a broken local route: ${route}`);
  }
}
for (const [path, text] of Object.entries(docs)) {
  if (text.includes("QUICKSTART.md")) {
    errors.push(`${path} contains a broken uppercase QUICKSTART.md link.`);
  }
}

if (errors.length) {
  console.error(errors.map((error) => `- ${error}`).join("\n"));
  process.exit(1);
}

console.log(`Documentation checks passed: ${unique(tsTools).length} SDK tools, ${unique(rustTools).length} sidecar tools.`);
