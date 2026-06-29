import type { CloudConfig } from "./cloud-config.js";

const DEFAULT_API_URL = "https://api.thingd.cloud";

type ApiOptions = {
  method?: string;
  body?: unknown;
};

export type CloudProject = {
  id: string;
  name: string;
  slug: string;
  createdAt: string;
};

export type CloudInstance = {
  id: string;
  name: string;
  slug: string;
  mcpUrl: string;
  createdAt: string;
};

export type CloudApiKey = {
  id: string;
  name: string;
  prefix: string;
  token?: string;
  createdAt: string;
};

export class CloudApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
    this.name = "CloudApiError";
  }
}

async function request<T>(config: CloudConfig, path: string, opts: ApiOptions = {}): Promise<T> {
  const url = `${config.url ?? DEFAULT_API_URL}${path}`;
  const headers: Record<string, string> = {
    authorization: `Bearer ${config.token}`,
    "content-type": "application/json",
  };

  const res = await fetch(url, {
    method: opts.method ?? "GET",
    headers,
    body: opts.body ? JSON.stringify(opts.body) : undefined,
  });

  if (!res.ok) {
    if (res.status === 401) {
      throw new CloudApiError(401, "Token expired or invalid. Run `thingd cloud login` again.");
    }
    const body = await res.json().catch(() => ({ message: res.statusText }));
    throw new CloudApiError(res.status, body.message ?? res.statusText);
  }

  return res.json() as Promise<T>;
}

export async function getMe(
  config: CloudConfig
): Promise<{ user: { id: string; email: string; name: string; role: string } }> {
  return request(config, "/api/users/me");
}

export async function listProjects(config: CloudConfig): Promise<{ projects: CloudProject[] }> {
  return request(config, "/api/projects");
}

export async function createProject(
  config: CloudConfig,
  name: string
): Promise<{ project: CloudProject }> {
  return request(config, "/api/projects", { method: "POST", body: { name } });
}

export async function listInstances(
  config: CloudConfig,
  projectId: string
): Promise<{ instances: CloudInstance[] }> {
  return request(config, `/api/projects/${projectId}/instances`);
}

export async function createInstance(
  config: CloudConfig,
  projectId: string,
  name: string
): Promise<{ instance: CloudInstance }> {
  return request(config, `/api/projects/${projectId}/instances`, {
    method: "POST",
    body: { name },
  });
}

export async function createApiKey(
  config: CloudConfig,
  projectId: string,
  name?: string
): Promise<{ key: CloudApiKey }> {
  return request(config, `/api/projects/${projectId}/api-keys`, {
    method: "POST",
    body: { name: name ?? "CLI key" },
  });
}
