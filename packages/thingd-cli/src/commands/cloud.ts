import { execSync } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";
import pc from "picocolors";
import { type CliContext, requiredToken, stringFlag } from "../index.js";
import {
  CloudApiError,
  createApiKey,
  createInstance,
  createProject,
  getMe,
  listInstances,
  listProjects,
  pollCliAuth,
  startCliAuth,
} from "../lib/cloud-api.js";
import {
  type CloudConfig,
  readCloudConfig,
  removeCloudConfig,
  writeCloudConfig,
} from "../lib/cloud-config.js";

const POLL_INTERVAL_MS = 2_000;
const POLL_TIMEOUT_MS = 5 * 60 * 1_000;

function cliApiUrl(context: CliContext): string {
  return stringFlag(context.parsed, "url") ?? process.env.THINGD_URL ?? "https://api.thingd.cloud";
}

function buildWebUrl(apiUrl: string): string {
  if (apiUrl.includes("//api.") || apiUrl.startsWith("api.")) {
    return apiUrl.replace("api.", "");
  }
  if (apiUrl.includes("localhost:8787") || apiUrl.includes("127.0.0.1:8787")) {
    return "http://localhost:5173";
  }
  return apiUrl;
}

function openBrowser(url: string): void {
  try {
    const platform = process.platform;
    if (platform === "darwin") {
      execSync(`open "${url}"`, { timeout: 5_000 });
    } else if (platform === "win32") {
      execSync(`start "" "${url}"`, { timeout: 5_000 });
    } else {
      execSync(`xdg-open "${url}"`, { timeout: 5_000 });
    }
  } catch {
    // browser open is best-effort
  }
}

function makeBaseConfig(context: CliContext): CloudConfig {
  return { token: "", url: cliApiUrl(context) };
}

export async function runCloud(context: CliContext): Promise<void> {
  const sub = requiredToken(context.parsed, 1, "subcommand");

  switch (sub) {
    case "login":
      await runLogin(context);
      return;
    case "logout":
      await runLogout(context);
      return;
    case "status":
      await runCloudStatus(context);
      return;
    case "project":
      await runProject(context);
      return;
    case "instance":
      await runInstance(context);
      return;
    case "api-key":
      await runApiKey(context);
      return;
    default:
      context.stderr.write(
        `Unknown cloud subcommand: ${sub}\n` +
          "Available: login, logout, status, project, instance, api-key\n"
      );
  }
}

async function runLogin(context: CliContext): Promise<void> {
  const code = context.parsed.tokens[2] ?? stringFlag(context.parsed, "code");
  const token = context.parsed.tokens[3] ?? stringFlag(context.parsed, "token");

  // ── Manual --code --token flow (fallback) ─────────────────────────
  if (code && token) {
    const config: CloudConfig = { token, url: cliApiUrl(context) };
    try {
      const { user } = await getMe(config);
      writeCloudConfig({ ...config, email: user.email });
      context.stdout.write(pc.green(`✓ Logged in as ${user.email}\n`));
    } catch (err) {
      if (err instanceof CloudApiError && err.status === 401) {
        context.stderr.write(pc.red("Invalid token. Please try again.\n"));
      } else {
        context.stderr.write(pc.red(`Failed to verify token: ${err}\n`));
      }
    }
    return;
  }

  // ── Auto device-code flow ─────────────────────────────────────────
  const baseConfig = makeBaseConfig(context);
  let deviceCode: string;

  try {
    const result = await startCliAuth(baseConfig);
    deviceCode = result.code;
  } catch (err) {
    context.stderr.write(pc.red(`Failed to start CLI auth: ${err}\n`));
    return;
  }

  const webUrl = buildWebUrl(cliApiUrl(context));
  const authUrl = `${webUrl}/cli/auth?code=${deviceCode}`;

  context.stdout.write(
    `\n  ${pc.cyan("Opening browser...")}\n` +
    `  ${pc.dim("If the browser doesn't open, visit:")}\n` +
    `  ${pc.dim(authUrl)}\n\n`
  );

  openBrowser(authUrl);

  // Poll for token
  const deadline = Date.now() + POLL_TIMEOUT_MS;
  let dots = 0;

  while (Date.now() < deadline) {
    await sleep(POLL_INTERVAL_MS);

    try {
      const result = await pollCliAuth(baseConfig, deviceCode);

      if ("token" in result) {
        const tokenConfig: CloudConfig = { token: result.token, url: cliApiUrl(context) };
        try {
          const { user } = await getMe(tokenConfig);
          writeCloudConfig({ ...tokenConfig, email: user.email });
          context.stdout.write(pc.green(`\r✓ Logged in as ${user.email}\n`));
          return;
        } catch {
          context.stderr.write(pc.red("\rToken received but verification failed. Try again.\n"));
          return;
        }
      }

      // Show spinner
      dots = (dots + 1) % 4;
      context.stdout.write(`\r  ${pc.dim("Waiting for browser login" + ".".repeat(dots) + " ".repeat(3 - dots))}`);
    } catch (err) {
      if (err instanceof CloudApiError && err.status === 410) {
        context.stderr.write(pc.red("\nCode expired. Run `thingd cloud login` again.\n"));
        return;
      }
    }
  }

  context.stderr.write(pc.red("\nTimed out. Run `thingd cloud login` again.\n"));
}

async function runLogout(context: CliContext): Promise<void> {
  removeCloudConfig();
  context.stdout.write(pc.green("✓ Logged out\n"));
}

async function runCloudStatus(context: CliContext): Promise<void> {
  const config = readCloudConfig();
  if (!config) {
    context.stdout.write(`Not logged in. Run ${pc.cyan("thingd cloud login")}\n`);
    return;
  }

  try {
    const { user } = await getMe(config);
    context.stdout.write(
      `Logged in as ${pc.green(user.email)} (${user.role})\n` +
        `API: ${config.url ?? "https://api.thingd.cloud"}\n`
    );
  } catch {
    context.stdout.write(`Token expired. Run ${pc.cyan("thingd cloud login")}\n`);
  }
}

async function requireConfig(context: CliContext): Promise<CloudConfig> {
  const config = readCloudConfig();
  if (!config) {
    context.stderr.write(`Not logged in. Run ${pc.cyan("thingd cloud login")} first.\n`);
    throw new Error("not_logged_in");
  }
  return config;
}

async function runProject(context: CliContext): Promise<void> {
  const config = await requireConfig(context);
  const action = requiredToken(context.parsed, 2, "action");

  if (action === "list") {
    const { projects } = await listProjects(config);
    if (projects.length === 0) {
      context.stdout.write("No projects found.\n");
      return;
    }
    for (const p of projects) {
      context.stdout.write(`${pc.cyan(p.slug)}  ${p.name}  ${pc.dim(p.createdAt.slice(0, 10))}\n`);
    }
    return;
  }

  if (action === "create") {
    const name = requiredToken(context.parsed, 3, "name");
    const { project } = await createProject(config, name);
    context.stdout.write(pc.green(`✓ Created project: ${project.slug}\n`));
    return;
  }

  context.stderr.write(`Unknown project action: ${action}. Available: list, create\n`);
}

async function runInstance(context: CliContext): Promise<void> {
  const config = await requireConfig(context);
  const action = requiredToken(context.parsed, 2, "action");

  if (action === "list") {
    const projectSlug = requiredToken(context.parsed, 3, "project");
    const { projects } = await listProjects(config);
    const project = projects.find((p) => p.slug === projectSlug || p.id === projectSlug);
    if (!project) {
      context.stderr.write(pc.red(`Project not found: ${projectSlug}\n`));
      return;
    }
    const { instances } = await listInstances(config, project.id);
    if (instances.length === 0) {
      context.stdout.write("No instances found.\n");
      return;
    }
    for (const inst of instances) {
      context.stdout.write(
        `${pc.cyan(inst.slug)}  ${inst.name}  ${pc.dim(inst.mcpUrl || "no URL")}\n`
      );
    }
    return;
  }

  if (action === "create") {
    const projectSlug = requiredToken(context.parsed, 3, "project");
    const name = requiredToken(context.parsed, 4, "name");
    const { projects } = await listProjects(config);
    const project = projects.find((p) => p.slug === projectSlug || p.id === projectSlug);
    if (!project) {
      context.stderr.write(pc.red(`Project not found: ${projectSlug}\n`));
      return;
    }
    const { instance } = await createInstance(config, project.id, name);
    context.stdout.write(pc.green(`✓ Created instance: ${instance.slug}\n`));
    return;
  }

  context.stderr.write(`Unknown instance action: ${action}. Available: list, create\n`);
}

async function runApiKey(context: CliContext): Promise<void> {
  const config = await requireConfig(context);
  const action = requiredToken(context.parsed, 2, "action");

  if (action === "create") {
    const projectSlug = requiredToken(context.parsed, 3, "project");
    const name = context.parsed.tokens[4];
    const { projects } = await listProjects(config);
    const project = projects.find((p) => p.slug === projectSlug || p.id === projectSlug);
    if (!project) {
      context.stderr.write(pc.red(`Project not found: ${projectSlug}\n`));
      return;
    }
    const { key } = await createApiKey(config, project.id, name);
    context.stdout.write(
      pc.green(`✓ Created API key: ${key.prefix}\n`) +
        `${pc.yellow("Save this token — it won't be shown again:")}\n` +
        `${pc.bold(key.token ?? "(token not returned)")}\n`
    );
    return;
  }

  context.stderr.write(`Unknown api-key action: ${action}. Available: create\n`);
}
