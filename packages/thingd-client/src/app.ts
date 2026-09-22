import type {
  AppAction,
  AppAuthResponse,
  AppManifest,
  AppObject,
  AppSearchOptions,
  AppSearchResult,
  AppUser,
} from "./types.js";

export type ThingdAppClientOptions = {
  baseUrl: string;
  publishableKey: string;
  fetch?: typeof globalThis.fetch;
  accessToken?: string;
  onSessionChange?: (session: AppAuthResponse | null) => void;
};

export class ThingdAppError extends Error {
  readonly status: number;
  readonly code: string;
  readonly requestId?: string;

  constructor(status: number, code: string, message: string, requestId?: string) {
    super(message);
    this.name = "ThingdAppError";
    this.status = status;
    this.code = code;
    this.requestId = requestId;
  }
}

type AppEnvelope<T> = {
  data: T;
  requestId?: string;
};

type AppErrorEnvelope = {
  error?: {
    type?: string;
    code?: string;
    detail?: string;
    message?: string;
  };
  requestId?: string;
};

type AppManifestPayload = Partial<AppManifest> & {
  schemaVersion?: string;
  version?: string;
  functions?: AppAction[];
  actions?: AppAction[];
};

function appPath(baseUrl: string): string {
  let normalized = baseUrl;
  while (normalized.endsWith("/")) {
    normalized = normalized.slice(0, -1);
  }
  return normalized.endsWith("/v1") ? normalized : `${normalized}/v1`;
}

function normalizeManifest(payload: AppManifestPayload): AppManifest {
  if (!payload.project || !payload.app || !payload.instance) {
    throw new ThingdAppError(
      502,
      "invalid_app_manifest",
      "The published app manifest is missing project, app, or instance identity"
    );
  }
  const actions = payload.actions ?? payload.functions ?? [];
  return {
    schemaVersion: "thingd.app/v1",
    version: payload.version ?? payload.schemaVersion ?? "thingd.app/v1",
    project: payload.project,
    app: payload.app,
    instance: payload.instance,
    entities: payload.entities ?? [],
    audiences: payload.audiences ?? [],
    roles: payload.roles ?? [],
    actions,
    functions: actions,
    views: payload.views ?? [],
    workflows: payload.workflows ?? [],
    integrations: payload.integrations ?? [],
    policies: payload.policies ?? {},
    distribution: payload.distribution ?? {},
    presentation: payload.presentation ?? {},
    capabilities: payload.capabilities ?? { reads: false, namedWrites: false },
  };
}

/**
 * Zero-dependency client for a hosted thingd app backend.
 *
 * The publishable key is safe to embed in browser and mobile applications.
 * Project-user access tokens are kept in memory by default; applications can
 * persist them using their platform's secure storage facilities.
 */
export class ThingdAppClient {
  private readonly base: string;
  private readonly fetcher: typeof globalThis.fetch;
  private accessToken: string | undefined;

  constructor(private readonly options: ThingdAppClientOptions) {
    this.base = appPath(options.baseUrl);
    this.fetcher = options.fetch ?? globalThis.fetch;
    this.accessToken = options.accessToken;
  }

  setAccessToken(token: string | undefined): void {
    this.accessToken = token;
  }

  getAccessToken(): string | undefined {
    return this.accessToken;
  }

  private async request<T>(method: string, path: string, body?: unknown, idempotencyKey?: string) {
    const headers: Record<string, string> = {
      accept: "application/json",
      "x-thingd-publishable-key": this.options.publishableKey,
    };
    if (body !== undefined) {
      headers["content-type"] = "application/json";
    }
    if (this.accessToken) {
      headers.authorization = `Bearer ${this.accessToken}`;
    }
    if (idempotencyKey) {
      headers["idempotency-key"] = idempotencyKey;
    }

    const response = await this.fetcher(`${this.base}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const payload = (await response.json()) as AppEnvelope<T> & AppErrorEnvelope;
    if (!response.ok) {
      const error = payload.error;
      throw new ThingdAppError(
        response.status,
        error?.code ?? error?.type ?? "app_request_failed",
        error?.detail ?? error?.message ?? `HTTP ${response.status}`,
        payload.requestId
      );
    }
    return payload.data;
  }

  async manifest(): Promise<AppManifest> {
    const payload = await this.request<AppManifestPayload>("GET", "/app/manifest");
    return normalizeManifest(payload);
  }

  readonly auth = {
    signUp: async (input: { email: string; password: string; name: string }) => {
      const result = await this.request<AppAuthResponse>("POST", "/app/auth/signup", input);
      this.setAccessToken(result.accessToken);
      this.options.onSessionChange?.(result);
      return result;
    },
    signIn: async (input: { email: string; password: string }) => {
      const result = await this.request<AppAuthResponse>("POST", "/app/auth/login", input);
      this.setAccessToken(result.accessToken);
      this.options.onSessionChange?.(result);
      return result;
    },
    refresh: async (refreshToken: string) => {
      const result = await this.request<AppAuthResponse>("POST", "/app/auth/refresh", {
        refreshToken,
      });
      this.setAccessToken(result.accessToken);
      this.options.onSessionChange?.(result);
      return result;
    },
    getCurrentUser: () => this.request<AppUser>("GET", "/app/auth/me"),
    signOut: async () => {
      try {
        if (this.accessToken) {
          await this.request<{ ok: true }>("POST", "/app/auth/logout", {});
        }
      } finally {
        this.setAccessToken(undefined);
        this.options.onSessionChange?.(null);
      }
    },
  };

  readonly actions = {
    list: () => this.request<AppAction[]>("GET", "/app/functions"),
    get: (name: string) =>
      this.request<AppAction>("GET", `/app/functions/${encodeURIComponent(name)}`),
    invoke: <T = unknown>(name: string, input: unknown, options?: { idempotencyKey?: string }) =>
      this.request<T>(
        "POST",
        `/app/functions/${encodeURIComponent(name)}`,
        input,
        options?.idempotencyKey
      ),
  };

  /** @deprecated Use actions. */
  readonly functions = this.actions;

  readonly objects = {
    get: <T extends AppObject = AppObject>(collection: string, id: string) =>
      this.request<T | null>(
        "GET",
        `/app/objects/${encodeURIComponent(collection)}/${encodeURIComponent(id)}`
      ),
  };

  readonly search = (query: string, options?: AppSearchOptions) =>
    this.request<AppSearchResult[]>("POST", "/app/search", { query, ...options });
}

export function createThingdAppClient(options: ThingdAppClientOptions): ThingdAppClient {
  return new ThingdAppClient(options);
}
