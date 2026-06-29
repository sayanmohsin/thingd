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

export type CloudOrganization = {
  id: string;
  name: string;
  slug: string;
  createdAt: string;
};

export type CloudOrganizationMember = {
  id: string;
  organizationId: string;
  userId: string;
  role: string;
  invitedBy: string;
  joinedAt: string;
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
  name: string,
  organizationId?: string
): Promise<{ project: CloudProject }> {
  const body: Record<string, string> = { name };
  if (organizationId) {
    body.organizationId = organizationId;
  }
  return request(config, "/api/projects", { method: "POST", body });
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

// ── Organization API ─────────────────────────────────────────────────

export async function createOrganization(
  config: CloudConfig,
  name: string
): Promise<{ organization: CloudOrganization }> {
  return request(config, "/api/organizations", { method: "POST", body: { name } });
}

export async function listOrganizations(
  config: CloudConfig
): Promise<{ organizations: CloudOrganization[] }> {
  return request(config, "/api/organizations");
}

export async function getOrganization(
  config: CloudConfig,
  orgId: string
): Promise<{ organization: CloudOrganization; role: string }> {
  return request(config, `/api/organizations/${orgId}`);
}

export async function listOrganizationMembers(
  config: CloudConfig,
  orgId: string
): Promise<{ members: CloudOrganizationMember[] }> {
  return request(config, `/api/organizations/${orgId}/members`);
}

export async function addOrganizationMember(
  config: CloudConfig,
  orgId: string,
  userId: string,
  role: string = "member"
): Promise<{ member: CloudOrganizationMember }> {
  return request(config, `/api/organizations/${orgId}/members`, {
    method: "POST",
    body: { userId, role },
  });
}

export async function removeOrganizationMember(
  config: CloudConfig,
  orgId: string,
  userId: string
): Promise<{ ok: boolean }> {
  return request(config, `/api/organizations/${orgId}/members/${userId}`, {
    method: "DELETE",
  });
}

// ── CLI device code auth (unauthenticated) ──────────────────────────

async function requestUnauthenticated<T>(apiUrl: string, path: string, body: unknown): Promise<T> {
  const url = `${apiUrl}${path}`;
  const res = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const errBody = await res.json().catch(() => ({ message: res.statusText }));
    throw new CloudApiError(res.status, errBody.message ?? res.statusText);
  }
  return res.json() as Promise<T>;
}

export async function startCliAuth(config: CloudConfig): Promise<{ code: string }> {
  return requestUnauthenticated(config.url ?? DEFAULT_API_URL, "/api/auth/cli/start", {});
}

export async function pollCliAuth(
  config: CloudConfig,
  code: string
): Promise<{ token: string } | { status: string }> {
  return requestUnauthenticated(config.url ?? DEFAULT_API_URL, "/api/auth/cli/poll", { code });
}
