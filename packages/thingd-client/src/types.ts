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
