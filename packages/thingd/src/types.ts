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

export type ConnectorPingResult = {
  ok: boolean;
  connector: string;
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
};

export type SortDirection = "asc" | "desc";

export type SortBy = {
  field: "id" | "collection" | "created_at" | "updated_at" | "version";
  direction?: SortDirection;
};

export type ListObjectsOptions = {
  limit?: number;
  offset?: number;
  filter?: Record<string, unknown>;
  sortBy?: SortBy;
};

export type PutOptions = {
  /** Optional expected version for optimistic locking (CAS). */
  expectedVersion?: number;
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
  readonly events: {
    append(stream: string, event: MemoryEvent): Promise<StoredMemoryEvent>;
    list<T = StoredMemoryEvent>(stream?: string, options?: ListEventsOptions): Promise<T[]>;
  };
  queue(name: string): MemoryQueue;
  close(): Promise<void>;
  countObjects(): Promise<number>;
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
  listConnectors(): Promise<string[]>;
  discoverConnectorSchema(
    type: string,
    query: string,
    auth?: ConnectorAuth
  ): Promise<ConnectorSchema>;
  connectorSync(type: string, options: ConnectorSyncOptions): Promise<ConnectorSyncResult>;
  pingConnector(type: string, auth?: ConnectorAuth): Promise<ConnectorPingResult>;
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
  listCollections?(): Promise<string[]>;
  listStreams?(): Promise<string[]>;
  listQueues?(): Promise<string[]>;
  close?(): Promise<void>;
  backupTo?(path: string): void;
  walCheckpoint?(): { framesBefore: number; framesAfter: number };
  listConnectors?(): Promise<string[]>;
  discoverConnectorSchema?(
    type: string,
    query: string,
    auth?: ConnectorAuth
  ): Promise<ConnectorSchema>;
  connectorSync?(type: string, options: ConnectorSyncOptions): Promise<ConnectorSyncResult>;
  pingConnector?(type: string, auth?: ConnectorAuth): Promise<ConnectorPingResult>;
}
