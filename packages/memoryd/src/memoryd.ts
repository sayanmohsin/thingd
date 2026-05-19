import { InMemoryMemoryStore } from "./stores/in-memory-memory-store.js";
import type {
  MemoryDeleteResult,
  MemoryEvent,
  MemoryObject,
  MemoryQueue,
  MemorySearchOptions,
  MemorySearchResult,
  MemoryStore,
  QueueJobOptions,
  QueueJobPayload,
  StoredMemoryEvent,
  StoredMemoryObject,
} from "./types.js";

export type MemoryDOpenOptions = {
  store?: MemoryStore;
};

export class MemoryD {
  static async open(path: string, options: MemoryDOpenOptions = {}): Promise<MemoryD> {
    return new MemoryD(path, options.store ?? new InMemoryMemoryStore());
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
      claim: () => this.store.claimJob(name),
      list: () => this.store.listJobs(name),
    };
  }
}
