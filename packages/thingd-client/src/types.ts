// ── Objects ──────────────────────────────────────────

export type MemoryObject = {
  id: string;
  [key: string]: unknown;
};

export type StoredMemoryObject = MemoryObject & {
  collection: string;
  createdAt: string;
  updatedAt: string;
  version: number;
};

export type PutOptions = {
  expectedVersion?: number;
};

export type ThingDeleteResult = {
  deleted: boolean;
};

export type SortBy = {
  field: "id" | "collection" | "created_at" | "updated_at" | "version";
  direction?: "asc" | "desc";
};

export type ListObjectsOptions = {
  limit?: number;
  offset?: number;
  filter?: Record<string, string>;
  sortBy?: SortBy;
};

// ── Hosted app backend ──────────────────────────────

export type AppUser = {
  id: string;
  email: string;
  name: string;
  role: string;
  createdAt: string;
};

export type AppAuthResponse = {
  user: AppUser;
  accessToken: string;
  refreshToken: string;
  expiresIn: number;
};

export type AppAction = {
  name: string;
  description: string;
  auth: "public" | "user" | "role";
  roles?: string[];
  inputSchema: Record<string, unknown>;
  outputSchema: Record<string, unknown>;
  version: number;
  idempotency: "optional" | "required";
};

/** Public authoring fragment for a hosted named action definition. */
export type AppNamedActionDefinition = {
  key: string;
  label: string;
  description: string;
  entity: string;
  inputSchema: Record<string, unknown>;
  outputSchema: Record<string, unknown>;
  allowedRoles: string[];
  effects: string[];
  riskClass: "descriptive" | "analytical" | "operational" | "access" | "consequential";
  requiresConfirmation: boolean;
  requiresApproval: boolean;
  idempotencyRequired: boolean;
  emits: string[];
  execution?: AppActionExecution;
};

export type AppActionExecution = {
  kind: "ai_json";
  operation: "agent_reasoning" | "complex_agent_planning";
  systemPrompt: string;
  promptFields: string[];
  usageLimit?: {
    scope: "principal";
    limit: number;
    period: "lifetime";
  };
  maxOutputTokens?: number;
  generatedIdPaths?: string[];
  retrieval?: AppActionRetrieval;
  write?: {
    collection: string;
    ownerField?: string;
    createdAtField?: string;
    updatedAtField?: string;
  };
};

export type AppActionRetrievalSource = {
  collection: string;
  queryFields: string[];
  fields: string[];
  limit: number;
};

export type AppActionRetrieval = {
  /** Legacy single-source configuration. */
  collection?: string;
  queryFields?: string[];
  fields?: string[];
  limit?: number;
  /** Bounded multi-source retrieval. */
  sources?: AppActionRetrievalSource[];
  resultEnrichment?: {
    arrayPath: string;
    matchOutputField: string;
    matchContextField: string;
    fields: string[];
  };
};

export type AppActionUsage = {
  action: string;
  limit: number;
  used: number;
  remaining: number;
  period: "lifetime";
};

export type AppErrorDetail = {
  path: string;
  keyword: string;
  params?: Record<string, string | number | boolean>;
};

/** @deprecated Use AppAction. */
export type AppFunction = AppAction;

export type AppEntity = Record<string, unknown>;
export type AppAudience = Record<string, unknown>;
export type AppRole = Record<string, unknown>;
export type AppView = Record<string, unknown>;
export type AppPolicy = Record<string, unknown>;
export type AppDistribution = Record<string, unknown>;
export type AppPresentation = Record<string, unknown>;

export type AppManifest = {
  schemaVersion: "thingd.app/v1";
  version: string;
  project: { id: string; slug: string };
  app: { id: string; slug: string; name?: string };
  instance: { id: string; slug: string; name?: string };
  entities: AppEntity[];
  audiences: AppAudience[];
  roles: AppRole[];
  actions: AppAction[];
  /** @deprecated Use actions. */
  functions: AppAction[];
  views: AppView[];
  workflows: Record<string, unknown>[];
  integrations: Record<string, unknown>[];
  policies: AppPolicy;
  distribution: AppDistribution;
  presentation: AppPresentation;
  capabilities: { reads: boolean; namedWrites: boolean };
};

export type AppObject = {
  id: string;
  [key: string]: unknown;
};

export type AppObjectListOptions = {
  limit?: number;
  offset?: number;
};

export type AppSearchOptions = {
  collections?: string[];
  limit?: number;
};

export type AppSearchResult = {
  id: string;
  collection?: string;
  score: number;
  value: Record<string, unknown>;
};

// ── Events ───────────────────────────────────────────

export type MemoryEvent = {
  type: string;
  text?: string;
  [key: string]: unknown;
};

export type StoredMemoryEvent = MemoryEvent & {
  id: string;
  stream: string;
  sequence: number;
  createdAt: string;
};

export type ListEventsOptions = {
  fromSequence?: number;
  limit?: number;
  since?: string;
};

// ── Queues ───────────────────────────────────────────

export type QueueJobPayload = Record<string, unknown>;

export type QueueJobStatus = "ready" | "leased" | "completed" | "dead";

export type QueueJob = {
  id: string;
  queue: string;
  payload: QueueJobPayload;
  status: QueueJobStatus;
  attempts: number;
  maxAttempts: number;
  createdAt: string;
  availableAt: string;
  leasedAt?: string;
  leaseExpiresAt?: string;
  completedAt?: string;
  deadAt?: string;
  lastError?: string;
};

export type QueueJobOptions = {
  idempotencyKey?: string;
  maxAttempts?: number;
  delayMs?: number;
};

export type QueueClaimOptions = {
  leaseMs?: number;
};

export type QueueNackOptions = {
  delayMs?: number;
  error?: string;
};

export type QueueJobResult =
  | { ok: true; job: QueueJob }
  | { ok: false; reason: "not_found" | "not_leased" | "terminal" };

export type MemoryQueue = {
  push(payload: QueueJobPayload, options?: QueueJobOptions): Promise<QueueJob>;
  claim(options?: QueueClaimOptions): Promise<QueueJob | null>;
  ack(jobId: string): Promise<QueueJobResult>;
  nack(jobId: string, options?: QueueNackOptions): Promise<QueueJobResult>;
  list(): Promise<QueueJob[]>;
  dead(): Promise<QueueJob[]>;
};

// ── Search ───────────────────────────────────────────

export type MemorySearchOptions = {
  collections?: string[];
  limit?: number;
  filter?: Record<string, string>;
};

export type MemorySearchResult = {
  kind: "object" | "event";
  id: string;
  collection?: string;
  stream?: string;
  score: number;
  value: Record<string, unknown>;
};

export type VectorSearchOptions = {
  topK?: number;
  filter?: Record<string, unknown>;
};

export type VectorSearchHit = {
  id: string;
  score: number;
  value: Record<string, unknown>;
};

// ── Links ────────────────────────────────────────────

export type LinkDirection = "Outgoing" | "Incoming" | "Both";

export type Link = {
  id: string;
  fromRef: string;
  linkType: string;
  toRef: string;
  weight?: number;
  metadataJson: string;
  createdAt: string;
};

export type LinkQueryOptions = {
  linkType?: string;
  limit?: number;
};

// ── Aggregate ────────────────────────────────────────

export type AggregateOptions = {
  function: "count" | "sum" | "avg" | "min" | "max";
  field?: string;
  groupBy?: string;
  filter?: Record<string, string>;
};

export type AggregateGroupResult = {
  key: string;
  value: number;
};

export type AggregateResult = {
  total: number;
  groups: AggregateGroupResult[];
};

export type TimeBucket = "hour" | "day" | "week" | "month";

export type TimeSeriesOptions = {
  function: "count" | "sum" | "avg" | "min" | "max";
  field?: string;
  bucket: TimeBucket;
  from?: string;
  to?: string;
  filter?: Record<string, string>;
};

export type TimeSeriesBucket = {
  label: string;
  value: number;
};

export type TimeSeriesResult = {
  buckets: TimeSeriesBucket[];
};

// ── Schema ───────────────────────────────────────────

export type FieldSchema = {
  name: string;
  type: "string" | "number" | "boolean" | "date" | "null" | "unknown";
  nullable: boolean;
  sampleValues: unknown[];
};

export type CollectionSchema = {
  name: string;
  objectCount: number;
  fields: FieldSchema[];
};

export type SchemaOptions = {
  sampleSize?: number;
};

// ── NLQ ──────────────────────────────────────────────

export type NlqOptions = {
  collections?: string[];
  model?: string;
  maxTokens?: number;
};

export type NlqResult = {
  answer: string;
  sql?: string;
  data?: Record<string, unknown>[];
  error?: string;
};
