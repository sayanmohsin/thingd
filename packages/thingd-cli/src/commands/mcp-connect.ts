import { createInterface } from "node:readline/promises";
import pc from "picocolors";
import type { CliContext } from "../index.js";
import { listInstances, listProjects } from "../lib/cloud-api.js";
import { readCloudConfig } from "../lib/cloud-config.js";
import {
  type McpServerConfig,
  printMcpConfigJson,
  updateAntigravityConfig,
  updateClaudeDesktopConfig,
} from "../lib/mcp-config-writer.js";

async function askQuestion(query: string): Promise<string> {
  const rl = createInterface({
    input: process.stdin,
    output: process.stderr,
  });
  try {
    return await rl.question(query);
  } finally {
    rl.close();
  }
}

export async function runMcpConnect(context: CliContext): Promise<void> {
  const config = readCloudConfig();
  if (!config?.token || !config.url) {
    context.stderr.write(
      `${pc.red("Not logged in.")} Run ${pc.cyan("thingd cloud login")} first.\n`
    );
    return;
  }

  // ── Fetch projects ────────────────────────────────────────────────
  let projects: { id: string; slug: string; name: string }[];
  try {
    const result = await listProjects(config);
    projects = result.projects;
  } catch (err) {
    context.stderr.write(pc.red(`Failed to fetch projects: ${err}\n`));
    return;
  }

  if (projects.length === 0) {
    context.stderr.write(
      `${pc.red("No projects found.")} Create one with ${pc.cyan("thingd cloud project create <name>")}.\n`
    );
    return;
  }

  context.stderr.write(`\n${pc.bold("Select a project")}\n`);
  for (let i = 0; i < projects.length; i++) {
    const p = projects[i];
    if (p) {
      context.stderr.write(`  [${i + 1}] ${pc.cyan(p.slug)}  ${p.name}\n`);
    }
  }
  const projectChoice = await askQuestion(`Select project [1-${projects.length}] (default 1): `);
  const projectIndex = Math.max(
    0,
    Math.min(projects.length - 1, (Number(projectChoice.trim()) || 1) - 1)
  );
  const selectedProject = projects[projectIndex];
  if (!selectedProject) {
    return;
  }
  context.stderr.write("\n");

  // ── Fetch instances ───────────────────────────────────────────────
  let instances: { id: string; slug: string; name: string; mcpUrl: string }[];
  try {
    const result = await listInstances(config, selectedProject.id);
    instances = result.instances;
  } catch (err) {
    context.stderr.write(pc.red(`Failed to fetch instances: ${err}\n`));
    return;
  }

  if (instances.length === 0) {
    context.stderr.write(
      `${pc.red("No instances found for project.")} Create one with ${pc.cyan(`thingd cloud instance create ${selectedProject.slug} <name>`)}.\n`
    );
    return;
  }

  context.stderr.write(`${pc.bold("Select an instance")}\n`);
  for (let i = 0; i < instances.length; i++) {
    const inst = instances[i];
    if (inst) {
      context.stderr.write(
        `  [${i + 1}] ${pc.cyan(inst.slug)}  ${inst.name}  ${pc.dim(inst.mcpUrl || "no URL")}\n`
      );
    }
  }
  const instanceChoice = await askQuestion(`Select instance [1-${instances.length}] (default 1): `);
  const instanceIndex = Math.max(
    0,
    Math.min(instances.length - 1, (Number(instanceChoice.trim()) || 1) - 1)
  );
  const selectedInstance = instances[instanceIndex];
  if (!selectedInstance) {
    return;
  }
  context.stderr.write("\n");

  // ── Pre-fill URL and token ────────────────────────────────────────
  let mcpUrl =
    selectedInstance.mcpUrl || `${config.url}/mcp/${selectedProject.slug}/${selectedInstance.slug}`;
  let authToken = config.token;

  context.stderr.write(`${pc.bold("MCP Connection Details")}\n`);
  const urlAnswer = await askQuestion(`MCP URL (default ${pc.cyan(mcpUrl)}): `);
  if (urlAnswer.trim()) {
    mcpUrl = urlAnswer.trim();
  }

  const tokenAnswer = await askQuestion(
    `Auth Token (default ${pc.cyan(`${authToken.slice(0, 8)}...`)}): `
  );
  if (tokenAnswer.trim()) {
    authToken = tokenAnswer.trim();
  }
  context.stderr.write("\n");

  // ── Build config ──────────────────────────────────────────────────
  const mcpConfig: McpServerConfig = {
    url: mcpUrl,
    headers: {
      Authorization: `Bearer ${authToken}`,
    },
  };

  // ── Preview ───────────────────────────────────────────────────────
  context.stderr.write(`${pc.bold("Generated Config:")}\n`);
  context.stderr.write(`  ${pc.dim(printMcpConfigJson(mcpConfig).replace(/\n/g, "\n  "))}\n\n`);

  // ── Where to install ──────────────────────────────────────────────
  context.stderr.write(`${pc.bold("Where would you like to write the MCP configuration?")}\n`);
  context.stderr.write(`  [1] Claude Desktop & Antigravity (Default)\n`);
  context.stderr.write(`  [2] Claude Desktop only\n`);
  context.stderr.write(`  [3] Antigravity only\n`);
  context.stderr.write(`  [4] Print raw JSON only\n`);
  context.stderr.write(`  [5] Print Cursor-compatible JSON\n\n`);

  const choice = (await askQuestion("Select option [1-5] (default 1): ")).trim() || "1";

  const showClaude = choice === "1" || choice === "2";
  const showAntigravity = choice === "1" || choice === "3";
  const showRaw = choice === "4";
  const showCursor = choice === "5";

  if (showClaude) {
    const result = updateClaudeDesktopConfig(mcpConfig);
    if (result.updated) {
      context.stderr.write(`  ${pc.bold("Claude Desktop:")}\n`);
      context.stderr.write(`    ${pc.green("✓")} Updated ${pc.cyan(result.path)}\n\n`);
    } else if (result.skipped) {
      context.stderr.write(`  ${pc.bold("Claude Desktop:")}\n`);
      context.stderr.write(`    ${pc.yellow("⊘")} Skipped: ${result.reason}\n\n`);
    }
  }

  if (showAntigravity) {
    const result = updateAntigravityConfig(mcpConfig);
    if (result.updated) {
      context.stderr.write(`  ${pc.bold("Antigravity IDE:")}\n`);
      context.stderr.write(`    ${pc.green("✓")} Updated ${pc.cyan(result.path)}\n\n`);
    } else if (result.skipped) {
      context.stderr.write(`  ${pc.bold("Antigravity IDE:")}\n`);
      context.stderr.write(`    ${pc.yellow("⊘")} Skipped: ${result.reason}\n\n`);
    }
  }

  if (showCursor) {
    context.stderr.write(`  ${pc.bold("Cursor:")}\n`);
    context.stderr.write(
      `    Paste this into Cursor Settings → Features → MCP → Add New MCP Server:\n\n`
    );
    context.stdout.write(`${printMcpConfigJson(mcpConfig)}\n`);
  }

  if (showRaw) {
    context.stdout.write(`${printMcpConfigJson(mcpConfig)}\n`);
  }

  if (showClaude) {
    context.stderr.write(`\n  Restart Claude Desktop or Antigravity to activate.\n\n`);
  } else if (showCursor) {
    context.stderr.write(`\n  Cursor activates immediately after pasting.\n\n`);
  } else if (showRaw) {
    context.stderr.write("\n");
  }
}
