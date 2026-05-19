import { randomUUID } from "node:crypto";
import type {
  MemoryDeleteResult,
  MemoryEvent,
  MemoryObject,
  MemorySearchOptions,
  MemorySearchResult,
  MemoryStore,
  QueueClaimOptions,
  QueueJob,
  QueueJobOptions,
  QueueJobPayload,
  QueueJobResult,
  QueueNackOptions,
  StoredMemoryEvent,
  StoredMemoryObject,
} from "../types.js";

type NativeMemoryStoreBinding = {
  putObjectJson(collection: string, id: string, body: string): string;
  getObjectJson(collection: string, id: string): string | null;
  listObjectsJson(collectionsJson?: string): string;
  deleteObject(collection: string, id: string): boolean;
  appendEventJson(stream: string, body: string): string;
  listEventsJson(stream?: string): string;
  pushJobJson(
    queue: string,
    id: string,
    body: string,
    maxAttempts: number,
    delayMs: number,
  ): string;
  claimJobJson(queue: string, leaseMs: number): string | null;
  ackJobJson(queue: string, id: string): string;
  nackJobJson(queue: string, id: string, delayMs: number): string;
  listJobsJson(queue: string): string;
  listDeadJobsJson(queue: string): string;
};

type NativeMemoryStoreConstructor = {
  open(path: string): NativeMemoryStoreBinding;
};

type NativeMemoryStoreModule = {
  NativeMemoryStore: NativeMemoryStoreConstructor;
};

type NativeObjectRecord = {
  collection: string;
  id: string;
  body: string;
  version: number;
};

type NativeEventRecord = {
  stream: string;
  eventType: string;
  body: string;
  sequence: number;
};

type NativeQueueJobRecord = {
  queue: string;
  id: string;
  body: string;
  status: QueueJob["status"];
  attempts: number;
  maxAttempts: number;
  availableAtMs: number;
  leasedAtMs?: number;
  leaseExpiresAtMs?: number;
  completedAtMs?: number;
  deadAtMs?: number;
};

type NativeQueueJobResult =
  | {
      ok: true;
      job: NativeQueueJobRecord;
    }
  | {
      ok: false;
      reason: "not_found" | "not_leased" | "terminal";
    };

const DEFAULT_LEASE_MS = 30_000;
const NATIVE_PACKAGE_NAME = "@sayanmohsin/memoryd-native";

export class NativeMemoryStore implements MemoryStore {
  static async open(path: string): Promise<NativeMemoryStore> {
    const native = await loadNativeModule();
    return new NativeMemoryStore(native.NativeMemoryStore.open(path));
  }

  private constructor(private readonly binding: NativeMemoryStoreBinding) {}

  async put(collection: string, object: MemoryObject): Promise<StoredMemoryObject> {
    const record = parseJson<NativeObjectRecord>(
      this.binding.putObjectJson(collection, object.id, JSON.stringify(object)),
    );

    return objectFromNative(record);
  }

  async get(collection: string, id: string): Promise<StoredMemoryObject | null> {
    const record = this.binding.getObjectJson(collection, id);

    return record ? objectFromNative(parseJson<NativeObjectRecord>(record)) : null;
  }

  async delete(collection: string, id: string): Promise<MemoryDeleteResult> {
    return {
      deleted: this.binding.deleteObject(collection, id),
    };
  }

  async appendEvent(stream: string, event: MemoryEvent): Promise<StoredMemoryEvent> {
    const record = parseJson<NativeEventRecord>(
      this.binding.appendEventJson(stream, JSON.stringify(event)),
    );

    return eventFromNative(record);
  }

  async listEvents(stream?: string): Promise<StoredMemoryEvent[]> {
    return parseJson<NativeEventRecord[]>(this.binding.listEventsJson(stream)).map(eventFromNative);
  }

  async pushJob(
    queue: string,
    payload: QueueJobPayload,
    options: QueueJobOptions = {},
  ): Promise<QueueJob> {
    const record = parseJson<NativeQueueJobRecord>(
      this.binding.pushJobJson(
        queue,
        options.idempotencyKey ?? randomUUID(),
        JSON.stringify(payload),
        options.maxAttempts ?? 3,
        options.delayMs ?? 0,
      ),
    );

    return jobFromNative(record);
  }

  async claimJob(queue: string, options: QueueClaimOptions = {}): Promise<QueueJob | null> {
    const record = this.binding.claimJobJson(queue, options.leaseMs ?? DEFAULT_LEASE_MS);

    return record ? jobFromNative(parseJson<NativeQueueJobRecord>(record)) : null;
  }

  async ackJob(queue: string, jobId: string): Promise<QueueJobResult> {
    return resultFromNative(parseJson<NativeQueueJobResult>(this.binding.ackJobJson(queue, jobId)));
  }

  async nackJob(
    queue: string,
    jobId: string,
    options: QueueNackOptions = {},
  ): Promise<QueueJobResult> {
    const result = resultFromNative(
      parseJson<NativeQueueJobResult>(this.binding.nackJobJson(queue, jobId, options.delayMs ?? 0)),
    );

    if (result.ok && options.error) {
      result.job.lastError = options.error;
    }

    return result;
  }

  async listJobs(queue: string): Promise<QueueJob[]> {
    return parseJson<NativeQueueJobRecord[]>(this.binding.listJobsJson(queue)).map(jobFromNative);
  }

  async listDeadJobs(queue: string): Promise<QueueJob[]> {
    return parseJson<NativeQueueJobRecord[]>(this.binding.listDeadJobsJson(queue)).map(
      jobFromNative,
    );
  }

  async search(query: string, options: MemorySearchOptions = {}): Promise<MemorySearchResult[]> {
    const normalizedQuery = query.toLowerCase();
    const collectionsJson = options.collections ? JSON.stringify(options.collections) : undefined;
    const objects = parseJson<NativeObjectRecord[]>(
      this.binding.listObjectsJson(collectionsJson),
    ).map(objectFromNative);
    const events = parseJson<NativeEventRecord[]>(this.binding.listEventsJson()).map(
      eventFromNative,
    );
    const results: MemorySearchResult[] = [];

    for (const object of objects) {
      const haystack = JSON.stringify(object).toLowerCase();
      if (haystack.includes(normalizedQuery)) {
        results.push({
          kind: "object",
          id: object.id,
          collection: object.collection,
          score: 1,
          value: object,
        });
      }
    }

    for (const event of events) {
      const haystack = JSON.stringify(event).toLowerCase();
      if (haystack.includes(normalizedQuery)) {
        results.push({
          kind: "event",
          id: event.id,
          stream: event.stream,
          score: 1,
          value: event,
        });
      }
    }

    return results.slice(0, options.limit ?? 10);
  }
}

async function loadNativeModule(): Promise<NativeMemoryStoreModule> {
  try {
    return (await import(NATIVE_PACKAGE_NAME)) as NativeMemoryStoreModule;
  } catch (error) {
    throw new Error(
      `The native memoryd driver is not available. Run "pnpm --filter @sayanmohsin/memoryd-native build" before using driver: "native". ${formatUnknownError(error)}`,
    );
  }
}

function objectFromNative(record: NativeObjectRecord): StoredMemoryObject {
  const value = parseJson<MemoryObject>(record.body);
  const now = new Date().toISOString();

  return {
    ...value,
    id: record.id,
    collection: record.collection,
    createdAt: now,
    updatedAt: now,
    version: record.version,
  };
}

function eventFromNative(record: NativeEventRecord): StoredMemoryEvent {
  const value = parseJson<MemoryEvent>(record.body);

  return {
    ...value,
    type: value.type ?? record.eventType,
    id: String(record.sequence),
    stream: record.stream,
    createdAt: new Date().toISOString(),
  };
}

function jobFromNative(record: NativeQueueJobRecord): QueueJob {
  return {
    id: record.id,
    queue: record.queue,
    payload: parseJson<QueueJobPayload>(record.body),
    status: record.status,
    attempts: record.attempts,
    maxAttempts: record.maxAttempts,
    createdAt: new Date().toISOString(),
    availableAt: timestampToIso(record.availableAtMs),
    leasedAt: optionalTimestampToIso(record.leasedAtMs),
    leaseExpiresAt: optionalTimestampToIso(record.leaseExpiresAtMs),
    completedAt: optionalTimestampToIso(record.completedAtMs),
    deadAt: optionalTimestampToIso(record.deadAtMs),
  };
}

function resultFromNative(result: NativeQueueJobResult): QueueJobResult {
  if (!result.ok) {
    return result;
  }

  return {
    ok: true,
    job: jobFromNative(result.job),
  };
}

function parseJson<T>(json: string): T {
  return JSON.parse(json) as T;
}

function timestampToIso(value: number): string {
  if (value <= 0) {
    return new Date().toISOString();
  }

  return new Date(value).toISOString();
}

function optionalTimestampToIso(value?: number): string | undefined {
  return value === undefined ? undefined : timestampToIso(value);
}

function formatUnknownError(error: unknown): string {
  if (error instanceof Error) {
    return `Original error: ${error.message}`;
  }

  return "";
}
