import { execSync } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";
import pc from "picocolors";
import { type CliContext, requiredToken, stringFlag } from "../index.js";
import {
  addOrganizationMember,
  CloudApiError,
  createApiKey,
  createInstance,
  createOrganization,
  createProject,
  getMe,
  getOrganization,
  listInstances,
  listOrganizationMembers,
  listOrganizations,
  listProjects,
  pollCliAuth,
  removeOrganizationMember,
  resolveFirstInstance,
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
    case "org":
      await runOrg(context);
      return;
    default:
      context.stderr.write(
        `Unknown cloud subcommand: ${sub}\n` +
          "Available: login, logout, status, org, project, instance, api-key\n"
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
      const cloudConfig: CloudConfig = { ...config, email: user.email };
      // Auto-discover first instance
      const instance = await resolveFirstInstance(cloudConfig);
      if (instance) {
        cloudConfig.instanceUrl = instance.mcpUrl;
        cloudConfig.projectSlug = instance.projectSlug;
        cloudConfig.instanceSlug = instance.instanceSlug;
      }
      writeCloudConfig(cloudConfig);
      context.stdout.write(pc.green(`✓ Logged in as ${user.email}\n`));
      if (instance) {
        context.stdout.write(
          `  Instance: ${pc.cyan(instance.projectSlug)}/${pc.cyan(instance.instanceSlug)}\n`
        );
      }
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
          const cloudConfig: CloudConfig = { ...tokenConfig, email: user.email };
          // Auto-discover first instance
          const instance = await resolveFirstInstance(cloudConfig);
          if (instance) {
            cloudConfig.instanceUrl = instance.mcpUrl;
            cloudConfig.projectSlug = instance.projectSlug;
            cloudConfig.instanceSlug = instance.instanceSlug;
          }
          writeCloudConfig(cloudConfig);
          context.stdout.write(pc.green(`\r✓ Logged in as ${user.email}\n`));
          if (instance) {
            context.stdout.write(
              `  Instance: ${pc.cyan(instance.projectSlug)}/${pc.cyan(instance.instanceSlug)}\n`
            );
          }
          return;
        } catch {
          context.stderr.write(pc.red("\rToken received but verification failed. Try again.\n"));
          return;
        }
      }

      // Show spinner
      dots = (dots + 1) % 4;
      context.stdout.write(
        `\r  ${pc.dim(`Waiting for browser login${".".repeat(dots)}${" ".repeat(3 - dots)}`)}`
      );
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
    if (config.projectSlug && config.instanceSlug && config.instanceUrl) {
      context.stdout.write(
        `Instance: ${pc.cyan(config.projectSlug)}/${pc.cyan(config.instanceSlug)}\n` +
          `  ${pc.dim(config.instanceUrl)}\n`
      );
    } else {
      context.stdout.write(
        `Instance: ${pc.dim("none — set with thingd cloud instance use <project> <instance>")}\n`
      );
    }
    if (config.organizationId) {
      try {
        const { organization, role } = await getOrganization(config, config.organizationId);
        context.stdout.write(
          `Org: ${pc.cyan(organization.name)} (${pc.dim(organization.slug)}) — ${role}\n`
        );
      } catch {
        context.stdout.write(`Org: ${pc.dim("unreachable")}\n`);
      }
    } else {
      context.stdout.write(`Org: ${pc.dim("none — set with thingd cloud org use <slug>")}\n`);
    }
  } catch {
    context.stdout.write(`Token expired. Run ${pc.cyan("thingd cloud login")}\n`);
  }
}

// ── Org helpers ──────────────────────────────────────────────────────

async function resolveOrg(
  config: CloudConfig,
  slugOrId: string
): Promise<{ id: string; name: string; slug: string }> {
  // Try direct ID lookup first, then assume it's a slug and list
  try {
    const { organization } = await getOrganization(config, slugOrId);
    return organization;
  } catch {
    // Fall through to slug resolution
  }

  const { organizations } = await listOrganizations(config);
  const found = organizations.find((o) => o.slug === slugOrId);
  if (!found) {
    throw new Error(`Organization not found: ${slugOrId}`);
  }
  return found;
}

async function runOrg(context: CliContext): Promise<void> {
  const config = await requireConfig(context);
  const action = context.parsed.tokens[2];

  if (!action || action === "list") {
    const { organizations } = await listOrganizations(config);
    if (organizations.length === 0) {
      context.stdout.write("No organizations found.\n");
      return;
    }
    for (const org of organizations) {
      const active = org.id === config.organizationId ? pc.green(" ●") : "";
      context.stdout.write(`${pc.cyan(org.slug)}  ${org.name}${active}\n`);
    }
    return;
  }

  if (action === "create") {
    const name = requiredToken(context.parsed, 3, "name");
    const { organization } = await createOrganization(config, name);
    context.stdout.write(pc.green(`✓ Created organization: ${organization.slug}\n`));
    return;
  }

  if (action === "use") {
    const slugOrId = requiredToken(context.parsed, 3, "organization");
    const org = await resolveOrg(config, slugOrId);
    config.organizationId = org.id;
    writeCloudConfig(config);
    context.stdout.write(pc.green(`✓ Active organization: ${org.slug}\n`));
    return;
  }

  if (action === "members") {
    await runOrgMembers(context, config);
    return;
  }

  context.stderr.write(`Unknown org action: ${action}. Available: list, create, use, members\n`);
}

async function runOrgMembers(context: CliContext, config: CloudConfig): Promise<void> {
  const sub = context.parsed.tokens[3];

  if (!sub || sub === "list") {
    const orgSlug = requiredToken(context.parsed, 4, "organization");
    const org = await resolveOrg(config, orgSlug);
    const { members } = await listOrganizationMembers(config, org.id);
    if (members.length === 0) {
      context.stdout.write("No members found.\n");
      return;
    }
    for (const m of members) {
      context.stdout.write(`${pc.cyan(m.userId)}  ${m.role}\n`);
    }
    return;
  }

  if (sub === "add") {
    const orgSlug = requiredToken(context.parsed, 4, "organization");
    const userId = requiredToken(context.parsed, 5, "user-id");
    const role = stringFlag(context.parsed, "role") ?? "member";
    const org = await resolveOrg(config, orgSlug);
    const { member } = await addOrganizationMember(config, org.id, userId, role);
    context.stdout.write(pc.green(`✓ Added ${member.userId} as ${member.role}\n`));
    return;
  }

  if (sub === "remove") {
    const orgSlug = requiredToken(context.parsed, 4, "organization");
    const userId = requiredToken(context.parsed, 5, "user-id");
    const org = await resolveOrg(config, orgSlug);
    await removeOrganizationMember(config, org.id, userId);
    context.stdout.write(pc.green(`✓ Removed ${userId}\n`));
    return;
  }

  context.stderr.write(`Unknown members action: ${sub}. Available: list, add, remove\n`);
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

    // Optional --org flag for team projects
    let organizationId: string | undefined;
    const orgSlug = stringFlag(context.parsed, "org");
    if (orgSlug) {
      const org = await resolveOrg(config, orgSlug);
      organizationId = org.id;
    }

    const { project } = await createProject(config, name, organizationId);
    const ctxNote = organizationId ? ` (under org ${orgSlug})` : "";
    context.stdout.write(pc.green(`✓ Created project: ${project.slug}${ctxNote}\n`));
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
      const active = inst.mcpUrl && inst.mcpUrl === config.instanceUrl ? pc.green(" ●") : "";
      context.stdout.write(
        `${pc.cyan(inst.slug)}  ${inst.name}  ${pc.dim(inst.mcpUrl || "no URL")}${active}\n`
      );
    }
    return;
  }

  if (action === "use") {
    const projectSlug = requiredToken(context.parsed, 3, "project");
    const instanceSlug = requiredToken(context.parsed, 4, "instance");
    const { projects } = await listProjects(config);
    const project = projects.find((p) => p.slug === projectSlug || p.id === projectSlug);
    if (!project) {
      context.stderr.write(pc.red(`Project not found: ${projectSlug}\n`));
      return;
    }
    const { instances } = await listInstances(config, project.id);
    const instance = instances.find((i) => i.slug === instanceSlug || i.id === instanceSlug);
    if (!instance) {
      context.stderr.write(pc.red(`Instance not found: ${instanceSlug}\n`));
      return;
    }
    if (!instance.mcpUrl) {
      context.stderr.write(pc.red("Instance has no MCP URL. Is it running?\n"));
      return;
    }
    config.instanceUrl = instance.mcpUrl;
    config.projectSlug = project.slug;
    config.instanceSlug = instance.slug;
    writeCloudConfig(config);
    context.stdout.write(
      pc.green(`✓ Active instance: ${pc.cyan(project.slug)}/${pc.cyan(instance.slug)}\n`) +
        `  ${pc.dim(instance.mcpUrl)}\n`
    );
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

  context.stderr.write(`Unknown instance action: ${action}. Available: list, use, create\n`);
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
