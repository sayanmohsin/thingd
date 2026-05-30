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
  createdAt: string;
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

export type MemoryQueue = {
  push(payload: QueueJobPayload, options?: QueueJobOptions): Promise<QueueJob>;
  claim(options?: QueueClaimOptions): Promise<QueueJob | null>;
  ack(jobId: string): Promise<QueueJobResult>;
  nack(jobId: string, options?: QueueNackOptions): Promise<QueueJobResult>;
  list(): Promise<QueueJob[]>;
  dead(): Promise<QueueJob[]>;
};

export interface ThingStore {
  put(collection: string, object: MemoryObject): Promise<StoredMemoryObject>;
  get(collection: string, id: string): Promise<StoredMemoryObject | null>;
  delete(collection: string, id: string): Promise<ThingDeleteResult>;
  listObjects?(collection: string): Promise<StoredMemoryObject[]>;
  appendEvent(stream: string, event: MemoryEvent): Promise<StoredMemoryEvent>;
  listEvents(stream?: string): Promise<StoredMemoryEvent[]>;
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
  listCollections?(): Promise<string[]>;
  listStreams?(): Promise<string[]>;
  listQueues?(): Promise<string[]>;
  close?(): Promise<void>;
}
