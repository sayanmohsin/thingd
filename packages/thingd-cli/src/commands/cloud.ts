import { readCloudConfig, writeCloudConfig, removeCloudConfig, type CloudConfig } from "../lib/cloud-config.js";
import {
  getMe,
  listProjects,
  createProject,
  listInstances,
  createInstance,
  createApiKey,
  CloudApiError,
} from "../lib/cloud-api.js";
import { requiredToken, requiredFlag, stringFlag, type CliContext } from "../index.js";
import pc from "picocolors";

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
  const code = context.parsed.tokens[2] ?? requiredFlag(context.parsed, "code");
  const token = context.parsed.tokens[3] ?? stringFlag(context.parsed, "token");

  if (!token) {
    context.stdout.write(
      `First, open this URL in your browser:\n\n` +
      `  ${pc.cyan(`https://thingd.cloud/cli/auth?code=${code}`)}\n\n` +
      `Then paste the token shown after logging in:\n\n` +
      `  ${pc.dim("$ thingd cloud login --code <code> --token <token>")}\n`
    );
    return;
  }

  // Verify token by calling /api/users/me
  const config: CloudConfig = { token };
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
}

async function runLogout(context: CliContext): Promise<void> {
  removeCloudConfig();
  context.stdout.write(pc.green("✓ Logged out\n"));
}

async function runCloudStatus(context: CliContext): Promise<void> {
  const config = readCloudConfig();
  if (!config) {
    context.stdout.write("Not logged in. Run " + pc.cyan("thingd cloud login") + "\n");
    return;
  }

  try {
    const { user } = await getMe(config);
    context.stdout.write(
      `Logged in as ${pc.green(user.email)} (${user.role})\n` +
      `API: ${config.url ?? "https://api.thingd.cloud"}\n`
    );
  } catch {
    context.stdout.write("Token expired. Run " + pc.cyan("thingd cloud login") + "\n");
  }
}

async function requireConfig(context: CliContext): Promise<CloudConfig> {
  const config = readCloudConfig();
  if (!config) {
    context.stderr.write("Not logged in. Run " + pc.cyan("thingd cloud login") + " first.\n");
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
    // Resolve project slug to ID by listing all projects
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
