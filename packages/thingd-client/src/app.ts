import type {
  AppAction,
  AppActionUsage,
  AppAuthResponse,
  AppErrorDetail,
  AppManifest,
  AppObject,
  AppObjectListOptions,
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
  expectedIdentity?: {
    projectId?: string;
    appId?: string;
    instanceId?: string;
  };
};

export class ThingdAppError extends Error {
  readonly status: number;
  readonly code: string;
  readonly requestId?: string;
  readonly details?: AppErrorDetail[];

  constructor(
    status: number,
    code: string,
    message: string,
    requestId?: string,
    details?: AppErrorDetail[]
  ) {
    super(message);
    this.name = "ThingdAppError";
    this.status = status;
    this.code = code;
    this.requestId = requestId;
    this.details = details;
  }
}

type AppEnvelope<T> = {
  data: T;
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function safeErrorDetails(value: unknown): AppErrorDetail[] | undefined {
  if (!Array.isArray(value)) {
    return undefined;
  }
  const details = value.slice(0, 12).flatMap((candidate): AppErrorDetail[] => {
    if (!isRecord(candidate)) {
      return [];
    }
    if (typeof candidate.path !== "string" || typeof candidate.keyword !== "string") {
      return [];
    }
    const params: Record<string, string | number | boolean> = {};
    if (isRecord(candidate.params)) {
      for (const [key, item] of Object.entries(candidate.params).slice(0, 12)) {
        if (
          (typeof item === "string" && item.length <= 160) ||
          typeof item === "number" ||
          typeof item === "boolean"
        ) {
          params[key.slice(0, 80)] = item;
        }
      }
    }
    return [
      {
        path: candidate.path.slice(0, 256),
        keyword: candidate.keyword.slice(0, 80),
        ...(Object.keys(params).length > 0 ? { params } : {}),
      },
    ];
  });
  return details.length > 0 ? details : undefined;
}

function normalizeManifest(payload: AppManifestPayload): AppManifest {
  if (
    !payload.project ||
    typeof payload.project.id !== "string" ||
    !payload.app ||
    typeof payload.app.id !== "string" ||
    !payload.instance ||
    typeof payload.instance.id !== "string"
  ) {
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
    let payload: unknown;
    try {
      payload = await response.json();
    } catch {
      payload = undefined;
    }
    const responseRecord = isRecord(payload) ? payload : {};
    const errorPayload = responseRecord.error;
    if (!response.ok) {
      const error = isRecord(errorPayload) ? errorPayload : undefined;
      const code =
        typeof errorPayload === "string"
          ? errorPayload
          : typeof error?.code === "string"
            ? error.code
            : typeof error?.type === "string"
              ? error.type
              : "app_request_failed";
      const message =
        (typeof error?.message === "string" && error.message) ||
        (typeof error?.detail === "string" && error.detail) ||
        (typeof responseRecord.message === "string" && responseRecord.message) ||
        (typeof errorPayload === "string" && errorPayload) ||
        `HTTP ${response.status}`;
      const requestId =
        (typeof responseRecord.requestId === "string" && responseRecord.requestId) ||
        response.headers.get("x-request-id") ||
        undefined;
      throw new ThingdAppError(
        response.status,
        code,
        message,
        requestId,
        safeErrorDetails(error?.details)
      );
    }
    if (!isRecord(payload) || !("data" in payload)) {
      throw new ThingdAppError(
        502,
        "invalid_app_response",
        "The app backend returned an invalid response",
        response.headers.get("x-request-id") ?? undefined
      );
    }
    return (payload as AppEnvelope<T>).data;
  }

  async manifest(): Promise<AppManifest> {
    const payload = await this.request<AppManifestPayload>("GET", "/app/manifest");
    const manifest = normalizeManifest(payload);
    const expected = this.options.expectedIdentity;
    const mismatches = [
      expected?.projectId && manifest.project.id !== expected.projectId
        ? `project ${manifest.project.id}`
        : undefined,
      expected?.appId && manifest.app.id !== expected.appId ? `app ${manifest.app.id}` : undefined,
      expected?.instanceId && manifest.instance.id !== expected.instanceId
        ? `instance ${manifest.instance.id}`
        : undefined,
    ].filter((value): value is string => Boolean(value));
    if (mismatches.length > 0) {
      throw new ThingdAppError(
        409,
        "app_identity_mismatch",
        `The published app identity does not match the configured identity (${mismatches.join(", ")})`
      );
    }
    return manifest;
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
    getUsage: (name: string) =>
      this.request<AppActionUsage>("GET", `/app/actions/${encodeURIComponent(name)}/usage`),
  };

  /** @deprecated Use actions. */
  readonly functions = this.actions;

  readonly objects = {
    get: <T extends AppObject = AppObject>(collection: string, id: string) =>
      this.request<T | null>(
        "GET",
        `/app/objects/${encodeURIComponent(collection)}/${encodeURIComponent(id)}`
      ),
    list: <T extends AppObject = AppObject>(
      collection: string,
      options: AppObjectListOptions = {}
    ) => {
      const params = new URLSearchParams();
      if (options.limit !== undefined) {
        params.set("limit", String(options.limit));
      }
      if (options.offset !== undefined) {
        params.set("offset", String(options.offset));
      }
      const query = params.size > 0 ? `?${params.toString()}` : "";
      return this.request<T[]>("GET", `/app/objects/${encodeURIComponent(collection)}${query}`);
    },
  };

  readonly search = (query: string, options?: AppSearchOptions) =>
    this.request<AppSearchResult[]>("POST", "/app/search", { query, ...options });
}

export function createThingdAppClient(options: ThingdAppClientOptions): ThingdAppClient {
  return new ThingdAppClient(options);
}
