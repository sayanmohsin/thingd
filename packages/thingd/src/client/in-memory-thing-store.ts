import type {
  Link,
  LinkDirection,
  LinkQueryOptions,
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

function uid(): string {
  return typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

function now(): string {
  return new Date().toISOString();
}

type Row = {
  collection: string;
  data: StoredMemoryObject;
};

type EventRow = StoredMemoryEvent;

type JobRow = QueueJob;

type LinkRow = Link;

export class InMemoryThingStore implements ThingStore {
  private objects = new Map<string, Row>();
  private events = new Map<string, EventRow[]>();
  private jobs = new Map<string, JobRow[]>();
  private links = new Map<string, LinkRow>();

  async put(collection: string, object: MemoryObject): Promise<StoredMemoryObject> {
    const key = `${collection}:${object.id}`;
    const existing = this.objects.get(key);
    const timestamp = now();
    const stored: StoredMemoryObject = {
      ...object,
      id: object.id,
      collection,
      createdAt: existing?.data.createdAt ?? timestamp,
      updatedAt: timestamp,
      version: (existing?.data.version ?? 0) + 1,
    };
    this.objects.set(key, { collection, data: stored });
    return stored;
  }

  async get<T = StoredMemoryObject>(collection: string, id: string): Promise<T | null> {
    const row = this.objects.get(`${collection}:${id}`);
    return row ? (row.data as T) : null;
  }

  async getBatch<T = StoredMemoryObject>(collection: string, ids: string[]): Promise<(T | null)[]> {
    return Promise.all(ids.map((id) => this.get<T>(collection, id)));
  }

  async delete(_collection: string, _id: string): Promise<ThingDeleteResult> {
    const deleted = this.objects.delete(`${_collection}:${_id}`);
    return { deleted };
  }

  async listObjects<T = StoredMemoryObject>(
    collection: string,
    options?: ListObjectsOptions
  ): Promise<T[]> {
    let items = Array.from(this.objects.values())
      .filter((r) => r.collection === collection)
      .map((r) => r.data);

    if (options?.filter) {
      const filter = options.filter;
      items = items.filter((obj) =>
        Object.entries(filter).every(([k, v]) => (obj as Record<string, unknown>)[k] === v)
      );
    }

    if (options?.sortBy) {
      const field =
        options.sortBy.field === "created_at"
          ? "createdAt"
          : options.sortBy.field === "updated_at"
            ? "updatedAt"
            : options.sortBy.field;
      const dir = options.sortBy.direction === "desc" ? -1 : 1;
      items.sort((a, b) => {
        const va = (a as Record<string, unknown>)[field] as string | number | undefined;
        const vb = (b as Record<string, unknown>)[field] as string | number | undefined;
        if (va == null) {
          return 1;
        }
        if (vb == null) {
          return -1;
        }
        return va < vb ? -dir : va > vb ? dir : 0;
      });
    }

    if (options?.offset) {
      items = items.slice(options.offset);
    }
    if (options?.limit) {
      items = items.slice(0, options.limit);
    }

    return items as T[];
  }

  async appendEvent(stream: string, event: MemoryEvent): Promise<StoredMemoryEvent> {
    const streamEvents = this.events.get(stream) ?? [];
    const sequence = streamEvents.length + 1;
    const stored: StoredMemoryEvent = {
      ...event,
      id: String(sequence),
      stream,
      sequence,
      createdAt: now(),
    };
    streamEvents.push(stored);
    this.events.set(stream, streamEvents);
    return stored;
  }

  async listEvents<T = StoredMemoryEvent>(
    stream?: string,
    options?: ListEventsOptions
  ): Promise<T[]> {
    let items: StoredMemoryEvent[];
    if (stream) {
      items = this.events.get(stream) ?? [];
    } else {
      items = Array.from(this.events.values()).flat();
    }

    if (options?.fromSequence) {
      items = items.filter((e) => e.sequence > (options.fromSequence ?? 0));
    }
    if (options?.since) {
      const since = options.since;
      items = items.filter((e) => e.createdAt >= since);
    }
    if (options?.limit) {
      items = items.slice(0, options.limit);
    }

    return items as T[];
  }

  async pushJob(
    queue: string,
    payload: QueueJobPayload,
    options: QueueJobOptions = {}
  ): Promise<QueueJob> {
    const timestamp = now();
    const job: QueueJob = {
      id: options.idempotencyKey ?? uid(),
      queue,
      payload,
      status: "ready",
      attempts: 0,
      maxAttempts: options.maxAttempts ?? 3,
      createdAt: timestamp,
      availableAt: new Date(Date.now() + (options.delayMs ?? 0)).toISOString(),
    };
    const queueJobs = this.jobs.get(queue) ?? [];
    queueJobs.push(job);
    this.jobs.set(queue, queueJobs);
    return job;
  }

  async claimJob(queue: string, options: QueueClaimOptions = {}): Promise<QueueJob | null> {
    const queueJobs = this.jobs.get(queue);
    if (!queueJobs) {
      return null;
    }

    const now_ = new Date();
    const idx = queueJobs.findIndex(
      (c) => c.status === "ready" && c.availableAt <= now_.toISOString()
    );
    if (idx === -1) {
      return null;
    }

    const job = queueJobs[idx] as JobRow;
    job.status = "leased";
    job.attempts += 1;
    job.leasedAt = now_.toISOString();
    job.leaseExpiresAt = new Date(
      now_.getTime() + (options.leaseMs ?? DEFAULT_LEASE_MS)
    ).toISOString();
    return job;
  }

  async ackJob(queue: string, jobId: string): Promise<QueueJobResult> {
    const job = this.findJob(queue, jobId);
    if (!job) {
      return { ok: false, reason: "not_found" };
    }
    if (job.status !== "leased") {
      return { ok: false, reason: "not_leased" };
    }
    job.status = "completed";
    job.completedAt = now();
    return { ok: true, job };
  }

  async nackJob(
    queue: string,
    jobId: string,
    options: QueueNackOptions = {}
  ): Promise<QueueJobResult> {
    const job = this.findJob(queue, jobId);
    if (!job) {
      return { ok: false, reason: "not_found" };
    }
    if (job.status !== "leased") {
      return { ok: false, reason: "not_leased" };
    }

    job.lastError = options.error;

    if (job.attempts >= job.maxAttempts) {
      job.status = "dead";
      job.deadAt = now();
    } else {
      job.status = "ready";
      job.availableAt = new Date(Date.now() + (options.delayMs ?? 0)).toISOString();
    }

    return { ok: true, job };
  }

  async listJobs(queue: string): Promise<QueueJob[]> {
    return this.jobs.get(queue) ?? [];
  }

  async listDeadJobs(queue: string): Promise<QueueJob[]> {
    return (this.jobs.get(queue) ?? []).filter((j) => j.status === "dead");
  }

  async search(_query: string, _options: MemorySearchOptions = {}): Promise<MemorySearchResult[]> {
    return [];
  }

  async putBatch(collection: string, objects: MemoryObject[]): Promise<StoredMemoryObject[]> {
    return Promise.all(objects.map((obj) => this.put(collection, obj)));
  }

  async deleteBatch(collection: string, ids: string[]): Promise<number> {
    let count = 0;
    for (const id of ids) {
      const result = await this.delete(collection, id);
      if (result.deleted) {
        count++;
      }
    }
    return count;
  }

  async createLink(
    fromRef: string,
    linkType: string,
    toRef: string,
    weight?: number,
    metadataJson?: string
  ): Promise<Link> {
    const link: Link = {
      id: uid(),
      fromRef,
      linkType,
      toRef,
      weight,
      metadataJson: metadataJson ?? "{}",
      createdAt: now(),
    };
    this.links.set(link.id, link);
    return link;
  }

  async deleteLink(id: string): Promise<boolean> {
    return this.links.delete(id);
  }

  async getLink(id: string): Promise<Link | null> {
    return this.links.get(id) ?? null;
  }

  async getNeighbors(
    reference: string,
    direction: LinkDirection = "Both",
    options: LinkQueryOptions = {}
  ): Promise<Link[]> {
    let items = Array.from(this.links.values()).filter((l) => {
      if (direction === "Outgoing" || direction === "Both") {
        if (l.fromRef === reference) {
          return true;
        }
      }
      if (direction === "Incoming" || direction === "Both") {
        if (l.toRef === reference) {
          return true;
        }
      }
      return false;
    });

    if (options.linkType) {
      items = items.filter((l) => l.linkType === options.linkType);
    }
    if (options.limit) {
      items = items.slice(0, options.limit);
    }

    return items;
  }

  async countObjects(): Promise<number> {
    return this.objects.size;
  }

  async countEvents(): Promise<number> {
    return Array.from(this.events.values()).reduce((acc, e) => acc + e.length, 0);
  }

  async countActiveJobs(): Promise<number> {
    let active = 0;
    for (const jobs of this.jobs.values()) {
      active += jobs.filter((j) => j.status === "ready" || j.status === "leased").length;
    }
    return active;
  }

  async countDeadJobs(): Promise<number> {
    let dead = 0;
    for (const jobs of this.jobs.values()) {
      dead += jobs.filter((j) => j.status === "dead").length;
    }
    return dead;
  }

  async countLinks(): Promise<number> {
    return this.links.size;
  }

  async listCollections(): Promise<string[]> {
    const collections = new Set(Array.from(this.objects.values()).map((r) => r.collection));
    return Array.from(collections);
  }

  async listStreams(): Promise<string[]> {
    return Array.from(this.events.keys());
  }

  async listQueues(): Promise<string[]> {
    return Array.from(this.jobs.keys());
  }

  async close(): Promise<void> {}

  private findJob(queue: string, jobId: string): QueueJob | undefined {
    return (this.jobs.get(queue) ?? []).find((j) => j.id === jobId);
  }
}
