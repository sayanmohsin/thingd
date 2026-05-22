import { randomUUID } from "node:crypto";
import type {
  ThingDeleteResult,
  MemoryEvent,
  MemoryObject,
  MemorySearchOptions,
  MemorySearchResult,
  ThingStore,
  QueueClaimOptions,
  QueueJob,
  QueueJobOptions,
  QueueJobPayload,
  QueueJobResult,
  QueueNackOptions,
  StoredMemoryEvent,
  StoredMemoryObject,
} from "../types.js";

const DEFAULT_LEASE_MS = 30_000;

export class InMemoryThingStore implements ThingStore {
  private readonly collections = new Map<string, Map<string, StoredMemoryObject>>();
  private readonly events: StoredMemoryEvent[] = [];
  private readonly queues = new Map<string, QueueJob[]>();

  async put(collection: string, object: MemoryObject): Promise<StoredMemoryObject> {
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
  }

  async get(collection: string, id: string): Promise<StoredMemoryObject | null> {
    return this.collections.get(collection)?.get(id) ?? null;
  }

  async delete(collection: string, id: string): Promise<ThingDeleteResult> {
    return {
      deleted: this.collections.get(collection)?.delete(id) ?? false,
    };
  }

  async appendEvent(stream: string, event: MemoryEvent): Promise<StoredMemoryEvent> {
    const record: StoredMemoryEvent = {
      ...event,
      id: randomUUID(),
      stream,
      createdAt: new Date().toISOString(),
    };

    this.events.push(record);
    return record;
  }

  async listEvents(stream?: string): Promise<StoredMemoryEvent[]> {
    if (!stream) {
      return [...this.events];
    }

    return this.events.filter((event) => event.stream === stream);
  }

  async pushJob(
    queue: string,
    payload: QueueJobPayload,
    options: QueueJobOptions = {},
  ): Promise<QueueJob> {
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
  }

  async claimJob(queue: string, options: QueueClaimOptions = {}): Promise<QueueJob | null> {
    this.releaseExpiredLeases(queue);

    const now = new Date();
    const job = this.queues
      .get(queue)
      ?.find(
        (candidate) => candidate.status === "ready" && candidate.availableAt <= now.toISOString(),
      );

    if (!job) {
      return null;
    }

    job.status = "leased";
    job.attempts += 1;
    job.leasedAt = now.toISOString();
    job.leaseExpiresAt = new Date(
      now.getTime() + (options.leaseMs ?? DEFAULT_LEASE_MS),
    ).toISOString();

    return this.cloneJob(job);
  }

  async ackJob(queue: string, jobId: string): Promise<QueueJobResult> {
    const job = this.findJob(queue, jobId);

    if (!job) {
      return {
        ok: false,
        reason: "not_found",
      };
    }

    if (job.status === "completed" || job.status === "dead") {
      return {
        ok: false,
        reason: "terminal",
      };
    }

    if (job.status !== "leased") {
      return {
        ok: false,
        reason: "not_leased",
      };
    }

    job.status = "completed";
    job.completedAt = new Date().toISOString();

    return {
      ok: true,
      job: this.cloneJob(job),
    };
  }

  async nackJob(
    queue: string,
    jobId: string,
    options: QueueNackOptions = {},
  ): Promise<QueueJobResult> {
    const job = this.findJob(queue, jobId);

    if (!job) {
      return {
        ok: false,
        reason: "not_found",
      };
    }

    if (job.status === "completed" || job.status === "dead") {
      return {
        ok: false,
        reason: "terminal",
      };
    }

    if (job.status !== "leased") {
      return {
        ok: false,
        reason: "not_leased",
      };
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

    return {
      ok: true,
      job: this.cloneJob(job),
    };
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
    const limit = options.limit ?? 10;
    const results: MemorySearchResult[] = [];

    for (const [collection, records] of this.collections) {
      if (collections && !collections.has(collection)) {
        continue;
      }

      for (const record of records.values()) {
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

    return results.slice(0, limit);
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
