import { existsSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { homedir, platform } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline/promises";
import pc from "picocolors";
import type { CliContext } from "./index.js";
import { defaultThingdDbPath, ensureThingdDir } from "./paths.js";

type McpServerConfig = {
  command: string;
  args: string[];
};

type McpServersBlock = {
  mcpServers: Record<string, McpServerConfig>;
};

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

export async function runInstall(context: CliContext): Promise<void> {
  const nodePath = process.execPath;
  const cliPath = resolveCliPath();
  const dbPathDefault = defaultThingdDbPath();
  const driverDefault = detectDriver();

  ensureThingdDir();

  const isRaw = context.parsed.booleans.has("raw") || context.parsed.flags.has("raw");
  const isClaude = context.parsed.booleans.has("claude") || context.parsed.flags.has("claude");
  const isCursor = context.parsed.booleans.has("cursor") || context.parsed.flags.has("cursor");

  if (!isRaw) {
    context.stderr.write(`\n${pc.bold("thingd install")}\n\n`);
  }

  let choice = "1";
  let dbPath = dbPathDefault;
  let driver = driverDefault;

  if (isRaw) {
    choice = "4";
  } else if (isClaude && isCursor) {
    choice = "1";
  } else if (isClaude) {
    choice = "2";
  } else if (isCursor) {
    choice = "3";
  } else if (process.stdin.isTTY) {
    // 1. Where to install
    context.stderr.write(`${pc.bold("Where would you like to install the MCP configuration?")}\n`);
    context.stderr.write(`  [1] Claude Desktop & Cursor (Default)\n`);
    context.stderr.write(`  [2] Claude Desktop only\n`);
    context.stderr.write(`  [3] Cursor only\n`);
    context.stderr.write(`  [4] Print raw JSON configuration only\n\n`);

    const answerInstall = await askQuestion(`Select option [1-4] (default 1): `);
    choice = answerInstall.trim() || "1";
    context.stderr.write("\n");

    // 2. Database Path
    const answerPath = await askQuestion(`Database path (default ${pc.cyan(dbPathDefault)}): `);
    const chosenPath = answerPath.trim();
    if (chosenPath) {
      dbPath = chosenPath;
    }

    // 3. Driver
    const answerDriver = await askQuestion(
      `Driver [native / memory] (default ${pc.cyan(driverDefault)}): `,
    );
    const chosenDriver = answerDriver.trim().toLowerCase();
    if (chosenDriver === "native" || chosenDriver === "memory") {
      driver = chosenDriver as "native" | "memory";
    }
    context.stderr.write("\n");
  }

  // Honor command line options if explicitly passed
  const cliPathOption = context.parsed.flags.get("path")?.at(-1);
  const cliDriverOption = context.parsed.flags.get("driver")?.at(-1);
  if (cliPathOption) {
    dbPath = cliPathOption;
  }
  if (cliDriverOption === "native" || cliDriverOption === "memory") {
    driver = cliDriverOption as "native" | "memory";
  }

  const globalBin = findGlobalBinPath();
  const config = globalBin
    ? {
        command: globalBin,
        args: ["mcp", "--path", dbPath, "--driver", driver],
      }
    : generateMcpConfig(nodePath, cliPath, dbPath, driver);

  if (!isRaw) {
    context.stderr.write(`  ${pc.bold("Configuration Details:")}\n`);
    context.stderr.write(`    ${pc.green("✓")} Database path: ${pc.cyan(dbPath)}\n`);
    context.stderr.write(`    ${pc.green("✓")} Driver:        ${pc.cyan(driver)}\n`);
    if (globalBin) {
      context.stderr.write(`    ${pc.green("✓")} Command:       ${pc.cyan(globalBin)}\n\n`);
    } else {
      context.stderr.write(`    ${pc.green("✓")} Node:          ${pc.cyan(nodePath)}\n`);
      context.stderr.write(`    ${pc.green("✓")} CLI:           ${pc.cyan(cliPath)}\n\n`);
    }
  }

  const showClaude = choice === "1" || choice === "2";
  const showCursor = choice === "1" || choice === "3";
  const showRaw = choice === "4";

  if (showClaude) {
    const claudeResult = updateClaudeDesktopConfig(config);
    if (claudeResult.updated) {
      context.stderr.write(`  ${pc.bold("Claude Desktop:")}\n`);
      context.stderr.write(`    ${pc.green("✓")} Updated ${pc.cyan(claudeResult.path)}\n\n`);
    } else if (claudeResult.skipped) {
      context.stderr.write(`  ${pc.bold("Claude Desktop:")}\n`);
      context.stderr.write(`    ${pc.yellow("⊘")} Skipped: ${claudeResult.reason}\n\n`);
    }
  }

  if (showCursor) {
    context.stderr.write(`  ${pc.bold("Cursor:")}\n`);
    context.stderr.write(
      `    Paste this into Cursor Settings → Features → MCP → Add New MCP Server:\n\n`,
    );

    const fullConfig: McpServersBlock = {
      mcpServers: {
        thingd: config,
      },
    };

    context.stdout.write(`${JSON.stringify(fullConfig, null, 2)}\n`);
  }

  if (showRaw) {
    const fullConfig: McpServersBlock = {
      mcpServers: {
        thingd: config,
      },
    };

    context.stdout.write(`${JSON.stringify(fullConfig, null, 2)}\n`);
  }

  if (choice === "1") {
    context.stderr.write(
      `\n  Restart Claude Desktop to activate. Cursor activates immediately after pasting.\n\n`,
    );
  } else if (choice === "2") {
    context.stderr.write(`\n  Restart Claude Desktop to activate.\n\n`);
  } else if (choice === "3") {
    context.stderr.write(`\n  Cursor activates immediately after pasting.\n\n`);
  }
}

function findGlobalBinPath(): string | null {
  try {
    const cliPath = resolveCliPath();
    // In standard npm/nvm/pnpm global installations:
    // CLI is at <prefix>/lib/node_modules/thingd-cli/dist/index.js
    // Binary is at <prefix>/bin/thingd
    const candidate = resolve(cliPath, "../../../../../bin/thingd");
    if (existsSync(candidate)) {
      return candidate;
    }
  } catch {
    // Ignore
  }
  return null;
}

function resolveCliPath(): string {
  const scriptPath = process.argv[1];
  if (!scriptPath) {
    throw new Error("Could not detect thingd CLI path from process.argv[1].");
  }
  try {
    return realpathSync(resolve(scriptPath));
  } catch {
    return resolve(scriptPath);
  }
}

function detectDriver(): "native" | "memory" {
  try {
    // Check if the native .node binary exists relative to the CLI entry point.
    // When installed globally via npm, thingd-native is typically a sibling package.
    const cliDir = join(resolveCliPath(), "..", "..");
    const nativePaths = [
      join(cliDir, "node_modules", "thingd-native", "dist", "thingd_native.node"),
      join(cliDir, "..", "thingd-native", "dist", "thingd_native.node"),
    ];

    for (const candidate of nativePaths) {
      if (existsSync(candidate)) {
        return "native";
      }
    }
  } catch {
    // Ignore detection errors.
  }

  // Default to native since most global installs will have it.
  // If it's not available, the SDK will produce a clear error at open time.
  return "native";
}

function generateMcpConfig(
  nodePath: string,
  cliPath: string,
  dbPath: string,
  driver: string,
): McpServerConfig {
  return {
    command: nodePath,
    args: [cliPath, "mcp", "--path", dbPath, "--driver", driver],
  };
}

type ClaudeUpdateResult =
  | { updated: true; path: string; skipped?: undefined; reason?: undefined }
  | { updated?: undefined; skipped: true; reason: string; path?: undefined };

function updateClaudeDesktopConfig(config: McpServerConfig): ClaudeUpdateResult {
  if (platform() !== "darwin") {
    return { skipped: true, reason: "Claude Desktop auto-config is only supported on macOS." };
  }

  const configPath = join(
    homedir(),
    "Library",
    "Application Support",
    "Claude",
    "claude_desktop_config.json",
  );

  if (!existsSync(configPath)) {
    return {
      skipped: true,
      reason: `Config file not found at ${configPath}. Is Claude Desktop installed?`,
    };
  }

  try {
    const raw = readFileSync(configPath, "utf-8");
    const existing = JSON.parse(raw) as Record<string, unknown>;

    const mcpServers = (existing.mcpServers ?? {}) as Record<string, unknown>;
    mcpServers.thingd = config;
    existing.mcpServers = mcpServers;

    writeFileSync(configPath, `${JSON.stringify(existing, null, 2)}\n`, "utf-8");

    return { updated: true, path: configPath };
  } catch (error) {
    return {
      skipped: true,
      reason: `Failed to update config: ${error instanceof Error ? error.message : String(error)}`,
    };
  }
}
