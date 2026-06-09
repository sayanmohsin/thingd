import { CloudThingStore } from "./stores/cloud-thing-store.js";
import { InMemoryThingStore } from "./stores/in-memory-thing-store.js";
import { NativeThingStore } from "./stores/native-thing-store.js";
import type {
  MemoryEvent,
  MemoryObject,
  MemoryQueue,
  MemorySearchOptions,
  MemorySearchResult,
  QueueClaimOptions,
  QueueJobOptions,
  QueueJobPayload,
  QueueNackOptions,
  StoredMemoryEvent,
  StoredMemoryObject,
  ThingDeleteResult,
  ThingStore,
} from "./types.js";

export type ThingDDriver = "memory" | "native" | "cloud";

export type ThingDOpenOptions = {
  driver?: ThingDDriver;
  store?: ThingStore;
  authToken?: string;
};

export type ThingDOpenConfig = ThingDOpenOptions & {
  path?: string;
  url?: string;
};

export class ThingD {
  static async open(
    pathOrConfig?: string | ThingDOpenConfig,
    options: ThingDOpenOptions = {}
  ): Promise<ThingD> {
    const resolvedOptions = resolveOpenOptions(pathOrConfig, options);

    return new ThingD(resolvedOptions.path, await openStore(resolvedOptions.path, resolvedOptions));
  }

  private constructor(
    readonly path: string,
    private readonly store: ThingStore
  ) {}

  put(collection: string, object: MemoryObject): Promise<StoredMemoryObject> {
    return this.store.put(collection, object);
  }

  get(collection: string, id: string): Promise<StoredMemoryObject | null> {
    return this.store.get(collection, id);
  }

  delete(collection: string, id: string): Promise<ThingDeleteResult> {
    return this.store.delete(collection, id);
  }

  listObjects(collection: string): Promise<StoredMemoryObject[]> {
    return this.store.listObjects?.(collection) ?? Promise.resolve([]);
  }

  search(query: string, options: MemorySearchOptions = {}): Promise<MemorySearchResult[]> {
    return this.store.search(query, options);
  }

  readonly events = {
    append: (stream: string, event: MemoryEvent): Promise<StoredMemoryEvent> =>
      this.store.appendEvent(stream, event),
    list: (stream?: string): Promise<StoredMemoryEvent[]> => this.store.listEvents(stream),
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

  async close(): Promise<void> {
    await this.store.close?.();
  }

  async countObjects(): Promise<number> {
    return this.store.countObjects?.() ?? 0;
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

  async listCollections(): Promise<string[]> {
    return this.store.listCollections?.() ?? [];
  }

  async listStreams(): Promise<string[]> {
    return this.store.listStreams?.() ?? [];
  }

  async listQueues(): Promise<string[]> {
    return this.store.listQueues?.() ?? [];
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
    authToken: options.authToken ?? config.authToken ?? process.env.THINGD_AUTH_TOKEN,
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
    return CloudThingStore.open({
      url: path,
      authToken: options.authToken,
    });
  }

  const hasNative = await NativeThingStore.isAvailable();

  if (options.driver === "native") {
    if (!hasNative) {
      throw new Error(
        `The native thingd driver is not available. Run "pnpm --filter thingd-native build" before using driver: "native".`
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
      `Warning: The native thingd driver is not available. Falling back to the temporary in-memory store. Data will not persist. Run "pnpm --filter thingd-native build" or install "thingd-native" to enable native persistence.`
    );
  }

  return new InMemoryThingStore();
}
