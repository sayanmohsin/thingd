import { randomUUID } from "node:crypto";
import { createRequire } from "node:module";
import type {
  AggregateOptions,
  AggregateResult,
  CollectionSchema,
  ListEventsOptions,
  ListObjectsOptions,
  MemoryEvent,
  MemoryObject,
  MemorySearchOptions,
  MemorySearchResult,
  PutOptions,
  QueueClaimOptions,
  QueueJob,
  QueueJobOptions,
  QueueJobPayload,
  QueueJobResult,
  QueueNackOptions,
  SchemaOptions,
  StoredMemoryEvent,
  StoredMemoryObject,
  ThingDeleteResult,
  ThingStore,
  TimeSeriesOptions,
  TimeSeriesResult,
  VectorSearchHit,
  VectorSearchOptions,
} from "../types.js";

type NativeThingStoreBinding = {
  close(): void;
  putObjectJson(collection: string, id: string, body: string, expectedVersion?: number): string;
  optimizeSearchIndex(): void;
  getObjectJson(collection: string, id: string): string | null;
  getObjectsBatchJson(collection: string, ids: string[]): string;
  listObjectsJson(
    collectionsJson?: string,
    filterJson?: string,
    limit?: number,
    offset?: number,
    sortField?: string,
    sortDirection?: string
  ): Promise<string>;
  deleteObject(collection: string, id: string): boolean;
  appendEventJson(stream: string, body: string): string;
  listEventsJson(stream?: string, fromSequence?: number, limit?: number, since?: string): string;
  pushJobJson(
    queue: string,
    id: string,
    body: string,
    maxAttempts: number,
    delayMs: number,
    priority?: number
  ): string;
  claimJobJson(queue: string, leaseMs: number): string | null;
  ackJobJson(queue: string, id: string): string;
  nackJobJson(queue: string, id: string, delayMs: number, error?: string): string;
  listJobsJson(queue: string): string;
  listDeadJobsJson(queue: string): string;
  countObjectsJson(): Promise<number>;
  countObjectsInCollectionJson(collection: string): Promise<number>;
  countEventsJson(): Promise<number>;
  countActiveJobsJson(): Promise<number>;
  countDeadJobsJson(): Promise<number>;
  countLinksJson(): Promise<number>;
  listCollectionsJson(): Promise<string>;
  listStreamsJson(): Promise<string>;
  listQueuesJson(): Promise<string>;
  createLinkJson(
    fromRef: string,
    linkType: string,
    toRef: string,
    weight?: number,
    metadataJson?: string
  ): string;
  deleteLink(id: string): boolean;
  getLinkJson(id: string): string | null;
  getNeighborsJson(reference: string, direction: string, linkType?: string, limit?: number): string;
  searchJson(
    query: string,
    collectionsJson?: string,
    limit?: number,
    filterJson?: string
  ): Promise<string>;
  putObjectsBatchJson(objectsJson: string): string;
  appendEventsBatchJson(eventsJson: string): string;
  pushJobsBatchJson(jobsJson: string): string;
  deleteObjectsBatchJson(keysJson: string): number;
  aggregateJson(
    collection: string,
    function_: string,
    field?: string,
    groupBy?: string,
    filterJson?: string
  ): string;
  timeseriesJson(
    collection: string,
    function_: string,
    field?: string,
    bucket?: string,
    from?: string,
    to?: string,
    filterJson?: string
  ): string;
  schemaJson(collection?: string, sampleSize?: number): string;
  getSchemaDocumentJson(): string | null;
  putSchemaDocumentJson(schemaJson: string, hash: string, updatedAt: string): void;
  listMigrationsJson(): string;
  recordMigrationJson(id: string, hash: string, appliedAt: string): void;
  createIndexJson(collection: string, field: string): void;
  createUniqueIndexJson(collection: string, field: string): void;
  deleteIndexJson(collection: string, field: string): boolean;
  listIndexesJson(): string;
  vectorSearchJson(
    collection: string,
    queryVectorJson: string,
    topK?: number,
    filterJson?: string
  ): string;
};

type NativeThingStoreConstructor = {
  open(path: string, encryptionKey?: string): NativeThingStoreBinding;
};

type NativeThingStoreModule = {
  NativeThingStore: NativeThingStoreConstructor;
  parseSchema(source: string): string;
  reencrypt(
    sourcePath: string,
    destinationPath: string,
    sourceKey?: string,
    destinationKey?: string,
    allowPlaintextOutput?: boolean
  ): void;
  loadedPath?: string;
};

/** Result returned after parsing and hashing a schema document. */
export type SchemaDocument = {
  schema: unknown;
  hash: string;
};

/** Persisted canonical schema metadata. */
export type StoredSchema = {
  schemaJson: string;
  hash: string;
  updatedAt: string;
};

/** Durable record of an applied migration. */
export type MigrationRecord = {
  id: string;
  hash: string;
  appliedAt: string;
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
  priority?: number;
};

type NativeLinkRecord = {
  id: string;
  fromRef: string;
  linkType: string;
  toRef: string;
  weight?: number;
  metadataJson: string;
  createdAt: string;
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

type NativeVectorValue = {
  key: { collection: string; id: string };
  body: string;
  version: number;
  createdAt: string;
  updatedAt: string;
  vector?: number[];
};

type NativeVectorSearchHit = {
  id: string;
  score: number;
  value: NativeVectorValue;
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
const NATIVE_PACKAGE_NAME = "@thingd/native";

export class NativeThingStore implements ThingStore {
  static async parseSchema(source: string): Promise<SchemaDocument> {
    const native = await loadNativeModule();
    return parseJson<SchemaDocument>(native.parseSchema(source));
  }

  static async open(path: string, encryptionKey?: string): Promise<NativeThingStore> {
    const native = await loadNativeModule();
    return new NativeThingStore(native.NativeThingStore.open(path, encryptionKey));
  }

  static async isAvailable(): Promise<boolean> {
    try {
      await loadNativeModule();
      return true;
    } catch {
      return false;
    }
  }

  static async reencrypt(
    sourcePath: string,
    destinationPath: string,
    sourceKey?: string,
    destinationKey?: string,
    allowPlaintextOutput = false
  ): Promise<void> {
    const native = await loadNativeModule();
    native.reencrypt(sourcePath, destinationPath, sourceKey, destinationKey, allowPlaintextOutput);
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

  async close(): Promise<void> {
    this.binding.close();
  }

  async put(
    collection: string,
    object: MemoryObject,
    options?: PutOptions
  ): Promise<StoredMemoryObject> {
    const record = parseJson<NativeObjectRecord>(
      this.binding.putObjectJson(
        collection,
        object.id,
        JSON.stringify(object),
        options?.expectedVersion
      )
    );

    return objectFromNative(record);
  }

  async get<T = StoredMemoryObject>(collection: string, id: string): Promise<T | null> {
    const record = this.binding.getObjectJson(collection, id);

    return record ? (objectFromNative(parseJson<NativeObjectRecord>(record)) as T) : null;
  }

  async getBatch<T = StoredMemoryObject>(collection: string, ids: string[]): Promise<(T | null)[]> {
    return parseJson<(NativeObjectRecord | null)[]>(
      this.binding.getObjectsBatchJson(collection, ids)
    ).map((record) => (record ? (objectFromNative(record) as T) : null));
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
    const filterJson = serializeFilter(options?.filter);
    const sortField = options?.sortBy?.field;
    const sortDirection = options?.sortBy?.direction;
    return parseJson<NativeObjectRecord[]>(
      await this.binding.listObjectsJson(
        collectionsJson,
        filterJson,
        options?.limit,
        options?.offset,
        sortField,
        sortDirection
      )
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
      this.binding.listEventsJson(stream, options?.fromSequence, options?.limit, options?.since)
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
        options.delayMs ?? 0,
        options.priority
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

  async vectorSearch(
    collection: string,
    queryVector: number[],
    options: VectorSearchOptions = {}
  ): Promise<VectorSearchHit[]> {
    const filterJson = serializeFilter(options.filter);

    const hits = parseJson<NativeVectorSearchHit[]>(
      this.binding.vectorSearchJson(
        collection,
        JSON.stringify(queryVector),
        options.topK,
        filterJson
      )
    );

    return hits.map((hit) => {
      const value = parseJson<MemoryObject>(hit.value.body);
      const storedObject: StoredMemoryObject = {
        ...value,
        id: hit.value.key.id,
        collection: hit.value.key.collection,
        createdAt: hit.value.createdAt,
        updatedAt: hit.value.updatedAt,
        version: hit.value.version,
      };
      if (hit.value.vector) {
        storedObject.vector = hit.value.vector;
      }
      return { id: hit.id, score: hit.score, value: storedObject };
    });
  }

  async search(query: string, options: MemorySearchOptions = {}): Promise<MemorySearchResult[]> {
    const collectionsJson = options.collections ? JSON.stringify(options.collections) : undefined;
    const filterJson = serializeFilter(options.filter);

    const hits = parseJson<NativeSearchHit[]>(
      await this.binding.searchJson(query, collectionsJson, options.limit, filterJson)
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

  async countObjectsInCollection(collection: string): Promise<number> {
    return this.binding.countObjectsInCollectionJson(collection);
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

  async countLinks(): Promise<number> {
    return this.binding.countLinksJson();
  }

  async putBatch(collection: string, objects: MemoryObject[]): Promise<StoredMemoryObject[]> {
    const inputs = objects.map((obj) => ({ collection, id: obj.id, body: JSON.stringify(obj) }));
    return parseJson<NativeObjectRecord[]>(
      this.binding.putObjectsBatchJson(JSON.stringify(inputs))
    ).map(objectFromNative);
  }

  async deleteBatch(collection: string, ids: string[]): Promise<number> {
    const keys = ids.map((id) => [collection, id]);
    return this.binding.deleteObjectsBatchJson(JSON.stringify(keys));
  }

  async createLink(
    fromRef: string,
    linkType: string,
    toRef: string,
    weight?: number,
    metadataJson?: string
  ): Promise<import("../types.js").Link> {
    return linkFromNative(
      parseJson<NativeLinkRecord>(
        this.binding.createLinkJson(fromRef, linkType, toRef, weight, metadataJson)
      )
    );
  }

  async deleteLink(id: string): Promise<boolean> {
    return this.binding.deleteLink(id);
  }

  async getLink(id: string): Promise<import("../types.js").Link | null> {
    const record = this.binding.getLinkJson(id);
    return record ? linkFromNative(parseJson<NativeLinkRecord>(record)) : null;
  }

  async getNeighbors(
    reference: string,
    direction: import("../types.js").LinkDirection,
    options: import("../types.js").LinkQueryOptions
  ): Promise<import("../types.js").Link[]> {
    return parseJson<NativeLinkRecord[]>(
      this.binding.getNeighborsJson(reference, direction, options.linkType, options.limit)
    ).map(linkFromNative);
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

  async createIndex(collection: string, field: string): Promise<void> {
    this.binding.createIndexJson(collection, field);
  }

  async createUniqueIndex(collection: string, field: string): Promise<void> {
    this.binding.createUniqueIndexJson(collection, field);
  }

  async deleteIndex(collection: string, field: string): Promise<boolean> {
    return this.binding.deleteIndexJson(collection, field);
  }

  async listIndexes(): Promise<Array<[string, string]>> {
    return parseJson<Array<[string, string]>>(this.binding.listIndexesJson());
  }

  async aggregate(collection: string, options: AggregateOptions): Promise<AggregateResult> {
    const filterJson = serializeFilter(options.filter);
    return parseJson<AggregateResult>(
      this.binding.aggregateJson(
        collection,
        options.function,
        options.field,
        options.groupBy,
        filterJson
      )
    );
  }

  async timeseries(collection: string, options: TimeSeriesOptions): Promise<TimeSeriesResult> {
    const filterJson = serializeFilter(options.filter);
    return parseJson<TimeSeriesResult>(
      this.binding.timeseriesJson(
        collection,
        options.function,
        options.field,
        options.bucket,
        options.from,
        options.to,
        filterJson
      )
    );
  }

  async schema(collection?: string, options?: SchemaOptions): Promise<CollectionSchema[]> {
    const sampleSize = options?.sampleSize ?? 50;
    return parseJson<CollectionSchema[]>(this.binding.schemaJson(collection, sampleSize));
  }

  async validateSchema(source: string): Promise<SchemaDocument> {
    return NativeThingStore.parseSchema(source);
  }

  async getSchemaDocument(): Promise<StoredSchema | null> {
    const value = this.binding.getSchemaDocumentJson();
    return value ? parseJson<StoredSchema>(value) : null;
  }

  async putSchemaDocument(schema: StoredSchema): Promise<void> {
    this.binding.putSchemaDocumentJson(schema.schemaJson, schema.hash, schema.updatedAt);
  }

  async listMigrations(): Promise<MigrationRecord[]> {
    return parseJson<MigrationRecord[]>(this.binding.listMigrationsJson());
  }

  async recordMigration(migration: MigrationRecord): Promise<void> {
    this.binding.recordMigrationJson(migration.id, migration.hash, migration.appliedAt);
  }
}

/** Parse a `schema.thingd` source document with the native Rust parser. */
export async function parseSchema(source: string): Promise<SchemaDocument> {
  return NativeThingStore.parseSchema(source);
}

async function loadNativeModule(): Promise<NativeThingStoreModule> {
  const customPath = process.env.THINGD_NATIVE_PATH;
  if (customPath) {
    try {
      const require = createRequire(import.meta.url);
      const binding = require(customPath);
      return {
        NativeThingStore: binding.NativeThingStore,
        parseSchema: binding.parseSchema,
        reencrypt: binding.reencrypt,
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
      parseSchema: mod.parseSchema,
      reencrypt: mod.reencrypt,
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
                parseSchema: binding.parseSchema,
                reencrypt: binding.reencrypt,
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
      `The native thingd driver is not available. Install @thingd/native with "npm install @thingd/native" or set THINGD_NATIVE_PATH. For monorepo development: "pnpm --filter thingd-native build". ${formatUnknownError(importError)}`
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
    priority: record.priority ?? 0,
  };
}

function linkFromNative(record: NativeLinkRecord): import("../types.js").Link {
  return {
    ...record,
    createdAt: record.createdAt ?? new Date().toISOString(),
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

function serializeFilter(filter: Record<string, unknown> | undefined): string | undefined {
  if (!filter) {
    return undefined;
  }
  const cleaned: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(filter)) {
    if (value !== undefined) {
      cleaned[key] = value;
    }
  }
  if (Object.keys(cleaned).length === 0) {
    return undefined;
  }
  return JSON.stringify(cleaned);
}
