import { InMemoryMemoryStore } from "./stores/in-memory-memory-store.js";
import { NativeMemoryStore } from "./stores/native-memory-store.js";
import { RemoteMemoryStore } from "./stores/remote-memory-store.js";
import type {
  MemoryDeleteResult,
  MemoryEvent,
  MemoryObject,
  MemoryQueue,
  MemorySearchOptions,
  MemorySearchResult,
  MemoryStore,
  QueueClaimOptions,
  QueueJobOptions,
  QueueJobPayload,
  QueueNackOptions,
  StoredMemoryEvent,
  StoredMemoryObject,
} from "./types.js";

export type MemoryDDriver = "memory" | "native" | "remote";

export type MemoryDOpenOptions = {
  driver?: MemoryDDriver;
  store?: MemoryStore;
  authToken?: string;
};

export type MemoryDOpenConfig = MemoryDOpenOptions & {
  path?: string;
  url?: string;
};

export class MemoryD {
  static async open(
    pathOrConfig?: string | MemoryDOpenConfig,
    options: MemoryDOpenOptions = {},
  ): Promise<MemoryD> {
    const resolvedOptions = resolveOpenOptions(pathOrConfig, options);

    return new MemoryD(
      resolvedOptions.path,
      await openStore(resolvedOptions.path, resolvedOptions),
    );
  }

  private constructor(
    readonly path: string,
    private readonly store: MemoryStore,
  ) {}

  put(collection: string, object: MemoryObject): Promise<StoredMemoryObject> {
    return this.store.put(collection, object);
  }

  get(collection: string, id: string): Promise<StoredMemoryObject | null> {
    return this.store.get(collection, id);
  }

  delete(collection: string, id: string): Promise<MemoryDeleteResult> {
    return this.store.delete(collection, id);
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
}

type ResolvedMemoryDOpenOptions = MemoryDOpenOptions & {
  path: string;
};

function resolveOpenOptions(
  pathOrConfig: string | MemoryDOpenConfig | undefined,
  options: MemoryDOpenOptions,
): ResolvedMemoryDOpenOptions {
  const config =
    typeof pathOrConfig === "string"
      ? {
          path: pathOrConfig,
        }
      : (pathOrConfig ?? {});
  const path = config.url ?? config.path ?? process.env.MEMORYD_URL ?? ":memory:";
  const driver = options.driver ?? config.driver ?? inferDriver(path);

  return {
    ...config,
    ...options,
    path,
    driver,
    authToken: options.authToken ?? config.authToken ?? process.env.MEMORYD_AUTH_TOKEN,
  };
}

function inferDriver(path: string): MemoryDDriver | undefined {
  if (isRemotePath(path)) {
    return "remote";
  }

  return undefined;
}

function isRemotePath(path: string): boolean {
  return path.startsWith("http://") || path.startsWith("https://") || path.startsWith("memoryd://");
}

async function openStore(path: string, options: ResolvedMemoryDOpenOptions): Promise<MemoryStore> {
  if (options.store) {
    return options.store;
  }

  if (options.driver === "remote") {
    return RemoteMemoryStore.open({
      url: path,
      authToken: options.authToken,
    });
  }

  if (options.driver === "native") {
    return NativeMemoryStore.open(path);
  }

  return new InMemoryMemoryStore();
}
