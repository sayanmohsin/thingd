import { randomUUID } from "node:crypto";
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
  private readonly eventIdempotencyKeys = new Map<string, number>();

  private async withLock<T>(fn: () => T): Promise<T> {
    const release = await this.mutex.acquire();
    try {
      return fn();
    } finally {
      release();
    }
  }

  async put(
    collection: string,
    object: MemoryObject,
    options?: PutOptions
  ): Promise<StoredMemoryObject> {
    return this.withLock(() => {
      const records = this.getCollection(collection);
      const now = new Date().toISOString();
      const existing = records.get(object.id);

      // CAS check: if expectedVersion is set, verify it matches
      if (options?.expectedVersion !== undefined) {
        const currentVersion = existing?.version ?? 0;
        if (currentVersion !== options.expectedVersion) {
          throw new Error(
            `Conflict: version mismatch for ${collection}/${object.id}: expected ${options.expectedVersion}, got ${currentVersion}`
          );
        }
      }

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

  async getBatch<T = StoredMemoryObject>(collection: string, ids: string[]): Promise<(T | null)[]> {
    const records = this.collections.get(collection);
    return ids.map((id) => (records?.get(id) as T) ?? null);
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
      // Idempotency check
      const idempotencyKey = event.idempotencyKey as string | undefined;
      if (idempotencyKey) {
        const existing = this.eventIdempotencyKeys.get(`${stream}:${idempotencyKey}`);
        if (existing !== undefined) {
          const found = this.events.find((e) => e.sequence === existing);
          if (found) {
            return found;
          }
        }
      }

      this.nextEventSequence += 1;
      const record: StoredMemoryEvent = {
        ...event,
        id: randomUUID(),
        stream,
        sequence: this.nextEventSequence,
        createdAt: new Date().toISOString(),
      };

      // Track idempotency key
      if (idempotencyKey) {
        this.eventIdempotencyKeys.set(`${stream}:${idempotencyKey}`, record.sequence);
      }

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
    if (options?.since) {
      const since = options.since;
      events = events.filter((event) => (event as unknown as StoredMemoryEvent).createdAt >= since);
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

  async createIndex(_collection: string, _field: string): Promise<void> {
    // No-op for in-memory store
  }

  async listIndexes(): Promise<Array<[string, string]>> {
    return [];
  }

  async listStreams(): Promise<string[]> {
    const streams = new Set<string>();
    for (const event of this.events) {
      streams.add(event.stream);
    }
    return Array.from(streams).sort();
  }

  async aggregate(collection: string, options: AggregateOptions): Promise<AggregateResult> {
    const records = Array.from(this.collections.get(collection)?.values() ?? []);

    // Apply filter
    const filtered = options.filter
      ? records.filter((obj) =>
          Object.entries(options.filter as Record<string, unknown>).every(
            ([key, value]) => (obj as Record<string, unknown>)[key] === value
          )
        )
      : records;

    if (options.groupBy) {
      const groups = new Map<string, typeof filtered>();
      for (const obj of filtered) {
        const key = String((obj as Record<string, unknown>)[options.groupBy] ?? "");
        const group = groups.get(key) ?? [];
        group.push(obj);
        groups.set(key, group);
      }

      const groupResults = Array.from(groups.entries())
        .map(([key, objs]) => ({
          key,
          value: this.computeAggregate(objs, options.function, options.field),
        }))
        .sort((a, b) => a.key.localeCompare(b.key));

      const total = groupResults.reduce((sum, g) => sum + g.value, 0);
      return { total, groups: groupResults };
    }

    const total = this.computeAggregate(filtered, options.function, options.field);
    return { total, groups: [] };
  }

  async timeseries(collection: string, options: TimeSeriesOptions): Promise<TimeSeriesResult> {
    const records = Array.from(this.collections.get(collection)?.values() ?? []);

    // Apply filter
    let filtered = options.filter
      ? records.filter((obj) =>
          Object.entries(options.filter as Record<string, unknown>).every(
            ([key, value]) => (obj as Record<string, unknown>)[key] === value
          )
        )
      : records;

    // Apply time range
    if (options.from) {
      filtered = filtered.filter((obj) => obj.createdAt >= (options.from as string));
    }
    if (options.to) {
      filtered = filtered.filter((obj) => obj.createdAt < (options.to as string));
    }

    // Bucket by createdAt
    const format = this.getTimeBucketFormat(options.bucket);
    const buckets = new Map<string, typeof filtered>();
    for (const obj of filtered) {
      const label = this.formatTimestamp(obj.createdAt, format);
      const group = buckets.get(label) ?? [];
      group.push(obj);
      buckets.set(label, group);
    }

    const resultBuckets = Array.from(buckets.entries())
      .map(([label, objs]) => ({
        label,
        value: this.computeAggregate(objs, options.function, options.field),
      }))
      .sort((a, b) => a.label.localeCompare(b.label));

    return { buckets: resultBuckets };
  }

  async schema(collection?: string, options?: SchemaOptions): Promise<CollectionSchema[]> {
    const sampleSize = options?.sampleSize ?? 50;
    const collections = collection ? [collection] : Array.from(this.collections.keys()).sort();

    const result: CollectionSchema[] = [];
    for (const col of collections) {
      const objects = Array.from(this.collections.get(col)?.values() ?? []);
      if (objects.length === 0) {
        continue;
      }

      const sampled = objects.slice(0, sampleSize);
      const fieldMap = new Map<string, { type: string; nullable: boolean; samples: unknown[] }>();

      for (const obj of sampled) {
        const body = typeof obj.body === "string" ? JSON.parse(obj.body) : obj.body;
        if (!body || typeof body !== "object") {
          continue;
        }

        for (const [key, value] of Object.entries(body as Record<string, unknown>)) {
          let entry = fieldMap.get(key);
          if (!entry) {
            entry = { type: this.inferType(value), nullable: false, samples: [] };
            fieldMap.set(key, entry);
          }
          if (value === null || value === undefined) {
            entry.nullable = true;
          } else {
            const inferred = this.inferType(value);
            if (entry.type !== inferred) {
              entry.type = "unknown";
            }
            if (entry.samples.length < 3) {
              entry.samples.push(value);
            }
          }
        }
      }

      result.push({
        name: col,
        objectCount: objects.length,
        fields: Array.from(fieldMap.entries()).map(([name, { type, nullable, samples }]) => ({
          name,
          type,
          nullable,
          sampleValues: samples,
        })),
      });
    }

    return result;
  }

  private inferType(value: unknown): string {
    if (value === null || value === undefined) {
      return "null";
    }
    if (typeof value === "boolean") {
      return "boolean";
    }
    if (typeof value === "number") {
      return "number";
    }
    if (typeof value === "string") {
      if (
        value.length > 10 &&
        (value.includes("T") || value.includes("-")) &&
        !Number.isNaN(Date.parse(value))
      ) {
        return "date";
      }
      return "string";
    }
    if (Array.isArray(value)) {
      return "array";
    }
    if (typeof value === "object") {
      return "object";
    }
    return "unknown";
  }

  private computeAggregate(
    objects: StoredMemoryObject[],
    function_: string,
    field?: string
  ): number {
    switch (function_) {
      case "count":
        return objects.length;
      case "sum":
        return objects.reduce(
          (sum, obj) => sum + (Number((obj as Record<string, unknown>)[field ?? ""]) || 0),
          0
        );
      case "avg": {
        const values = objects
          .map((obj) => Number((obj as Record<string, unknown>)[field ?? ""]) || 0)
          .filter((v) => !Number.isNaN(v));
        return values.length > 0 ? values.reduce((a, b) => a + b, 0) / values.length : 0;
      }
      case "min": {
        const values = objects
          .map((obj) => Number((obj as Record<string, unknown>)[field ?? ""]) || 0)
          .filter((v) => !Number.isNaN(v));
        return values.length > 0 ? Math.min(...values) : 0;
      }
      case "max": {
        const values = objects
          .map((obj) => Number((obj as Record<string, unknown>)[field ?? ""]) || 0)
          .filter((v) => !Number.isNaN(v));
        return values.length > 0 ? Math.max(...values) : 0;
      }
      default:
        return 0;
    }
  }

  private getTimeBucketFormat(bucket: string): string {
    switch (bucket) {
      case "hour":
        return "YYYY-MM-DDTHH:00:00Z";
      case "day":
        return "YYYY-MM-DD";
      case "week":
        return "YYYY-[W]WW";
      case "month":
        return "YYYY-MM";
      default:
        return "YYYY-MM-DD";
    }
  }

  private formatTimestamp(ts: string, format: string): string {
    const date = new Date(ts);
    if (Number.isNaN(date.getTime())) {
      return ts;
    }

    const pad = (n: number) => String(n).padStart(2, "0");
    const year = date.getUTCFullYear();
    const month = pad(date.getUTCMonth() + 1);
    const day = pad(date.getUTCDate());
    const hours = pad(date.getUTCHours());
    const weekNum = Math.ceil((date.getUTCDate() - date.getUTCDay() + 1) / 7);

    return format
      .replace("YYYY", String(year))
      .replace("MM", month)
      .replace("DD", day)
      .replace("HH", hours)
      .replace("[W]WW", `W${String(weekNum).padStart(2, "0")}`);
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
