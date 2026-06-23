import { randomUUID } from "node:crypto";
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

const DEFAULT_LEASE_MS = 30_000;

class Mutex {
  private queue: (() => void)[] = [];
  private locked = false;

  async acquire(): Promise<() => void> {
    if (!this.locked) {
      this.locked = true;
      return () => this.release();
    }
    return new Promise<() => void>((resolve) => {
      this.queue.push(() => {
        this.locked = true;
        resolve(() => this.release());
      });
    });
  }

  private release(): void {
    const next = this.queue.shift();
    if (next) {
      next();
    } else {
      this.locked = false;
    }
  }
}

export class InMemoryThingStore implements ThingStore {
  private readonly collections = new Map<string, Map<string, StoredMemoryObject>>();
  private readonly events: StoredMemoryEvent[] = [];
  private nextEventSequence = 0;
  private readonly queues = new Map<string, QueueJob[]>();
  private readonly links = new Map<string, import("../types.js").Link>();
  private readonly mutex = new Mutex();

  private async withLock<T>(fn: () => T): Promise<T> {
    const release = await this.mutex.acquire();
    try {
      return fn();
    } finally {
      release();
    }
  }

  async put(collection: string, object: MemoryObject): Promise<StoredMemoryObject> {
    return this.withLock(() => {
      const records = this.getCollection(collection);
      const now = new Date().toISOString();
      const existing = records.get(object.id);
      const record: StoredMemoryObject = {
        ...object,
        id: object.id,
        collection,
        createdAt: existing?.createdAt ?? now,
        updatedAt: now,
        version: (existing?.version ?? 0) + 1,
      };
      records.set(object.id, record);
      return record;
    });
  }

  async get<T = StoredMemoryObject>(collection: string, id: string): Promise<T | null> {
    return (this.collections.get(collection)?.get(id) as T | null) ?? null;
  }

  async delete(collection: string, id: string): Promise<ThingDeleteResult> {
    return this.withLock(() => ({
      deleted: this.collections.get(collection)?.delete(id) ?? false,
    }));
  }

  async listObjects<T = StoredMemoryObject>(
    collection: string,
    options?: ListObjectsOptions
  ): Promise<T[]> {
    const records = this.collections.get(collection);
    if (!records) {
      return [];
    }
    let results = Array.from(records.values()) as T[];
    const filter = options?.filter;
    if (filter) {
      results = results.filter((obj) =>
        Object.entries(filter).every(
          ([key, value]) => (obj as Record<string, unknown>)[key] === value
        )
      );
    }
    if (options?.sortBy) {
      const { field, direction } = options.sortBy;
      const asc = direction !== "desc";
      results.sort((a, b) => {
        const va = (a as Record<string, unknown>)[field] as string | number | undefined;
        const vb = (b as Record<string, unknown>)[field] as string | number | undefined;
        if (va === vb) {
          return 0;
        }
        if (va === undefined) {
          return 1;
        }
        if (vb === undefined) {
          return -1;
        }
        const cmp = va < vb ? -1 : 1;
        return asc ? cmp : -cmp;
      });
    }
    if (options?.offset) {
      results = results.slice(options.offset);
    }
    if (options?.limit) {
      results = results.slice(0, options.limit);
    }
    return results;
  }

  async appendEvent(stream: string, event: MemoryEvent): Promise<StoredMemoryEvent> {
    return this.withLock(() => {
      this.nextEventSequence += 1;
      const record: StoredMemoryEvent = {
        ...event,
        id: randomUUID(),
        stream,
        sequence: this.nextEventSequence,
        createdAt: new Date().toISOString(),
      };
      this.events.push(record);
      return record;
    });
  }

  async listEvents<T = StoredMemoryEvent>(
    stream?: string,
    options?: ListEventsOptions
  ): Promise<T[]> {
    let events = this.events as T[];
    if (stream) {
      events = events.filter((event) => (event as unknown as StoredMemoryEvent).stream === stream);
    }
    const fromSeq = options?.fromSequence;
    if (fromSeq) {
      events = events.filter((event) => (event as unknown as StoredMemoryEvent).sequence > fromSeq);
    }
    if (options?.limit) {
      events = events.slice(0, options.limit);
    }
    return [...events];
  }

  async pushJob(
    queue: string,
    payload: QueueJobPayload,
    options: QueueJobOptions = {}
  ): Promise<QueueJob> {
    return this.withLock(() => {
      const jobs = this.getQueue(queue);
      const now = new Date().toISOString();
      const job: QueueJob = {
        id: options.idempotencyKey ?? randomUUID(),
        queue,
        payload,
        status: "ready",
        attempts: 0,
        maxAttempts: options.maxAttempts ?? 3,
        createdAt: now,
        availableAt: new Date(Date.now() + (options.delayMs ?? 0)).toISOString(),
      };

      const existing = jobs.find((candidate) => candidate.id === job.id);
      if (existing) {
        return this.cloneJob(existing);
      }

      jobs.push(job);
      return this.cloneJob(job);
    });
  }

  async claimJob(queue: string, options: QueueClaimOptions = {}): Promise<QueueJob | null> {
    return this.withLock(() => {
      this.releaseExpiredLeases(queue);

      const now = new Date();
      const job = this.queues
        .get(queue)
        ?.find(
          (candidate) => candidate.status === "ready" && candidate.availableAt <= now.toISOString()
        );

      if (!job) {
        return null;
      }

      job.status = "leased";
      job.attempts += 1;
      job.leasedAt = now.toISOString();
      job.leaseExpiresAt = new Date(
        now.getTime() + (options.leaseMs ?? DEFAULT_LEASE_MS)
      ).toISOString();

      return this.cloneJob(job);
    });
  }

  async ackJob(queue: string, jobId: string): Promise<QueueJobResult> {
    return this.withLock(() => {
      const job = this.findJob(queue, jobId);

      if (!job) {
        return { ok: false, reason: "not_found" } as QueueJobResult;
      }

      if (job.status === "completed" || job.status === "dead") {
        return { ok: false, reason: "terminal" } as QueueJobResult;
      }

      if (job.status !== "leased") {
        return { ok: false, reason: "not_leased" } as QueueJobResult;
      }

      job.status = "completed";
      job.completedAt = new Date().toISOString();

      return { ok: true, job: this.cloneJob(job) };
    });
  }

  async nackJob(
    queue: string,
    jobId: string,
    options: QueueNackOptions = {}
  ): Promise<QueueJobResult> {
    return this.withLock(() => {
      const job = this.findJob(queue, jobId);

      if (!job) {
        return { ok: false, reason: "not_found" } as QueueJobResult;
      }

      if (job.status === "completed" || job.status === "dead") {
        return { ok: false, reason: "terminal" } as QueueJobResult;
      }

      if (job.status !== "leased") {
        return { ok: false, reason: "not_leased" } as QueueJobResult;
      }

      job.lastError = options.error;
      job.leasedAt = undefined;
      job.leaseExpiresAt = undefined;

      if (job.attempts >= job.maxAttempts) {
        job.status = "dead";
        job.deadAt = new Date().toISOString();
      } else {
        job.status = "ready";
        job.availableAt = new Date(Date.now() + (options.delayMs ?? 0)).toISOString();
      }

      return { ok: true, job: this.cloneJob(job) };
    });
  }

  async listJobs(queue: string): Promise<QueueJob[]> {
    return (this.queues.get(queue) ?? []).map((job) => this.cloneJob(job));
  }

  async listDeadJobs(queue: string): Promise<QueueJob[]> {
    return (this.queues.get(queue) ?? [])
      .filter((job) => job.status === "dead")
      .map((job) => this.cloneJob(job));
  }

  async search(query: string, options: MemorySearchOptions = {}): Promise<MemorySearchResult[]> {
    const normalizedQuery = query.toLowerCase();
    const collections = options.collections ? new Set(options.collections) : null;
    const filter = options.filter;
    const results: MemorySearchResult[] = [];

    for (const [collection, records] of this.collections) {
      if (collections && !collections.has(collection)) {
        continue;
      }

      for (const record of records.values()) {
        if (filter && !this.matchesFilter(filter, record)) {
          continue;
        }
        const haystack = JSON.stringify(record).toLowerCase();
        if (haystack.includes(normalizedQuery)) {
          results.push({
            kind: "object",
            id: record.id,
            collection,
            score: 1,
            value: record,
          });
        }
      }
    }

    for (const event of this.events) {
      if (filter && !this.matchesFilter(filter, event)) {
        continue;
      }
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

    return options.limit !== undefined ? results.slice(0, options.limit) : results;
  }

  private matchesFilter(filter: Record<string, unknown>, obj: Record<string, unknown>): boolean {
    return Object.entries(filter).every(([key, expected]) => {
      return key in obj && obj[key] === expected;
    });
  }

  async countObjects(): Promise<number> {
    let total = 0;
    for (const records of this.collections.values()) {
      total += records.size;
    }
    return total;
  }

  async countEvents(): Promise<number> {
    return this.events.length;
  }

  async countActiveJobs(): Promise<number> {
    let total = 0;
    for (const jobs of this.queues.values()) {
      total += jobs.filter((job) => job.status !== "dead").length;
    }
    return total;
  }

  async countDeadJobs(): Promise<number> {
    let total = 0;
    for (const jobs of this.queues.values()) {
      total += jobs.filter((job) => job.status === "dead").length;
    }
    return total;
  }

  async countLinks(): Promise<number> {
    return this.links.size;
  }

  async putBatch(collection: string, objects: MemoryObject[]): Promise<StoredMemoryObject[]> {
    return this.withLock(() => {
      const records = this.getCollection(collection);
      const now = new Date().toISOString();
      const results: StoredMemoryObject[] = [];
      for (const object of objects) {
        const existing = records.get(object.id);
        const record: StoredMemoryObject = {
          ...object,
          id: object.id,
          collection,
          createdAt: existing?.createdAt ?? now,
          updatedAt: now,
          version: (existing?.version ?? 0) + 1,
        };
        records.set(object.id, record);
        results.push(record);
      }
      return results;
    });
  }

  async deleteBatch(collection: string, ids: string[]): Promise<number> {
    return this.withLock(() => {
      const records = this.collections.get(collection);
      if (!records) {
        return 0;
      }
      let count = 0;
      for (const id of ids) {
        if (records.delete(id)) {
          count++;
        }
      }
      return count;
    });
  }

  async createLink(
    fromRef: string,
    linkType: string,
    toRef: string,
    weight?: number,
    metadataJson?: string
  ): Promise<import("../types.js").Link> {
    return this.withLock(() => {
      const link: import("../types.js").Link = {
        id: randomUUID(),
        fromRef,
        linkType,
        toRef,
        weight,
        metadataJson: metadataJson ?? "{}",
        createdAt: new Date().toISOString(),
      };
      this.links.set(link.id, link);
      return link;
    });
  }

  async deleteLink(id: string): Promise<boolean> {
    return this.withLock(() => {
      return this.links.delete(id);
    });
  }

  async getLink(id: string): Promise<import("../types.js").Link | null> {
    return this.links.get(id) ?? null;
  }

  async getNeighbors(
    reference: string,
    direction: import("../types.js").LinkDirection,
    options: import("../types.js").LinkQueryOptions
  ): Promise<import("../types.js").Link[]> {
    let results = Array.from(this.links.values());

    results = results.filter((link) => {
      if (direction === "Outgoing") {
        return link.fromRef === reference;
      }
      if (direction === "Incoming") {
        return link.toRef === reference;
      }
      return link.fromRef === reference || link.toRef === reference;
    });

    if (options.linkType) {
      results = results.filter((link) => link.linkType === options.linkType);
    }

    if (options.limit !== undefined) {
      results = results.slice(0, options.limit);
    }

    return results;
  }

  async listCollections(): Promise<string[]> {
    return Array.from(this.collections.keys()).sort();
  }

  async listQueues(): Promise<string[]> {
    return Array.from(this.queues.keys()).sort();
  }

  async listStreams(): Promise<string[]> {
    const streams = new Set<string>();
    for (const event of this.events) {
      streams.add(event.stream);
    }
    return Array.from(streams).sort();
  }

  async close(): Promise<void> {
    // no-op for in-memory
  }

  walCheckpoint(): { framesBefore: number; framesAfter: number } {
    throw new Error("WAL checkpoint is not supported for in-memory storage");
  }

  private getCollection(collection: string): Map<string, StoredMemoryObject> {
    const records = this.collections.get(collection) ?? new Map<string, StoredMemoryObject>();
    this.collections.set(collection, records);
    return records;
  }

  private getQueue(queue: string): QueueJob[] {
    const jobs = this.queues.get(queue) ?? [];
    this.queues.set(queue, jobs);
    return jobs;
  }

  private findJob(queue: string, jobId: string): QueueJob | null {
    return this.queues.get(queue)?.find((job) => job.id === jobId) ?? null;
  }

  private releaseExpiredLeases(queue: string) {
    const now = new Date().toISOString();

    for (const job of this.queues.get(queue) ?? []) {
      if (job.status === "leased" && job.leaseExpiresAt && job.leaseExpiresAt <= now) {
        job.status = "ready";
        job.leasedAt = undefined;
        job.leaseExpiresAt = undefined;
      }
    }
  }

  private cloneJob(job: QueueJob): QueueJob {
    return {
      ...job,
      payload: {
        ...job.payload,
      },
    };
  }
}
