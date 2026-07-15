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

export type UserTokenDto = {
  id: string;
  name: string;
  prefix: string;
  projectAccess: string;
  createdAt: string;
  lastUsedAt: string | null;
  revokedAt: string | null;
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

function resolveAuthToken(config: CloudConfig): string {
  return config.userToken ?? config.token ?? config.apiKey ?? "";
}

async function request<T>(config: CloudConfig, path: string, opts: ApiOptions = {}): Promise<T> {
  const url = `${config.url ?? DEFAULT_API_URL}${path}`;
  const authToken = resolveAuthToken(config);
  const headers: Record<string, string> = {
    authorization: `Bearer ${authToken}`,
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
  return request(config, "/users/me");
}

export async function listProjects(config: CloudConfig): Promise<{ projects: CloudProject[] }> {
  return request(config, "/projects");
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
  return request(config, "/projects", { method: "POST", body });
}

export async function listInstances(
  config: CloudConfig,
  projectId: string
): Promise<{ instances: CloudInstance[] }> {
  return request(config, `/projects/${projectId}/instances`);
}

export async function createInstance(
  config: CloudConfig,
  projectId: string,
  name: string
): Promise<{ instance: CloudInstance }> {
  return request(config, `/projects/${projectId}/instances`, {
    method: "POST",
    body: { name },
  });
}

export async function createApiKey(
  config: CloudConfig,
  projectId: string,
  name?: string
): Promise<{ key: CloudApiKey; token: string }> {
  return request(config, `/projects/${projectId}/api-keys`, {
    method: "POST",
    body: { name: name ?? "thingd CLI" },
  });
}

// ── Organization API ─────────────────────────────────────────────────

export async function createOrganization(
  config: CloudConfig,
  name: string
): Promise<{ organization: CloudOrganization }> {
  return request(config, "/organizations", { method: "POST", body: { name } });
}

export async function listOrganizations(
  config: CloudConfig
): Promise<{ organizations: CloudOrganization[] }> {
  return request(config, "/organizations");
}

export async function getOrganization(
  config: CloudConfig,
  orgId: string
): Promise<{ organization: CloudOrganization; role: string }> {
  return request(config, `/organizations/${orgId}`);
}

export async function listOrganizationMembers(
  config: CloudConfig,
  orgId: string
): Promise<{ members: CloudOrganizationMember[] }> {
  return request(config, `/organizations/${orgId}/members`);
}

export async function addOrganizationMember(
  config: CloudConfig,
  orgId: string,
  userId: string,
  role: string = "member"
): Promise<{ member: CloudOrganizationMember }> {
  return request(config, `/organizations/${orgId}/members`, {
    method: "POST",
    body: { userId, role },
  });
}

export async function removeOrganizationMember(
  config: CloudConfig,
  orgId: string,
  userId: string
): Promise<{ ok: boolean }> {
  return request(config, `/organizations/${orgId}/members/${userId}`, {
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
  return requestUnauthenticated(config.url ?? DEFAULT_API_URL, "/auth/cli/start", {});
}

export async function pollCliAuth(
  config: CloudConfig,
  code: string
): Promise<{ token: string } | { status: string }> {
  return requestUnauthenticated(config.url ?? DEFAULT_API_URL, "/auth/cli/poll", { code });
}

// ── Instance auto-discovery ────────────────────────────────────────

export type ResolvedInstance = {
  mcpUrl: string;
  projectId: string;
  projectSlug: string;
  instanceSlug: string;
};

/**
 * Derive the REST base URL from an MCP gateway URL.
 * 'https://api.thingd.cloud/mcp/proj/inst' → 'https://api.thingd.cloud'
 * 'https://api.thingd.cloud' → 'https://api.thingd.cloud' (idempotent)
 */
export function deriveRestUrl(mcpUrl: string): string {
  return new URL(mcpUrl).origin;
}

/**
 * Fetch the first available cloud instance for the logged-in user.
 * Returns null if no projects or instances exist.
 */
export async function resolveFirstInstance(config: CloudConfig): Promise<ResolvedInstance | null> {
  try {
    const { projects } = await listProjects(config);
    for (const project of projects) {
      try {
        const { instances } = await listInstances(config, project.id);
        const instance = instances[0];
        if (instance?.mcpUrl) {
          return {
            mcpUrl: instance.mcpUrl,
            projectId: project.id,
            projectSlug: project.slug,
            instanceSlug: instance.slug,
          };
        }
      } catch {
        // Skip projects that fail to list instances
      }
    }
  } catch {
    // API unreachable or token invalid
  }
  return null;
}

// ── User Token API ──────────────────────────────────────────────────

export async function createUserToken(
  config: CloudConfig,
  name: string,
  projectAccess?: string
): Promise<{ token: string; userToken: UserTokenDto }> {
  return request(config, "/auth/user-tokens", {
    method: "POST",
    body: { name, projectAccess },
  });
}

export async function listUserTokens(
  config: CloudConfig
): Promise<{ userTokens: UserTokenDto[] }> {
  return request(config, "/auth/user-tokens");
}

export async function revokeUserToken(
  config: CloudConfig,
  tokenId: string
): Promise<void> {
  await request(config, `/auth/user-tokens/${tokenId}`, { method: "DELETE" });
}

export async function updateUserToken(
  config: CloudConfig,
  tokenId: string,
  updates: { name?: string; projectAccess?: string }
): Promise<{ userToken: UserTokenDto }> {
  return request(config, `/auth/user-tokens/${tokenId}`, {
    method: "PATCH",
    body: updates,
  });
}

/**
 * Extract the token ID from a full user token string (md_user_<hexId>_<secret> → utk_<hexId>).
 */
export function parseUserTokenId(userToken: string): string | null {
  const match = /^md_user_([a-f0-9]{20})_/.exec(userToken);
  if (!match?.[1]) {
    return null;
  }
  return `utk_${match[1]}`;
}

/**
 * Fetch all cloud instances across all projects for the logged-in user.
 * Returns an empty array if no projects or instances exist.
 */
export async function resolveAllInstances(config: CloudConfig): Promise<ResolvedInstance[]> {
  const all: ResolvedInstance[] = [];
  try {
    const { projects } = await listProjects(config);
    for (const project of projects) {
      try {
        const { instances } = await listInstances(config, project.id);
        for (const instance of instances) {
          if (instance?.mcpUrl) {
            all.push({
              mcpUrl: instance.mcpUrl,
              projectId: project.id,
              projectSlug: project.slug,
              instanceSlug: instance.slug,
            });
          }
        }
      } catch {
        // Skip projects that fail to list instances
      }
    }
  } catch {
    // API unreachable or token invalid
  }
  return all;
}
