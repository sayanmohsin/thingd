import { randomUUID } from "node:crypto";
import { createRequire } from "node:module";
import type {
  ListEventsOptions,
  ListObjectsOptions,
  MemoryEvent,
  MemoryObject,
  MemorySearchOptions,
  MemorySearchResult,
  QueueClaimOptions,
  QueueJob,
  QueueJobOptions,
  QueueJobPayload,
  QueueJobResult,
  QueueNackOptions,
  StoredMemoryEvent,
  StoredMemoryObject,
  ThingDeleteResult,
  ThingStore,
} from "../types.js";

type NativeThingStoreBinding = {
  putObjectJson(collection: string, id: string, body: string): string;
  getObjectJson(collection: string, id: string): string | null;
  listObjectsJson(
    collectionsJson?: string,
    filterJson?: string,
    limit?: number,
    offset?: number
  ): string;
  deleteObject(collection: string, id: string): boolean;
  appendEventJson(stream: string, body: string): string;
  listEventsJson(stream?: string, fromSequence?: number, limit?: number): string;
  pushJobJson(
    queue: string,
    id: string,
    body: string,
    maxAttempts: number,
    delayMs: number
  ): string;
  claimJobJson(queue: string, leaseMs: number): string | null;
  ackJobJson(queue: string, id: string): string;
  nackJobJson(queue: string, id: string, delayMs: number, error?: string): string;
  listJobsJson(queue: string): string;
  listDeadJobsJson(queue: string): string;
  countObjectsJson(): Promise<number>;
  countEventsJson(): Promise<number>;
  countActiveJobsJson(): Promise<number>;
  countDeadJobsJson(): Promise<number>;
  listCollectionsJson(): Promise<string>;
  listStreamsJson(): Promise<string>;
  listQueuesJson(): Promise<string>;
  searchJson(query: string, collectionsJson?: string, limit?: number, filterJson?: string): string;
  putObjectsBatchJson(objectsJson: string): string;
  appendEventsBatchJson(eventsJson: string): string;
  pushJobsBatchJson(jobsJson: string): string;
};

type NativeThingStoreConstructor = {
  open(path: string): NativeThingStoreBinding;
};

type NativeThingStoreModule = {
  NativeThingStore: NativeThingStoreConstructor;
  loadedPath?: string;
};

type NativeObjectRecord = {
  collection: string;
  id: string;
  body: string;
  version: number;
  createdAt: string;
  updatedAt: string;
};

type NativeEventRecord = {
  stream: string;
  eventType: string;
  body: string;
  sequence: number;
  createdAt: string;
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
  createdAt: string;
  lastError: string;
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

type NativeSearchHit = {
  kind: "object" | "event";
  collection: string;
  id: string;
  text: string;
  score: number;
  body: string;
  version?: number;
  createdAt?: string;
  updatedAt?: string;
  eventType?: string;
};

const DEFAULT_LEASE_MS = 30_000;
const NATIVE_PACKAGE_NAME = "thingd-native";

export class NativeThingStore implements ThingStore {
  static async open(path: string): Promise<NativeThingStore> {
    const native = await loadNativeModule();
    return new NativeThingStore(native.NativeThingStore.open(path));
  }

  static async isAvailable(): Promise<boolean> {
    try {
      await loadNativeModule();
      return true;
    } catch {
      return false;
    }
  }

  static async getLoadedPath(): Promise<string | undefined> {
    try {
      const native = await loadNativeModule();
      return native.loadedPath;
    } catch {
      return undefined;
    }
  }

  private constructor(private readonly binding: NativeThingStoreBinding) {}

  async put(collection: string, object: MemoryObject): Promise<StoredMemoryObject> {
    const record = parseJson<NativeObjectRecord>(
      this.binding.putObjectJson(collection, object.id, JSON.stringify(object))
    );

    return objectFromNative(record);
  }

  async get<T = StoredMemoryObject>(collection: string, id: string): Promise<T | null> {
    const record = this.binding.getObjectJson(collection, id);

    return record ? (objectFromNative(parseJson<NativeObjectRecord>(record)) as T) : null;
  }

  async delete(collection: string, id: string): Promise<ThingDeleteResult> {
    return {
      deleted: this.binding.deleteObject(collection, id),
    };
  }

  async listObjects<T = StoredMemoryObject>(
    collection: string,
    options?: ListObjectsOptions
  ): Promise<T[]> {
    const collectionsJson = JSON.stringify([collection]);
    const filterJson = options?.filter ? JSON.stringify(options.filter) : undefined;
    return parseJson<NativeObjectRecord[]>(
      this.binding.listObjectsJson(collectionsJson, filterJson, options?.limit, options?.offset)
    ).map(objectFromNative) as T[];
  }

  async appendEvent(stream: string, event: MemoryEvent): Promise<StoredMemoryEvent> {
    const record = parseJson<NativeEventRecord>(
      this.binding.appendEventJson(stream, JSON.stringify(event))
    );

    return eventFromNative(record);
  }

  async listEvents<T = StoredMemoryEvent>(
    stream?: string,
    options?: ListEventsOptions
  ): Promise<T[]> {
    return parseJson<NativeEventRecord[]>(
      this.binding.listEventsJson(stream, options?.fromSequence, options?.limit)
    ).map(eventFromNative) as T[];
  }

  async pushJob(
    queue: string,
    payload: QueueJobPayload,
    options: QueueJobOptions = {}
  ): Promise<QueueJob> {
    const record = parseJson<NativeQueueJobRecord>(
      this.binding.pushJobJson(
        queue,
        options.idempotencyKey ?? randomUUID(),
        JSON.stringify(payload),
        options.maxAttempts ?? 3,
        options.delayMs ?? 0
      )
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
    options: QueueNackOptions = {}
  ): Promise<QueueJobResult> {
    const result = resultFromNative(
      parseJson<NativeQueueJobResult>(
        this.binding.nackJobJson(queue, jobId, options.delayMs ?? 0, options.error)
      )
    );

    return result;
  }

  async listJobs(queue: string): Promise<QueueJob[]> {
    return parseJson<NativeQueueJobRecord[]>(this.binding.listJobsJson(queue)).map(jobFromNative);
  }

  async listDeadJobs(queue: string): Promise<QueueJob[]> {
    return parseJson<NativeQueueJobRecord[]>(this.binding.listDeadJobsJson(queue)).map(
      jobFromNative
    );
  }

  async search(query: string, options: MemorySearchOptions = {}): Promise<MemorySearchResult[]> {
    const collectionsJson = options.collections ? JSON.stringify(options.collections) : undefined;
    const filterJson = options.filter ? JSON.stringify(options.filter) : undefined;

    const hits = parseJson<NativeSearchHit[]>(
      this.binding.searchJson(query, collectionsJson, options.limit, filterJson)
    );

    return hits.map((hit) => {
      if (hit.kind === "object") {
        const objectRecord: NativeObjectRecord = {
          collection: hit.collection,
          id: hit.id,
          body: hit.body,
          version: hit.version ?? 0,
          createdAt: hit.createdAt ?? "",
          updatedAt: hit.updatedAt ?? "",
        };
        const storedObject = objectFromNative(objectRecord);

        return {
          kind: "object",
          id: hit.id,
          collection: hit.collection,
          score: hit.score,
          value: storedObject,
        };
      } else {
        const eventRecord: NativeEventRecord = {
          stream: hit.collection,
          eventType: hit.eventType ?? "event",
          body: hit.body,
          sequence: Number(hit.id) || 0,
          createdAt: hit.createdAt ?? "",
        };
        const storedEvent = eventFromNative(eventRecord);

        return {
          kind: "event",
          id: hit.id,
          stream: hit.collection,
          score: hit.score,
          value: storedEvent,
        };
      }
    });
  }

  async putObjectsBatch(
    objects: Array<{ collection: string; id: string; body: string }>
  ): Promise<StoredMemoryObject[]> {
    return parseJson<NativeObjectRecord[]>(
      this.binding.putObjectsBatchJson(JSON.stringify(objects))
    ).map(objectFromNative);
  }

  async appendEventsBatch(
    events: Array<{ stream: string; eventType: string; body: string }>
  ): Promise<StoredMemoryEvent[]> {
    return parseJson<NativeEventRecord[]>(
      this.binding.appendEventsBatchJson(JSON.stringify(events))
    ).map(eventFromNative);
  }

  async pushJobsBatch(
    jobs: Array<{ queue: string; id: string; body: string; maxAttempts: number; delayMs: number }>
  ): Promise<QueueJob[]> {
    return parseJson<NativeQueueJobRecord[]>(
      this.binding.pushJobsBatchJson(JSON.stringify(jobs))
    ).map(jobFromNative);
  }

  async countObjects(): Promise<number> {
    return this.binding.countObjectsJson();
  }

  async countEvents(): Promise<number> {
    return this.binding.countEventsJson();
  }

  async countActiveJobs(): Promise<number> {
    return this.binding.countActiveJobsJson();
  }

  async countDeadJobs(): Promise<number> {
    return this.binding.countDeadJobsJson();
  }

  async listCollections(): Promise<string[]> {
    return parseJson<string[]>(await this.binding.listCollectionsJson());
  }

  async listStreams(): Promise<string[]> {
    return parseJson<string[]>(await this.binding.listStreamsJson());
  }

  async listQueues(): Promise<string[]> {
    return parseJson<string[]>(await this.binding.listQueuesJson());
  }
}

async function loadNativeModule(): Promise<NativeThingStoreModule> {
  const customPath = process.env.THINGD_NATIVE_PATH;
  if (customPath) {
    try {
      const require = createRequire(import.meta.url);
      const binding = require(customPath);
      return {
        NativeThingStore: binding.NativeThingStore,
        loadedPath: customPath,
      };
    } catch (error) {
      throw new Error(
        `Failed to load native store from THINGD_NATIVE_PATH="${customPath}": ${error instanceof Error ? error.message : String(error)}`
      );
    }
  }

  // Try direct import (resolves via node_modules when published, or workspace link locally)
  try {
    const mod = (await import(NATIVE_PACKAGE_NAME)) as NativeThingStoreModule;
    return {
      NativeThingStore: mod.NativeThingStore,
      loadedPath: mod.loadedPath,
    };
  } catch (importError) {
    // Fallback: scan workspace-relative paths for local development
    try {
      const { existsSync } = await import("node:fs");
      const { join, dirname } = await import("node:path");
      const { fileURLToPath } = await import("node:url");

      const __dirname = dirname(fileURLToPath(import.meta.url));
      const platform = process.platform;
      const arch = process.arch;

      const candidates = [
        // monorepo: packages/thingd/dist/stores/ -> ../../../../thingd-native/
        join(__dirname, "../../../../thingd-native/dist/thingd_native.node"),
        join(
          __dirname,
          "../../../../thingd-native/prebuilds",
          `${platform}-${arch}`,
          "thingd_native.node"
        ),
        // global install: sibling to thingd-cli in node_modules
        join(__dirname, "../../../../../../thingd-native/dist/thingd_native.node"),
        join(
          __dirname,
          "../../../../../../thingd-native/prebuilds",
          `${platform}-${arch}`,
          "thingd_native.node"
        ),
      ];

      for (const candidate of candidates) {
        if (existsSync(candidate)) {
          try {
            const require = createRequire(import.meta.url);
            const binding = require(candidate);
            if (binding?.NativeThingStore) {
              return {
                NativeThingStore: binding.NativeThingStore,
                loadedPath: candidate,
              };
            }
          } catch {
            // candidate failed to load, try next
          }
        }
      }
    } catch {
      // fallback resolution failed
    }

    throw new Error(
      `The native thingd driver is not available. Run "pnpm --filter thingd-native build" before using driver: "native". ${formatUnknownError(importError)}`
    );
  }
}

function objectFromNative(record: NativeObjectRecord): StoredMemoryObject {
  const value = parseJson<MemoryObject>(record.body);

  return {
    ...value,
    id: record.id,
    collection: record.collection,
    createdAt: record.createdAt ?? new Date().toISOString(),
    updatedAt: record.updatedAt ?? new Date().toISOString(),
    version: record.version,
  };
}

function eventFromNative(record: NativeEventRecord): StoredMemoryEvent {
  const value = parseJson<MemoryEvent>(record.body);

  return {
    ...value,
    type: value.type ?? record.eventType,
    id: String(record.sequence),
    sequence: record.sequence,
    stream: record.stream,
    createdAt: record.createdAt ?? new Date().toISOString(),
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
    createdAt: record.createdAt ?? new Date().toISOString(),
    availableAt: timestampToIso(record.availableAtMs),
    leasedAt: optionalTimestampToIso(record.leasedAtMs),
    leaseExpiresAt: optionalTimestampToIso(record.leaseExpiresAtMs),
    completedAt: optionalTimestampToIso(record.completedAtMs),
    deadAt: optionalTimestampToIso(record.deadAtMs),
    lastError: record.lastError || undefined,
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
  return `Original error: ${String(error)}`;
}
