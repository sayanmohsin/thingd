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
  idempotencyKey?: string;
};

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
  /** Priority for claim ordering (higher = claimed sooner). Default: 0. */
  priority?: number;
};

export type QueueJobOptions = {
  idempotencyKey?: string;
  maxAttempts?: number;
  delayMs?: number;
  /** Priority for claim ordering (higher = claimed sooner). Default: 0. */
  priority?: number;
};

export type QueueClaimOptions = {
  leaseMs?: number;
};

export type QueueNackOptions = {
  delayMs?: number;
  error?: string;
};

export type QueueJobResult =
  | {
      ok: true;
      job: QueueJob;
    }
  | {
      ok: false;
      reason: "not_found" | "not_leased" | "terminal";
    };

export type ThingDeleteResult = {
  deleted: boolean;
};

export type MemorySearchOptions = {
  collections?: string[];
  limit?: number;
  filter?: Record<string, unknown>;
};

export type MemorySearchResult =
  | {
      kind: "object";
      id: string;
      collection: string;
      score: number;
      value: StoredMemoryObject;
    }
  | {
      kind: "event";
      id: string;
      stream: string;
      score: number;
      value: StoredMemoryEvent;
    };

export type VectorSearchOptions = {
  topK?: number;
  filter?: Record<string, unknown>;
};

export type VectorSearchHit = {
  id: string;
  score: number;
  value: StoredMemoryObject;
};

export type LinkDirection = "Outgoing" | "Incoming" | "Both";

export type LinkQueryOptions = {
  linkType?: string;
  limit?: number;
};

export type Link = {
  id: string;
  fromRef: string;
  linkType: string;
  toRef: string;
  weight?: number;
  metadataJson: string;
  createdAt: string;
};

export type ConnectorAuth = {
  host: string;
  port: number;
  database: string;
  username: string;
  password: string;
  sslMode?: "disable" | "prefer" | "require";
};

export type ConnectorSchema = {
  name: string;
  columns: {
    name: string;
    dataType: "text" | "integer" | "float" | "boolean" | "timestamp" | "json" | "unknown";
    nullable: boolean;
    sampleValues: unknown[];
  }[];
  estimatedRows: number | null;
};

export type ConnectorSyncResult = {
  imported: number;
  collection: string;
};

export type ConnectorSyncOptions = {
  auth?: ConnectorAuth;
  source?: string;
  collection: string;
  query: string;
  batchSize?: number;
  columnMapping?: Record<string, string>;
  syncStrategy?: "full" | "incremental";
};

export type MemoryQueue = {
  push(payload: QueueJobPayload, options?: QueueJobOptions): Promise<QueueJob>;
  claim(options?: QueueClaimOptions): Promise<QueueJob | null>;
  ack(jobId: string): Promise<QueueJobResult>;
  nack(jobId: string, options?: QueueNackOptions): Promise<QueueJobResult>;
  list(): Promise<QueueJob[]>;
  dead(): Promise<QueueJob[]>;
};

export type ListEventsOptions = {
  fromSequence?: number;
  limit?: number;
  since?: string;
};

export type SortDirection = "asc" | "desc";

export type SortBy = {
  /** Column name or JSON path like "$.price" for body field sorting */
  field: string;
  direction?: SortDirection;
};

export type FilterOperator = {
  $gt?: unknown;
  $gte?: unknown;
  $lt?: unknown;
  $lte?: unknown;
  $ne?: unknown;
  $in?: unknown[];
  $like?: string;
};

export type ListObjectsOptions = {
  limit?: number;
  offset?: number;
  filter?: Record<string, unknown | FilterOperator>;
  sortBy?: SortBy;
};

export type PutOptions = {
  /** Optional expected version for optimistic locking (CAS). */
  expectedVersion?: number;
};

export type AggregateFunction = "count" | "sum" | "avg" | "min" | "max";

export type AggregateOptions = {
  function: AggregateFunction;
  field?: string;
  groupBy?: string;
  filter?: Record<string, unknown>;
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
  function: AggregateFunction;
  field?: string;
  bucket: TimeBucket;
  from?: string;
  to?: string;
  filter?: Record<string, unknown>;
};

export type TimeSeriesBucket = {
  label: string;
  value: number;
};

export type TimeSeriesResult = {
  buckets: TimeSeriesBucket[];
};

export type FieldSchema = {
  name: string;
  type: string;
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

export type NlqIntent = {
  action: string;
  collection: string;
  function?: string;
  field?: string;
  groupBy?: string;
  bucket?: string;
  query?: string;
  limit?: number;
};

export type NlqResult = {
  answer: string;
  data: unknown;
  intent: NlqIntent;
};

export type NlqOptions = {
  collection?: string;
  model?: string;
  endpoint?: string;
  apiKey?: string;
};

// ── Scheduler types ──

export type Schedule = {
  id: string;
  expression: string;
  timezone?: string;
  payload: Record<string, unknown>;
  enabled: boolean;
  nextRunAt: string;
  lastRunAt?: string;
  lastStatus?: "completed" | "failed" | "running";
  lastError?: string;
  lastDurationMs?: number;
  runCount: number;
  failCount: number;
  consecutiveFails: number;
  maxConsecutiveFails: number;
  createdAt: string;
  updatedAt: string;
  metadata?: Record<string, unknown>;
};

export type ScheduleHandler = (schedule: Schedule, context: ScheduleContext) => Promise<void>;

export type ScheduleContext = {
  log: (message: string) => void;
  fail: (error: string) => void;
};

export type ScheduleOptions = {
  expression?: string;
  intervalMs?: number;
  timezone?: string;
  payload?: Record<string, unknown>;
  enabled?: boolean;
  maxConsecutiveFails?: number;
  metadata?: Record<string, unknown>;
  handler: ScheduleHandler;
};

export type ScheduleOnceOptions = {
  runAt: string;
  payload?: Record<string, unknown>;
  metadata?: Record<string, unknown>;
  handler: ScheduleHandler;
};

export type ScheduleIntervalOptions = {
  intervalMs: number;
  payload?: Record<string, unknown>;
  enabled?: boolean;
  maxConsecutiveFails?: number;
  metadata?: Record<string, unknown>;
  handler: ScheduleHandler;
};

export type ScheduleEvent = {
  scheduleId: string;
  expression: string;
  status: "started" | "completed" | "failed" | "disabled";
  timestamp: string;
  durationMs?: number;
  error?: string;
  runCount: number;
  failCount: number;
};

export type SchedulerStats = {
  total: number;
  enabled: number;
  disabled: number;
  running: number;
  nextRun: { id: string; at: string } | null;
};

export type SchedulerEventType = "started" | "completed" | "failed" | "disabled";

export type SchedulerListener = (event: ScheduleEvent) => void;

export type SchedulerFacade = {
  schedule(id: string, options: ScheduleOptions): Promise<Schedule>;
  scheduleOnce(id: string, options: ScheduleOnceOptions): Promise<Schedule>;
  scheduleInterval(id: string, options: ScheduleIntervalOptions): Promise<Schedule>;
  get(id: string): Promise<Schedule | null>;
  list(): Promise<Schedule[]>;
  pause(id: string): Promise<Schedule>;
  resume(id: string): Promise<Schedule>;
  remove(id: string): Promise<boolean>;
  run(id: string): Promise<void>;
  stats(): Promise<SchedulerStats>;
  start(): Promise<void>;
  stop(): Promise<void>;
  on(event: SchedulerEventType, listener: SchedulerListener): void;
  off(event: SchedulerEventType, listener: SchedulerListener): void;
};

/**
 * Typed interface for a thingd database connection returned by `ThingD.open()`.
 * Consumers can use this for type-safe dependency injection instead of `any`:
 *
 * ```ts
 * import type { ThingDConnection } from "@thingd/sdk";
 * private readonly db: ThingDConnection;
 * ```
 */
export interface ThingDConnection {
  put(collection: string, object: MemoryObject, options?: PutOptions): Promise<StoredMemoryObject>;
  get<T = StoredMemoryObject>(collection: string, id: string): Promise<T | null>;
  delete(collection: string, id: string): Promise<ThingDeleteResult>;
  listObjects<T = StoredMemoryObject>(
    collection: string,
    options?: ListObjectsOptions
  ): Promise<T[]>;
  search(query: string, options?: MemorySearchOptions): Promise<MemorySearchResult[]>;
  searchObjects<T = StoredMemoryObject>(query: string, options?: MemorySearchOptions): Promise<T[]>;
  putBatch(collection: string, objects: MemoryObject[]): Promise<StoredMemoryObject[]>;
  deleteBatch(collection: string, ids: string[]): Promise<number>;
  getBatch<T = StoredMemoryObject>(collection: string, ids: string[]): Promise<(T | null)[]>;
  readonly events: {
    append(stream: string, event: MemoryEvent): Promise<StoredMemoryEvent>;
    list<T = StoredMemoryEvent>(stream?: string, options?: ListEventsOptions): Promise<T[]>;
  };
  queue(name: string): MemoryQueue;
  close(): Promise<void>;
  countObjects(): Promise<number>;
  countObjectsInCollection(collection: string): Promise<number>;
  countEvents(): Promise<number>;
  countActiveJobs(): Promise<number>;
  countDeadJobs(): Promise<number>;
  countLinks(): Promise<number>;
  readonly links: {
    create(
      fromRef: string,
      linkType: string,
      toRef: string,
      weight?: number,
      metadataJson?: string
    ): Promise<Link>;
    delete(id: string): Promise<boolean>;
    get(id: string): Promise<Link | null>;
    neighbors(
      reference: string,
      direction?: LinkDirection,
      options?: LinkQueryOptions
    ): Promise<Link[]>;
  };
  listCollections(): Promise<string[]>;
  listStreams(): Promise<string[]>;
  listQueues(): Promise<string[]>;
  createIndex(collection: string, field: string): Promise<void>;
  listIndexes(): Promise<Array<[string, string]>>;
  listConnectors(): Promise<string[]>;
  discoverConnectorSchema(
    type: string,
    query: string,
    auth?: ConnectorAuth
  ): Promise<ConnectorSchema>;
  connectorSync(type: string, options: ConnectorSyncOptions): Promise<ConnectorSyncResult>;
  schema(collection?: string, options?: SchemaOptions): Promise<CollectionSchema[]>;
  readonly nlq: {
    query(question: string, options?: NlqOptions): Promise<NlqResult>;
  };
  readonly scheduler: SchedulerFacade;
  readonly aggregate: {
    count(
      collection: string,
      options?: Omit<AggregateOptions, "function">
    ): Promise<AggregateResult>;
    sum(
      collection: string,
      field: string,
      options?: Omit<AggregateOptions, "function" | "field">
    ): Promise<AggregateResult>;
    avg(
      collection: string,
      field: string,
      options?: Omit<AggregateOptions, "function" | "field">
    ): Promise<AggregateResult>;
    min(
      collection: string,
      field: string,
      options?: Omit<AggregateOptions, "function" | "field">
    ): Promise<AggregateResult>;
    max(
      collection: string,
      field: string,
      options?: Omit<AggregateOptions, "function" | "field">
    ): Promise<AggregateResult>;
  };
  timeseries(collection: string, options: TimeSeriesOptions): Promise<TimeSeriesResult>;
  vectorSearch(
    collection: string,
    queryVector: number[],
    options?: VectorSearchOptions
  ): Promise<VectorSearchHit[]>;
}

export interface ThingStore {
  put(collection: string, object: MemoryObject, options?: PutOptions): Promise<StoredMemoryObject>;
  get<T = StoredMemoryObject>(collection: string, id: string): Promise<T | null>;
  delete(collection: string, id: string): Promise<ThingDeleteResult>;
  listObjects<T = StoredMemoryObject>(
    collection: string,
    options?: ListObjectsOptions
  ): Promise<T[]>;
  appendEvent(stream: string, event: MemoryEvent): Promise<StoredMemoryEvent>;
  listEvents<T = StoredMemoryEvent>(stream?: string, options?: ListEventsOptions): Promise<T[]>;
  pushJob(queue: string, payload: QueueJobPayload, options?: QueueJobOptions): Promise<QueueJob>;
  claimJob(queue: string, options?: QueueClaimOptions): Promise<QueueJob | null>;
  ackJob(queue: string, jobId: string): Promise<QueueJobResult>;
  nackJob(queue: string, jobId: string, options?: QueueNackOptions): Promise<QueueJobResult>;
  listJobs(queue: string): Promise<QueueJob[]>;
  listDeadJobs(queue: string): Promise<QueueJob[]>;
  search(query: string, options?: MemorySearchOptions): Promise<MemorySearchResult[]>;
  countObjects?(): Promise<number>;
  countObjectsInCollection?(collection: string): Promise<number>;
  countEvents?(): Promise<number>;
  countActiveJobs?(): Promise<number>;
  countDeadJobs?(): Promise<number>;
  countLinks?(): Promise<number>;
  createLink?(
    fromRef: string,
    linkType: string,
    toRef: string,
    weight?: number,
    metadataJson?: string
  ): Promise<Link>;
  deleteLink?(id: string): Promise<boolean>;
  getLink?(id: string): Promise<Link | null>;
  getNeighbors?(
    reference: string,
    direction: LinkDirection,
    options: LinkQueryOptions
  ): Promise<Link[]>;
  putBatch?(collection: string, objects: MemoryObject[]): Promise<StoredMemoryObject[]>;
  deleteBatch?(collection: string, ids: string[]): Promise<number>;
  getBatch?(collection: string, ids: string[]): Promise<(StoredMemoryObject | null)[]>;
  listCollections?(): Promise<string[]>;
  listStreams?(): Promise<string[]>;
  listQueues?(): Promise<string[]>;
  createIndex?(collection: string, field: string): Promise<void>;
  listIndexes?(): Promise<Array<[string, string]>>;
  listConnectors?(): Promise<string[]>;
  discoverConnectorSchema?(
    type: string,
    query: string,
    auth?: ConnectorAuth
  ): Promise<ConnectorSchema>;
  connectorSync?(type: string, options: ConnectorSyncOptions): Promise<ConnectorSyncResult>;
  aggregate?(collection: string, options: AggregateOptions): Promise<AggregateResult>;
  timeseries?(collection: string, options: TimeSeriesOptions): Promise<TimeSeriesResult>;
  schema?(collection?: string, options?: SchemaOptions): Promise<CollectionSchema[]>;
  nlqQuery?(question: string, options?: NlqOptions): Promise<NlqResult>;
  vectorSearch?(
    collection: string,
    queryVector: number[],
    options?: VectorSearchOptions
  ): Promise<VectorSearchHit[]>;
  close?(): Promise<void>;
  backupTo?(path: string): void;
  walCheckpoint?(): { framesBefore: number; framesAfter: number };
}
