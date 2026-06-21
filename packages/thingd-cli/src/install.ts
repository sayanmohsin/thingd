import { existsSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { homedir, platform } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline/promises";
import { fileURLToPath } from "node:url";
import { NativeThingStore } from "@thingd/sdk";
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
  const isAntigravity =
    context.parsed.booleans.has("antigravity") || context.parsed.flags.has("antigravity");

  if (!isRaw) {
    context.stderr.write(`\n${pc.bold("thingd install")}\n\n`);
  }

  let choice = "1";
  let dbPath = dbPathDefault;
  let driver = driverDefault;

  if (isRaw) {
    choice = "5";
  } else if (isClaude && isCursor && isAntigravity) {
    choice = "1";
  } else if (isClaude) {
    choice = "2";
  } else if (isCursor) {
    choice = "3";
  } else if (isAntigravity) {
    choice = "4";
  } else if (process.stdin.isTTY) {
    // 1. Where to install
    context.stderr.write(`${pc.bold("Where would you like to install the MCP configuration?")}\n`);
    context.stderr.write(`  [1] Claude Desktop, Cursor & Antigravity (Default)\n`);
    context.stderr.write(`  [2] Claude Desktop only\n`);
    context.stderr.write(`  [3] Cursor only\n`);
    context.stderr.write(`  [4] Antigravity only\n`);
    context.stderr.write(`  [5] Print raw JSON configuration only\n\n`);

    const answerInstall = await askQuestion(`Select option [1-5] (default 1): `);
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
      `Driver [native / memory] (default ${pc.cyan(driverDefault)}): `
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
    const hasNative = await NativeThingStore.isAvailable();
    const bindingStatus = hasNative
      ? pc.green("Available/Loaded")
      : pc.red("Unavailable/Not Found");

    context.stderr.write(`  ${pc.bold("Configuration Details:")}\n`);
    context.stderr.write(`    ${pc.green("✓")} Database path:  ${pc.cyan(dbPath)}\n`);
    context.stderr.write(`    ${pc.green("✓")} Driver:         ${pc.cyan(driver)}\n`);
    context.stderr.write(
      `    ${hasNative ? pc.green("✓") : pc.yellow("⚠")} Native Addon:  ${bindingStatus}\n`
    );
    if (globalBin) {
      context.stderr.write(`    ${pc.green("✓")} Command:        ${pc.cyan(globalBin)}\n\n`);
    } else {
      context.stderr.write(`    ${pc.green("✓")} Node:           ${pc.cyan(nodePath)}\n`);
      context.stderr.write(`    ${pc.green("✓")} CLI:            ${pc.cyan(cliPath)}\n\n`);
    }
  }

  const showClaude = choice === "1" || choice === "2";
  const showCursor = choice === "1" || choice === "3";
  const showAntigravity = choice === "1" || choice === "4";
  const showRaw = choice === "5";

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

  if (showAntigravity) {
    const antigravityResult = updateAntigravityConfig(config);
    if (antigravityResult.updated) {
      context.stderr.write(`  ${pc.bold("Antigravity IDE:")}\n`);
      context.stderr.write(`    ${pc.green("✓")} Updated ${pc.cyan(antigravityResult.path)}\n\n`);
    } else if (antigravityResult.skipped) {
      context.stderr.write(`  ${pc.bold("Antigravity IDE:")}\n`);
      context.stderr.write(`    ${pc.yellow("⊘")} Skipped: ${antigravityResult.reason}\n\n`);
    }
  }

  if (showCursor) {
    context.stderr.write(`  ${pc.bold("Cursor:")}\n`);
    context.stderr.write(
      `    Paste this into Cursor Settings → Features → MCP → Add New MCP Server:\n\n`
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
      `\n  Restart Claude Desktop or Antigravity to activate. Cursor activates immediately.\n\n`
    );
  } else if (choice === "2") {
    context.stderr.write(`\n  Restart Claude Desktop to activate.\n\n`);
  } else if (choice === "3") {
    context.stderr.write(`\n  Cursor activates immediately after pasting.\n\n`);
  } else if (choice === "4") {
    context.stderr.write(`\n  Restart Antigravity IDE to activate.\n\n`);
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
  try {
    const currentFile = fileURLToPath(import.meta.url);
    const dir = dirname(currentFile);

    // In compiled dist folder: dist/install.js -> dist/index.js
    let candidate = join(dir, "index.js");
    if (existsSync(candidate)) {
      return realpathSync(candidate);
    }

    // In dev src folder: src/install.ts -> src/index.ts
    candidate = join(dir, "index.ts");
    if (existsSync(candidate)) {
      return realpathSync(candidate);
    }
  } catch {
    // Ignore and fallback
  }

  const scriptPath = process.argv[1];
  if (!scriptPath) {
    throw new Error("Could not detect thingd CLI path.");
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
  driver: string
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
    "claude_desktop_config.json"
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

function updateAntigravityConfig(config: McpServerConfig): ClaudeUpdateResult {
  const candidates = [
    join(homedir(), ".gemini", "config", "mcp_config.json"),
    join(homedir(), ".gemini", "antigravity-ide", "mcp_config.json"),
  ];

  let updatedAny = false;
  const pathsUpdated: string[] = [];
  let lastError: Error | null = null;

  for (const configPath of candidates) {
    const dir = dirname(configPath);
    if (existsSync(dir)) {
      try {
        let existing: Record<string, unknown> = {};
        if (existsSync(configPath)) {
          const raw = readFileSync(configPath, "utf-8").trim();
          if (raw) {
            existing = JSON.parse(raw) as Record<string, unknown>;
          }
        }

        const mcpServers = (existing.mcpServers ?? {}) as Record<string, unknown>;
        mcpServers.thingd = config;
        existing.mcpServers = mcpServers;

        writeFileSync(configPath, `${JSON.stringify(existing, null, 2)}\n`, "utf-8");
        updatedAny = true;
        pathsUpdated.push(configPath);
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));
      }
    }
  }

  if (updatedAny) {
    return { updated: true, path: pathsUpdated.join(" & ") };
  }

  if (lastError) {
    return {
      skipped: true,
      reason: `Failed to update config: ${lastError.message}`,
    };
  }

  return {
    skipped: true,
    reason: `Antigravity directory not found in ${candidates.map((c) => dirname(c)).join(" or ")}.`,
  };
}
