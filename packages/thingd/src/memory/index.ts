import { InMemoryThingStore } from "../client/in-memory-thing-store.js";
import type {
  ListEventsOptions,
  ListObjectsOptions,
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
  ThingDConnection,
  ThingDeleteResult,
} from "../types.js";

export class ThingD implements ThingDConnection {
  static async open(): Promise<ThingD> {
    return new ThingD(new InMemoryThingStore());
  }

  private constructor(private readonly store: InMemoryThingStore) {}

  put(collection: string, object: MemoryObject): Promise<StoredMemoryObject> {
    return this.store.put(collection, object);
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
    return this.store.listObjects?.(collection, options) ?? Promise.resolve([]);
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

  async putBatch(collection: string, objects: MemoryObject[]): Promise<StoredMemoryObject[]> {
    return this.store.putBatch?.(collection, objects) ?? Promise.reject(new Error("Not supported"));
  }

  async deleteBatch(collection: string, ids: string[]): Promise<number> {
    return this.store.deleteBatch?.(collection, ids) ?? Promise.reject(new Error("Not supported"));
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
      Promise.reject(new Error("Not supported")),
    delete: (id: string) => this.store.deleteLink?.(id) ?? Promise.resolve(false),
    get: (id: string) => this.store.getLink?.(id) ?? Promise.resolve(null),
    neighbors: (
      reference: string,
      direction: import("../types.js").LinkDirection = "Both",
      options: import("../types.js").LinkQueryOptions = {}
    ) => this.store.getNeighbors?.(reference, direction, options) ?? Promise.resolve([]),
  };

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
}

export { InMemoryThingStore } from "../client/in-memory-thing-store.js";
