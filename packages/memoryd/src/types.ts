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

export type QueueJobStatus = "ready" | "leased" | "completed" | "failed";

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
};

export type QueueJobOptions = {
  idempotencyKey?: string;
  maxAttempts?: number;
  delayMs?: number;
};

export type MemoryDeleteResult = {
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
  claim(): Promise<QueueJob | null>;
  list(): Promise<QueueJob[]>;
};

export interface MemoryStore {
  put(collection: string, object: MemoryObject): Promise<StoredMemoryObject>;
  get(collection: string, id: string): Promise<StoredMemoryObject | null>;
  delete(collection: string, id: string): Promise<MemoryDeleteResult>;
  appendEvent(stream: string, event: MemoryEvent): Promise<StoredMemoryEvent>;
  listEvents(stream?: string): Promise<StoredMemoryEvent[]>;
  pushJob(queue: string, payload: QueueJobPayload, options?: QueueJobOptions): Promise<QueueJob>;
  claimJob(queue: string): Promise<QueueJob | null>;
  listJobs(queue: string): Promise<QueueJob[]>;
  search(query: string, options?: MemorySearchOptions): Promise<MemorySearchResult[]>;
}
