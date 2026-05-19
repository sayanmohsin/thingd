import { InMemoryMemoryStore } from "./stores/in-memory-memory-store.js";
import { NativeMemoryStore } from "./stores/native-memory-store.js";
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

export type MemoryDDriver = "memory" | "native";

export type MemoryDOpenOptions = {
  driver?: MemoryDDriver;
  store?: MemoryStore;
};

export type MemoryDOpenConfig = MemoryDOpenOptions & {
  path: string;
};

export class MemoryD {
  static async open(
    pathOrConfig: string | MemoryDOpenConfig,
    options: MemoryDOpenOptions = {},
  ): Promise<MemoryD> {
    const path = typeof pathOrConfig === "string" ? pathOrConfig : pathOrConfig.path;
    const resolvedOptions =
      typeof pathOrConfig === "string"
        ? options
        : {
            ...pathOrConfig,
            ...options,
          };

    return new MemoryD(path, await openStore(path, resolvedOptions));
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
}

async function openStore(path: string, options: MemoryDOpenOptions): Promise<MemoryStore> {
  if (options.store) {
    return options.store;
  }

  if (options.driver === "native") {
    return NativeMemoryStore.open(path);
  }

  return new InMemoryMemoryStore();
}
