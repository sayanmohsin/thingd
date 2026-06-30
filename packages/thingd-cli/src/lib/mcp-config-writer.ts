import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { homedir, platform } from "node:os";
import { dirname, join } from "node:path";

export type McpServerConfig =
  | { command: string; args: string[] }
  | { url: string; headers?: Record<string, string> };

export type McpServersBlock = {
  mcpServers: Record<string, McpServerConfig>;
};

export function printMcpConfigJson(config: McpServerConfig): string {
  const full: McpServersBlock = {
    mcpServers: {
      thingd: config,
    },
  };
  return JSON.stringify(full, null, 2);
}

export type ClaudeUpdateResult =
  | { updated: true; path: string; skipped?: undefined; reason?: undefined }
  | { updated?: undefined; skipped: true; reason: string; path?: undefined };

export function getClaudeConfigPath(): string {
  return join(homedir(), "Library", "Application Support", "Claude", "claude_desktop_config.json");
}

export function updateClaudeDesktopConfig(config: McpServerConfig): ClaudeUpdateResult {
  if (platform() !== "darwin") {
    return { skipped: true, reason: "Claude Desktop auto-config is only supported on macOS." };
  }

  const configPath = getClaudeConfigPath();

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

export function updateAntigravityConfig(config: McpServerConfig): ClaudeUpdateResult {
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
