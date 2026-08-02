import { HttpThingStore } from "./client/http-thing-store.js";
import { Scheduler } from "./scheduler.js";
import { InMemoryThingStore } from "./stores/in-memory-thing-store.js";
import { NativeThingStore } from "./stores/native-thing-store.js";
import type {
  AggregateOptions,
  AggregateResult,
  CollectionSchema,
  ConnectorAuth,
  ConnectorSchema,
  ConnectorSyncOptions,
  ConnectorSyncResult,
  ListEventsOptions,
  ListObjectsOptions,
  LocalThingDConnection,
  MemoryEvent,
  MemoryObject,
  MemoryQueue,
  MemorySearchOptions,
  MemorySearchResult,
  NlqOptions,
  NlqResult,
  PutOptions,
  QueueClaimOptions,
  QueueJobOptions,
  QueueJobPayload,
  QueueNackOptions,
  SchedulerFacade,
  SchemaOptions,
  StoredMemoryEvent,
  StoredMemoryObject,
  ThingDeleteResult,
  ThingStore,
  TimeSeriesOptions,
  TimeSeriesResult,
  VectorSearchHit,
  VectorSearchOptions,
} from "./types.js";

export type ThingDDriver = "memory" | "native" | "cloud";

export type ThingDOpenOptions = {
  driver?: ThingDDriver;
  store?: ThingStore;
  authToken?: string;
  /** Alias for authToken. If both are set, authToken takes precedence. */
  apiKey?: string;
  /** Cloud instance slug for multi-instance routing. Passed as X-Instance-Slug header. */
  instanceSlug?: string;
};

export type ThingDOpenConfig = ThingDOpenOptions & {
  path?: string;
  url?: string;
};

export class ThingD implements LocalThingDConnection {
  static async open(
    pathOrConfig?: string | ThingDOpenConfig,
    options: ThingDOpenOptions = {}
  ): Promise<ThingD> {
    const resolvedOptions = resolveOpenOptions(pathOrConfig, options);

    return new ThingD(resolvedOptions.path, await openStore(resolvedOptions.path, resolvedOptions));
  }

  readonly scheduler: SchedulerFacade;

  private constructor(
    readonly path: string,
    private readonly store: ThingStore
  ) {
    this.scheduler = new Scheduler(store);
  }

  put(collection: string, object: MemoryObject, options?: PutOptions): Promise<StoredMemoryObject> {
    return this.store.put(collection, object, options);
  }

  get<T = StoredMemoryObject>(collection: string, id: string): Promise<T | null> {
    return this.store.get(collection, id);
  }

  delete(collection: string, id: string): Promise<ThingDeleteResult> {
    return this.store.delete(collection, id);
  }

  listObjects<T = StoredMemoryObject>(
    collection: string,
    options?: ListObjectsOptions
  ): Promise<T[]> {
    return this.store.listObjects?.<T>(collection, options) ?? Promise.resolve([]);
  }

  search(query: string, options: MemorySearchOptions = {}): Promise<MemorySearchResult[]> {
    return this.store.search(query, options);
  }

  async searchObjects<T = StoredMemoryObject>(
    query: string,
    options: MemorySearchOptions = {}
  ): Promise<T[]> {
    const results = await this.search(query, options);
    return results
      .filter((r): r is Extract<MemorySearchResult, { kind: "object" }> => r.kind === "object")
      .map((r) => r.value as T);
  }

  vectorSearch(
    collection: string,
    queryVector: number[],
    options: VectorSearchOptions = {}
  ): Promise<VectorSearchHit[]> {
    return (
      this.store.vectorSearch?.(collection, queryVector, options) ??
      Promise.reject(new Error("Vector search not supported by this driver"))
    );
  }

  async putBatch(collection: string, objects: MemoryObject[]): Promise<StoredMemoryObject[]> {
    return (
      this.store.putBatch?.(collection, objects) ??
      Promise.reject(new Error("Batch put not supported by this driver"))
    );
  }

  async deleteBatch(collection: string, ids: string[]): Promise<number> {
    return (
      this.store.deleteBatch?.(collection, ids) ??
      Promise.reject(new Error("Batch delete not supported by this driver"))
    );
  }

  async getBatch<T = StoredMemoryObject>(collection: string, ids: string[]): Promise<(T | null)[]> {
    return (
      (this.store.getBatch?.(collection, ids) as Promise<(T | null)[]>) ??
      Promise.reject(new Error("Batch get not supported by this driver"))
    );
  }

  readonly events = {
    append: (stream: string, event: MemoryEvent): Promise<StoredMemoryEvent> =>
      this.store.appendEvent(stream, event),
    list: <T = StoredMemoryEvent>(stream?: string, options?: ListEventsOptions): Promise<T[]> =>
      this.store.listEvents<T>(stream, options),
  };

  queue(name: string): MemoryQueue {
    return {
      push: (payload: QueueJobPayload, options?: QueueJobOptions) =>
        this.store.pushJob(name, payload, options),
      claim: (options?: QueueClaimOptions) => this.store.claimJob(name, options),
      ack: (jobId: string) => this.store.ackJob(name, jobId),
      nack: (jobId: string, options?: QueueNackOptions) => this.store.nackJob(name, jobId, options),
      list: () => this.store.listJobs(name),
      dead: () => this.store.listDeadJobs(name),
    };
  }

  readonly links = {
    create: (
      fromRef: string,
      linkType: string,
      toRef: string,
      weight?: number,
      metadataJson?: string
    ) =>
      this.store.createLink?.(fromRef, linkType, toRef, weight, metadataJson) ??
      Promise.reject(new Error("Graph links not supported by this driver")),
    delete: (id: string) => this.store.deleteLink?.(id) ?? Promise.resolve(false),
    get: (id: string) => this.store.getLink?.(id) ?? Promise.resolve(null),
    neighbors: (
      reference: string,
      direction: import("./types.js").LinkDirection = "Both",
      options: import("./types.js").LinkQueryOptions = {}
    ) => this.store.getNeighbors?.(reference, direction, options) ?? Promise.resolve([]),
  };

  readonly aggregate = {
    count: (
      collection: string,
      options: Omit<AggregateOptions, "function"> = {}
    ): Promise<AggregateResult> =>
      this.store.aggregate?.(collection, { ...options, function: "count" }) ??
      Promise.reject(new Error("Aggregation not supported by this driver")),
    sum: (
      collection: string,
      field: string,
      options: Omit<AggregateOptions, "function" | "field"> = {}
    ): Promise<AggregateResult> =>
      this.store.aggregate?.(collection, { ...options, function: "sum", field }) ??
      Promise.reject(new Error("Aggregation not supported by this driver")),
    avg: (
      collection: string,
      field: string,
      options: Omit<AggregateOptions, "function" | "field"> = {}
    ): Promise<AggregateResult> =>
      this.store.aggregate?.(collection, { ...options, function: "avg", field }) ??
      Promise.reject(new Error("Aggregation not supported by this driver")),
    min: (
      collection: string,
      field: string,
      options: Omit<AggregateOptions, "function" | "field"> = {}
    ): Promise<AggregateResult> =>
      this.store.aggregate?.(collection, { ...options, function: "min", field }) ??
      Promise.reject(new Error("Aggregation not supported by this driver")),
    max: (
      collection: string,
      field: string,
      options: Omit<AggregateOptions, "function" | "field"> = {}
    ): Promise<AggregateResult> =>
      this.store.aggregate?.(collection, { ...options, function: "max", field }) ??
      Promise.reject(new Error("Aggregation not supported by this driver")),
  };

  timeseries(collection: string, options: TimeSeriesOptions): Promise<TimeSeriesResult> {
    return (
      this.store.timeseries?.(collection, options) ??
      Promise.reject(new Error("Time-series aggregation not supported by this driver"))
    );
  }

  async schema(collection?: string, options?: SchemaOptions): Promise<CollectionSchema[]> {
    return (
      this.store.schema?.(collection, options) ??
      Promise.reject(new Error("schema not supported by this driver"))
    );
  }

  readonly nlq = {
    query: (question: string, options?: NlqOptions): Promise<NlqResult> => {
      return (
        this.store.nlqQuery?.(question, options) ??
        Promise.reject(new Error("NLQ not supported by this driver"))
      );
    },
  };

  async close(): Promise<void> {
    await this.store.close?.();
  }

  backupTo(path: string): void {
    if (this.store.backupTo) {
      this.store.backupTo(path);
    } else {
      throw new Error("Backup is only supported on the native durable storage driver");
    }
  }

  walCheckpoint(): import("./types.js").WalCheckpointResult {
    if (this.store.walCheckpoint) {
      return this.store.walCheckpoint();
    }
    throw new Error("Checkpoint is only supported on the native durable storage driver");
  }

  async countObjects(): Promise<number> {
    return this.store.countObjects?.() ?? 0;
  }

  async countObjectsInCollection(collection: string): Promise<number> {
    return this.store.countObjectsInCollection?.(collection) ?? 0;
  }

  async countEvents(): Promise<number> {
    return this.store.countEvents?.() ?? 0;
  }

  async countActiveJobs(): Promise<number> {
    return this.store.countActiveJobs?.() ?? 0;
  }

  async countDeadJobs(): Promise<number> {
    return this.store.countDeadJobs?.() ?? 0;
  }

  async countLinks(): Promise<number> {
    return this.store.countLinks?.() ?? 0;
  }

  async listCollections(): Promise<string[]> {
    return this.store.listCollections?.() ?? [];
  }

  async listStreams(): Promise<string[]> {
    return this.store.listStreams?.() ?? [];
  }

  async listQueues(): Promise<string[]> {
    return this.store.listQueues?.() ?? [];
  }

  /** Create a functional index on a JSON body field for a collection. */
  async createIndex(collection: string, field: string): Promise<void> {
    await this.store.createIndex?.(collection, field);
  }

  /** List all custom functional indexes. */
  async listIndexes(): Promise<Array<[string, string]>> {
    return this.store.listIndexes?.() ?? [];
  }

  /** List available connector types. Requires sidecar/HTTP store (returns [] for in-memory/native). */
  async listConnectors(): Promise<string[]> {
    return this.store.listConnectors?.() ?? [];
  }

  /**
   * Discover the schema of an external table or file source.
   * Requires sidecar/HTTP store — throws on in-memory/native stores.
   */
  async discoverConnectorSchema(
    type: string,
    query: string,
    auth?: ConnectorAuth
  ): Promise<ConnectorSchema> {
    const result = await this.store.discoverConnectorSchema?.(type, query, auth);
    if (!result) {
      throw new Error(`Connector '${type}' not supported by this store`);
    }
    return result;
  }

  /**
   * Import data from an external source into a thingd collection.
   * Requires sidecar/HTTP store — throws on in-memory/native stores.
   */
  async connectorSync(type: string, options: ConnectorSyncOptions): Promise<ConnectorSyncResult> {
    const result = await this.store.connectorSync?.(type, options);
    if (!result) {
      throw new Error(`Connector '${type}' not supported by this store`);
    }
    return result;
  }
}

type ResolvedThingDOpenOptions = ThingDOpenOptions & {
  path: string;
};

function resolveOpenOptions(
  pathOrConfig: string | ThingDOpenConfig | undefined,
  options: ThingDOpenOptions
): ResolvedThingDOpenOptions {
  const config =
    typeof pathOrConfig === "string"
      ? {
          path: pathOrConfig,
        }
      : (pathOrConfig ?? {});
  const path = config.url ?? config.path ?? process.env.THINGD_URL ?? ":memory:";
  const driver = options.driver ?? config.driver ?? inferDriver(path);

  return {
    ...config,
    ...options,
    path,
    driver,
    authToken:
      options.authToken ??
      config.authToken ??
      config.apiKey ??
      options.apiKey ??
      process.env.THINGD_AUTH_TOKEN,
  };
}

function inferDriver(path: string): ThingDDriver | undefined {
  if (isCloudPath(path)) {
    return "cloud";
  }

  return undefined;
}

function isCloudPath(path: string): boolean {
  return path.startsWith("http://") || path.startsWith("https://") || path.startsWith("thingd://");
}

async function openStore(path: string, options: ResolvedThingDOpenOptions): Promise<ThingStore> {
  if (options.store) {
    return options.store;
  }

  if (options.driver === "cloud") {
    return HttpThingStore.open({
      url: path,
      authToken: options.authToken,
      instanceSlug: options.instanceSlug,
    });
  }

  const hasNative = await NativeThingStore.isAvailable();

  if (options.driver === "native") {
    if (!hasNative) {
      throw new Error(
        `The native thingd driver is not available. Install @thingd/native with "npm install @thingd/native". For monorepo development: "pnpm --filter thingd-native build".`
      );
    }
    return NativeThingStore.open(path);
  }

  // Auto-detect and promote file paths to native store when available, with a warning fallback to memory.
  if (!options.driver && path !== ":memory:") {
    if (hasNative) {
      return NativeThingStore.open(path);
    }

    console.warn(
      `Warning: The native thingd driver is not available. Falling back to the temporary in-memory store. Data will not persist. Run "pnpm --filter @thingd/native build" or install "@thingd/native" to enable native persistence.`
    );
  }

  return new InMemoryThingStore();
}
