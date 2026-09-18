import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";
import { createThingdAppClient } from "@thingd/client";
import { parseSchema } from "@thingd/sdk";
import {
  type CliContext,
  hasFlag,
  requiredFlag,
  requiredToken,
  stringFlag,
  writeJson,
} from "../index.js";
import {
  type CloudAppFunction,
  type CloudInstance,
  type CloudProject,
  type CloudPublishApp,
  createAppFunction,
  createPublishApp,
  getAppConfig,
  listAppFunctions,
  listInstances,
  listProjects,
  listPublishApps,
  transitionAppFunction,
  transitionPublishApp,
  updateAppFunction,
  updatePublishApp,
  validatePublishApp,
  validateRuntimeSchema,
} from "../lib/cloud-api.js";
import { type CloudConfig, readCloudConfig } from "../lib/cloud-config.js";

const APP_SCHEMA_VERSION = "thingd.app/v1";

type AppDefinition = Record<string, unknown> & {
  schemaVersion?: unknown;
  name?: unknown;
  slug?: unknown;
  description?: unknown;
};

type AppReference = {
  project: CloudProject;
  app: CloudPublishApp;
};

const APP_HELP = {
  usage: "thingd cloud app <command>",
  commands: {
    init: "Create a declarative thingd.app/v1 definition file",
    bootstrap: "Validate and create or update a Publish app",
    config: "Show the mobile app backend configuration",
    list: "List Publish apps in a project",
    create: "Create a Publish app from a definition file",
    update: "Update a Publish app from a definition file",
    validate: "Validate an existing Publish app",
    test: "Move a Publish app to test status",
    publish: "Publish a Publish app",
    disable: "Disable a Publish app",
    rollback: "Roll back a Publish app to a version",
    functions: "Manage project app-backend functions",
    smoke: "Run an app-client signup/login/manifest smoke test",
  },
};

function requireCloudConfig(): CloudConfig {
  const config = readCloudConfig();
  if (!config?.userToken && !config?.token) {
    throw new Error("Not logged in. Run `thingd cloud login` first.");
  }
  return config;
}

function appFilePath(context: CliContext): string {
  return resolve(requiredFlag(context.parsed, "file"));
}

function readDefinition(file: string): AppDefinition {
  if (!existsSync(file)) {
    throw new Error(`App definition not found: ${file}`);
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(readFileSync(file, "utf8"));
  } catch (error) {
    throw new Error(
      `Invalid JSON in ${file}: ${error instanceof Error ? error.message : String(error)}`
    );
  }

  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error(`App definition must be a JSON object: ${file}`);
  }

  const definition = parsed as AppDefinition;
  if (definition.schemaVersion !== APP_SCHEMA_VERSION) {
    throw new Error(`App definition schemaVersion must be ${APP_SCHEMA_VERSION}`);
  }
  for (const field of ["name", "slug"]) {
    if (typeof definition[field] !== "string" || definition[field].length === 0) {
      throw new Error(`App definition field '${field}' must be a non-empty string`);
    }
  }

  return definition;
}

function readJsonObject(file: string): Record<string, unknown> {
  if (!existsSync(file)) {
    throw new Error(`Definition not found: ${file}`);
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(readFileSync(file, "utf8"));
  } catch (error) {
    throw new Error(
      `Invalid JSON in ${file}: ${error instanceof Error ? error.message : String(error)}`
    );
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error(`Definition must be a JSON object: ${file}`);
  }
  return parsed as Record<string, unknown>;
}

function defaultSlug(file: string): string {
  return (
    basename(file)
      .replace(/\.app\.json$/i, "")
      .replace(/[^a-z0-9]+/gi, "-")
      .replace(/^-|-$/g, "")
      .toLowerCase() || "nice-rep"
  );
}

function skeletonDefinition(name: string, slug: string): AppDefinition {
  return {
    schemaVersion: APP_SCHEMA_VERSION,
    name,
    slug,
    description: "",
    entities: [],
    audiences: [],
    roles: [],
    actions: [],
    views: [
      { key: "search", kind: "search", label: "Search", placeholder: "Search…" },
      { key: "results", kind: "results", label: "Results" },
    ],
    workflows: [],
    integrations: [],
    policies: {},
    distribution: {
      hosted: true,
      embed: true,
      api: true,
      mcp: true,
      standalone: false,
      adapters: [],
    },
    presentation: {},
  };
}

function resolveProject(projects: CloudProject[], value: string): CloudProject {
  const project = projects.find((candidate) => candidate.id === value || candidate.slug === value);
  if (!project) {
    throw new Error(`Project not found: ${value}`);
  }
  return project;
}

async function getProject(config: CloudConfig, context: CliContext): Promise<CloudProject> {
  const value = requiredFlag(context.parsed, "project");
  const { projects } = await listProjects(config);
  return resolveProject(projects, value);
}

async function getInstance(
  config: CloudConfig,
  project: CloudProject,
  context: CliContext
): Promise<CloudInstance> {
  const value = requiredFlag(context.parsed, "instance");
  const { instances } = await listInstances(config, project.id);
  const instance = instances.find(
    (candidate) => candidate.id === value || candidate.slug === value
  );
  if (!instance) {
    throw new Error(`Instance not found in ${project.slug}: ${value}`);
  }
  return instance;
}

function output(context: CliContext, value: unknown): void {
  writeJson(context.stdout, value, context.pretty);
}

async function resolveApp(context: CliContext): Promise<AppReference> {
  const config = requireCloudConfig();
  const project = await getProject(config, context);
  const appValue = stringFlag(context.parsed, "app") ?? stringFlag(context.parsed, "slug");
  if (!appValue) {
    throw new Error("Missing required flag: --app or --slug");
  }
  const { apps } = await listPublishApps(config, project.id);
  const app = apps.find((candidate) => candidate.id === appValue || candidate.slug === appValue);
  if (!app) {
    throw new Error(`Publish app not found in ${project.slug}: ${appValue}`);
  }
  return { project, app };
}

async function runInit(context: CliContext): Promise<void> {
  const file = appFilePath(context);
  if (existsSync(file)) {
    throw new Error(`Refusing to overwrite existing app definition: ${file}`);
  }
  const slug = stringFlag(context.parsed, "slug") ?? defaultSlug(file);
  const name = stringFlag(context.parsed, "name") ?? slug.replace(/-/g, " ");
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, `${JSON.stringify(skeletonDefinition(name, slug), null, 2)}\n`, "utf8");
  output(context, { created: file, schemaVersion: APP_SCHEMA_VERSION, name, slug });
}

async function runConfig(context: CliContext): Promise<void> {
  const config = requireCloudConfig();
  const project = await getProject(config, context);
  const result = await getAppConfig(config, project.id);
  const baseUrl = (config.url ?? "https://api.thingd.cloud").replace(/\/+$/, "");
  output(context, {
    baseUrl,
    appBaseUrl: `${baseUrl}/v1`,
    appEndpoint: `${baseUrl}/v1/app`,
    project: { id: project.id, slug: project.slug },
    credentialType: "publishable_key",
    publishableKey: result.app.publishableKey,
    mobileClient: "createThingdAppClient",
    appUserToken: "issued by /v1/app/auth/login or /v1/app/auth/signup; never generated by the CLI",
  });
}

async function validateSchemaIfRequested(
  config: CloudConfig,
  project: CloudProject,
  instance: CloudInstance | undefined,
  context: CliContext
): Promise<Record<string, unknown> | undefined> {
  const schemaFile = stringFlag(context.parsed, "schema");
  if (!schemaFile) {
    return undefined;
  }
  const file = resolve(schemaFile);
  if (!existsSync(file)) {
    throw new Error(`Schema file not found: ${file}`);
  }
  const source = readFileSync(file, "utf8");
  await parseSchema(source);
  if (!instance) {
    return { file, local: "valid", remote: "skipped" };
  }
  const remote = await validateRuntimeSchema(config, project.id, instance.id, source);
  return { file, local: "valid", remote };
}

async function runBootstrap(context: CliContext): Promise<void> {
  const config = requireCloudConfig();
  const project = await getProject(config, context);
  const instance = await getInstance(config, project, context);
  const file = appFilePath(context);
  const definition = readDefinition(file);
  const { apps } = await listPublishApps(config, project.id);
  const existing = apps.find((candidate) => candidate.slug === definition.slug);
  const action = existing ? "update" : "create";
  const schema = await validateSchemaIfRequested(
    config,
    project,
    hasFlag(context.parsed, "dry-run") ? undefined : instance,
    context
  );

  if (hasFlag(context.parsed, "dry-run")) {
    output(context, {
      dryRun: true,
      action,
      project: { id: project.id, slug: project.slug },
      instance: { id: instance.id, slug: instance.slug },
      app: { id: existing?.id, slug: definition.slug },
      appDefinition: file,
      schema: schema ?? null,
      publish: hasFlag(context.parsed, "publish"),
      mutations: [],
    });
    return;
  }

  const payload = { ...definition, instanceId: instance.id };
  const result = existing
    ? await updatePublishApp(config, project.id, existing.id, payload)
    : await createPublishApp(config, project.id, payload);
  const app = result.app;
  const validation = await validatePublishApp(config, project.id, app.id);
  let published: { app: CloudPublishApp; hostedUrl?: string } | undefined;
  if (hasFlag(context.parsed, "publish")) {
    published = await transitionPublishApp(config, project.id, app.id, "publish");
  }

  output(context, {
    action,
    project: { id: project.id, slug: project.slug },
    instance: { id: instance.id, slug: instance.slug },
    app,
    schema: schema ?? null,
    validation: validation.report,
    published: published ?? null,
  });
}

async function runCreateOrUpdate(context: CliContext, action: "create" | "update"): Promise<void> {
  const config = requireCloudConfig();
  const project = await getProject(config, context);
  const definition = readDefinition(appFilePath(context));
  const instance = stringFlag(context.parsed, "instance");
  if (action === "create" && !instance) {
    throw new Error("create requires --instance <instance>");
  }
  const instanceId = instance ? (await getInstance(config, project, context)).id : undefined;
  const payload = instanceId ? { ...definition, instanceId } : definition;

  if (action === "create") {
    output(context, await createPublishApp(config, project.id, payload));
    return;
  }

  const { app } = await resolveApp(context);
  output(context, await updatePublishApp(config, project.id, app.id, payload));
}

async function runLifecycle(
  context: CliContext,
  action: "validate" | "test" | "publish" | "disable" | "rollback"
) {
  const config = requireCloudConfig();
  const { project, app } = await resolveApp(context);
  if (action === "validate") {
    output(context, await validatePublishApp(config, project.id, app.id));
    return;
  }
  const version = action === "rollback" ? Number(stringFlag(context.parsed, "version")) : undefined;
  if (action === "rollback" && (!Number.isSafeInteger(version) || (version ?? 0) < 1)) {
    throw new Error("rollback requires a positive --version");
  }
  output(context, await transitionPublishApp(config, project.id, app.id, action, version));
}

async function runFunctions(context: CliContext): Promise<void> {
  const config = requireCloudConfig();
  const project = await getProject(config, context);
  const action = requiredToken(context.parsed, 3, "function action");
  if (action === "list") {
    output(context, await listAppFunctions(config, project.id));
    return;
  }
  const name = stringFlag(context.parsed, "name") ?? context.parsed.tokens[4];
  if (!name && action !== "create") {
    throw new Error("Missing function name; use --name <name>");
  }
  if (action === "create" || action === "update") {
    const definition = readJsonObject(resolve(requiredFlag(context.parsed, "file")));
    output(
      context,
      action === "create"
        ? await createAppFunction(config, project.id, definition)
        : await updateAppFunction(config, project.id, name as string, definition)
    );
    return;
  }
  if (action === "rollback") {
    const version = Number(requiredFlag(context.parsed, "version"));
    if (!Number.isSafeInteger(version) || version < 1) {
      throw new Error("rollback requires a positive --version");
    }
    output(
      context,
      await transitionAppFunction(config, project.id, name as string, "rollback", version)
    );
    return;
  }
  if (action === "test" || action === "publish" || action === "disable") {
    output(context, await transitionAppFunction(config, project.id, name as string, action));
    return;
  }
  throw new Error(`Unknown function action: ${action}`);
}

async function runSmoke(context: CliContext): Promise<void> {
  const config = requireCloudConfig();
  const project = await getProject(config, context);
  const file = appFilePath(context);
  const definition = readDefinition(file);
  const appConfig = await getAppConfig(config, project.id);
  const client = createThingdAppClient({
    baseUrl: config.url ?? "https://api.thingd.cloud",
    publishableKey: appConfig.app.publishableKey,
  });
  const checks: string[] = [];
  const manifest = await client.manifest();
  checks.push(`manifest:${manifest.version}`);

  const email = requiredFlag(context.parsed, "email");
  const password = requiredFlag(context.parsed, "password");
  if (hasFlag(context.parsed, "signup")) {
    await client.auth.signUp({
      email,
      password,
      name: stringFlag(context.parsed, "name") ?? "thingd CLI smoke",
    });
    checks.push("signup");
  } else {
    await client.auth.signIn({ email, password });
    checks.push("login");
  }
  await client.auth.getCurrentUser();
  checks.push("me");
  await client.auth.signOut();
  checks.push("logout");

  output(context, {
    ok: true,
    project: { id: project.id, slug: project.slug },
    app: { slug: definition.slug },
    checks,
    credentialType: "publishable_key",
  });
}

export async function runCloudApp(context: CliContext): Promise<void> {
  const action = context.parsed.tokens[2];
  if (!action) {
    output(context, APP_HELP);
    return;
  }
  if (action === "init") {
    await runInit(context);
    return;
  }
  if (action === "config") {
    await runConfig(context);
    return;
  }
  if (action === "bootstrap") {
    await runBootstrap(context);
    return;
  }
  if (action === "list") {
    const config = requireCloudConfig();
    const project = await getProject(config, context);
    output(context, await listPublishApps(config, project.id));
    return;
  }
  if (action === "create" || action === "update") {
    await runCreateOrUpdate(context, action);
    return;
  }
  if (["validate", "test", "publish", "disable", "rollback"].includes(action)) {
    await runLifecycle(context, action as "validate" | "test" | "publish" | "disable" | "rollback");
    return;
  }
  if (action === "functions") {
    await runFunctions(context);
    return;
  }
  if (action === "smoke") {
    await runSmoke(context);
    return;
  }
  throw new Error(`Unknown cloud app action: ${action}`);
}

export type { CloudAppFunction };
