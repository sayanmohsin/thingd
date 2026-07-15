import { execSync } from "node:child_process";
import * as os from "node:os";
import { createInterface, type Interface } from "node:readline/promises";
import { setTimeout as sleep } from "node:timers/promises";
import pc from "picocolors";
import { type CliContext, requiredToken, stringFlag } from "../index.js";
import {
  addOrganizationMember,
  CloudApiError,
  cleanupSessions,
  createApiKey,
  createInstance,
  createOrganization,
  createProject,
  createUserToken,
  getMe,
  getOrganization,
  listInstances,
  listOrganizationMembers,
  listOrganizations,
  listProjects,
  listUserTokens,
  parseUserTokenId,
  pollCliAuth,
  type ResolvedInstance,
  removeOrganizationMember,
  resolveAllInstances,
  revokeUserToken,
  startCliAuth,
  updateUserToken,
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
  return { url: cliApiUrl(context) };
}

async function askQuestion(rl: Interface, query: string): Promise<string> {
  return rl.question(query);
}

async function pickAndSaveInstance(context: CliContext, cloudConfig: CloudConfig): Promise<void> {
  const instances = await resolveAllInstances(cloudConfig);
  if (instances.length === 0) {
    return;
  }

  const rl = createInterface({
    input: context.stdin as NodeJS.ReadableStream,
    output: context.stderr as NodeJS.WritableStream,
  });

  try {
    let selected: ResolvedInstance;
    if (instances.length === 1) {
      selected = instances[0] as ResolvedInstance;
    } else {
      context.stderr.write(`${pc.bold("Select an instance")}\n`);
      for (let i = 0; i < instances.length; i++) {
        const inst = instances[i] as ResolvedInstance;
        context.stderr.write(
          `  [${i + 1}] ${pc.cyan(inst.projectSlug)}/${pc.cyan(inst.instanceSlug)}\n`
        );
      }
      const choice = await askQuestion(rl, `Select instance [1-${instances.length}] (default 1): `);
      const index = Math.max(0, Math.min(instances.length - 1, (Number(choice.trim()) || 1) - 1));
      selected = instances[index] as ResolvedInstance;
    }

    cloudConfig.instanceUrl = selected.mcpUrl;
    cloudConfig.projectId = selected.projectId;
    cloudConfig.projectSlug = selected.projectSlug;
    cloudConfig.instanceSlug = selected.instanceSlug;
    writeCloudConfig(cloudConfig);
    context.stdout.write(
      `  Instance: ${pc.cyan(selected.projectSlug)}/${pc.cyan(selected.instanceSlug)}\n`
    );
  } finally {
    rl.close();
  }
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
    case "token":
      await runToken(context);
      return;
    case "project":
      await runProject(context);
      return;
    case "instance":
      await runInstance(context);
      return;
    case "api-key":
      context.stderr.write(
        pc.yellow(
          "Deprecated. Use `thingd cloud token create` for CLI tokens. Project API keys are managed in the dashboard.\n"
        )
      );
      await runApiKey(context);
      return;
    case "org":
      await runOrg(context);
      return;
    default:
      context.stderr.write(
        `Unknown cloud subcommand: ${sub}\n` +
          "Available: login, logout, status, token, org, project, instance, api-key\n"
      );
  }
}

async function runLogin(context: CliContext): Promise<void> {
  const code = context.parsed.tokens[2] ?? stringFlag(context.parsed, "code");
  const token = context.parsed.tokens[3] ?? stringFlag(context.parsed, "token");

  // ── Manual --code --token flow (fallback) ─────────────────────────
  if (code && token) {
    const config: CloudConfig = { url: cliApiUrl(context) };
    try {
      // Verify JWT and get user info
      const { user } = await getMe({ ...config, token });
      // Create a permanent user token
      const hostname = os.hostname();
      const { token: userToken } = await createUserToken({ ...config, token }, `cli-${hostname}`);
      // Save config with user token (not JWT)
      const cloudConfig: CloudConfig = { userToken, email: user.email, ...config };
      writeCloudConfig(cloudConfig);
      context.stdout.write(pc.green(`✓ Logged in as ${user.email}\n`));
      // Discover and select an instance (interactive if multiple)
      await pickAndSaveInstance(context, cloudConfig);
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
        const jwt = result.token;
        const tokenConfig: CloudConfig = { url: cliApiUrl(context) };
        try {
          // Verify temporary JWT
          const { user } = await getMe({ ...tokenConfig, token: jwt });
          // Create permanent user token
          const hostname = os.hostname();
          const { token: userToken } = await createUserToken(
            { ...tokenConfig, token: jwt },
            `cli-${hostname}`
          );
          // Save config with user token (not JWT)
          const cloudConfig: CloudConfig = { userToken, email: user.email, ...tokenConfig };
          writeCloudConfig(cloudConfig);
          context.stdout.write(pc.green(`\r✓ Logged in as ${user.email}\n`));
          // Discover and select an instance (interactive if multiple)
          await pickAndSaveInstance(context, cloudConfig);
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
  const config = readCloudConfig();
  if (config?.userToken) {
    try {
      const tokenId = parseUserTokenId(config.userToken);
      if (tokenId) {
        await revokeUserToken(config, tokenId);
      }
    } catch {
      // Best-effort — token may already be revoked or API unreachable
    }
  }
  removeCloudConfig();
  context.stdout.write(pc.green("✓ Logged out\n"));
}

async function runCloudStatus(context: CliContext): Promise<void> {
  const config = readCloudConfig();
  if (!config) {
    context.stdout.write(`Not logged in. Run ${pc.cyan("thingd cloud login")}\n`);
    return;
  }

  // Warn about old config format
  if (config.token && !config.userToken) {
    context.stdout.write(
      `${pc.yellow("Your credentials use an older format.")} Run ${pc.cyan("thingd cloud login")} to upgrade to a persistent CLI token.\n\n`
    );
  }

  try {
    const { user } = await getMe(config);
    context.stdout.write(`Logged in as ${pc.green(user.email)} (${user.role})\n`);

    // Show token info
    if (config.userToken) {
      try {
        const { userTokens } = await listUserTokens(config);
        const active = userTokens.find((t) => !t.revokedAt);
        if (active) {
          context.stdout.write(
            `CLI Token: ${pc.cyan(active.name)}\n` +
              `  Prefix:   ${pc.dim(active.prefix)}\n` +
              `  Created:  ${pc.dim(formatTimeAgo(active.createdAt))}\n` +
              `  Last used:${pc.dim(active.lastUsedAt ? formatTimeAgo(active.lastUsedAt) : "never")}\n` +
              `  Access:   ${pc.dim(active.projectAccess === "all" ? "All projects" : active.projectAccess)}\n`
          );
        }
      } catch {
        context.stdout.write(`  ${pc.dim("(token info unavailable)")}\n`);
      }
    }

    context.stdout.write(`API: ${config.url ?? "https://api.thingd.cloud"}\n`);
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

function formatTimeAgo(iso: string): string {
  const ms = Date.now() - new Date(iso).getTime();
  const s = Math.floor(ms / 1000);
  if (s < 5) {
    return "just now";
  }
  if (s < 60) {
    return `${s}s ago`;
  }
  const m = Math.floor(s / 60);
  if (m < 60) {
    return `${m}m ago`;
  }
  const h = Math.floor(m / 60);
  if (h < 24) {
    return `${h}h ago`;
  }
  const d = Math.floor(h / 24);
  return `${d}d ago`;
}

// ── Token subcommands ────────────────────────────────────────────────

function requireLoggedInConfig(context: CliContext): CloudConfig {
  const config = readCloudConfig();
  if (!config?.userToken && !config?.token) {
    context.stderr.write(pc.yellow("Not logged in. Run thingd cloud login first.\n"));
    throw new Error("not_logged_in");
  }
  return config;
}

async function runToken(context: CliContext): Promise<void> {
  const sub = context.parsed.tokens[2];
  if (!sub) {
    context.stderr.write("Usage: thingd cloud token <list|create|revoke|restrict|unrestricted>\n");
    return;
  }

  switch (sub) {
    case "list":
      await runTokenList(context);
      return;
    case "create":
      await runTokenCreate(context);
      return;
    case "revoke":
      await runTokenRevoke(context);
      return;
    case "restrict":
      await runTokenRestrict(context);
      return;
    case "unrestricted":
      await runTokenUnrestricted(context);
      return;
    case "cleanup":
      await runTokenCleanup(context);
      return;
    default:
      context.stderr.write(
        `Unknown token action: ${sub}. Available: list, create, revoke, restrict, unrestricted, cleanup\n`
      );
  }
}

async function runTokenCleanup(context: CliContext): Promise<void> {
  const config = requireLoggedInConfig(context);
  try {
    // Clean up expired sessions on the user's account
    const { removed } = await cleanupSessions(config);
    context.stdout.write(pc.green(`✓ Cleaned up ${removed} expired sessions\n`));
  } catch (err) {
    context.stderr.write(pc.red(`Failed to clean up sessions: ${err}\n`));
  }
}

async function runTokenList(context: CliContext): Promise<void> {
  const config = requireLoggedInConfig(context);
  try {
    const { userTokens } = await listUserTokens(config);
    if (userTokens.length === 0) {
      context.stdout.write(
        "No CLI tokens found. Create one with `thingd cloud token create <name>`\n"
      );
      return;
    }
    // Header
    const header = `${pc.bold("Name".padEnd(18))} ${pc.bold("Prefix".padEnd(22))} ${pc.bold("Created".padEnd(14))} ${pc.bold("Last Used".padEnd(14))} ${pc.bold("Access")}`;
    context.stdout.write(`${header}\n`);
    context.stdout.write(`${pc.dim("─".repeat(header.length))}\n`);
    for (const t of userTokens) {
      if (t.revokedAt) {
        continue; // Skip revoked tokens
      }
      const access = t.projectAccess === "all" ? "All" : t.projectAccess;
      context.stdout.write(
        `${t.name.padEnd(18)} ${pc.dim(t.prefix.padEnd(22))} ${formatTimeAgo(t.createdAt).padEnd(14)} ${(t.lastUsedAt ? formatTimeAgo(t.lastUsedAt) : "never").padEnd(14)} ${pc.cyan(access)}\n`
      );
    }
  } catch (err) {
    context.stderr.write(pc.red(`Failed to list tokens: ${err}\n`));
  }
}

async function runTokenCreate(context: CliContext): Promise<void> {
  const config = requireLoggedInConfig(context);
  const name = context.parsed.tokens[3] ?? "cli-token";
  try {
    const { token: userToken } = await createUserToken(config, name);
    context.stdout.write(
      `\n${pc.green("✓ Token created")}\n\n` +
        `${pc.bold(userToken)}\n\n` +
        `${pc.yellow("⚠ This token will only be shown once. Copy it now.\n")}` +
        `${pc.dim("Press Enter to continue...")}\n`
    );
    // Wait for user to acknowledge
    const rl = createInterface({
      input: context.stdin as NodeJS.ReadableStream,
      output: context.stderr as NodeJS.WritableStream,
    });
    await rl.question("");
    rl.close();
  } catch (err) {
    context.stderr.write(pc.red(`Failed to create token: ${err}\n`));
  }
}

async function runTokenRevoke(context: CliContext): Promise<void> {
  const config = requireLoggedInConfig(context);
  const tokenId = requiredToken(context.parsed, 3, "token-id");
  try {
    await revokeUserToken(config, tokenId);
    context.stdout.write(pc.green(`✓ Token ${tokenId} revoked\n`));
  } catch (err) {
    if (err instanceof CloudApiError && err.status === 404) {
      context.stderr.write(pc.red("Token not found.\n"));
    } else {
      context.stderr.write(pc.red(`Failed to revoke token: ${err}\n`));
    }
  }
}

async function runTokenRestrict(context: CliContext): Promise<void> {
  const config = requireLoggedInConfig(context);
  const tokenId = requiredToken(context.parsed, 3, "token-id");
  const projectSlug = requiredToken(context.parsed, 4, "project");
  try {
    const { userToken } = await updateUserToken(config, tokenId, {
      projectAccess: projectSlug,
    });
    context.stdout.write(pc.green(`✓ Token "${userToken.name}" restricted to: ${projectSlug}\n`));
  } catch (err) {
    if (err instanceof CloudApiError && err.status === 404) {
      context.stderr.write(pc.red("Token not found.\n"));
    } else {
      context.stderr.write(pc.red(`Failed to update token: ${err}\n`));
    }
  }
}

async function runTokenUnrestricted(context: CliContext): Promise<void> {
  const config = requireLoggedInConfig(context);
  const tokenId = requiredToken(context.parsed, 3, "token-id");
  try {
    const { userToken } = await updateUserToken(config, tokenId, {
      projectAccess: "all",
    });
    context.stdout.write(pc.green(`✓ Token "${userToken.name}" now has access to all projects\n`));
  } catch (err) {
    if (err instanceof CloudApiError && err.status === 404) {
      context.stderr.write(pc.red("Token not found.\n"));
    } else {
      context.stderr.write(pc.red(`Failed to update token: ${err}\n`));
    }
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
    config.projectId = project.id;
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
