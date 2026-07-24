import type {
  AggregateOptions,
  AggregateResult,
  CollectionSchema,
  Link,
  LinkDirection,
  LinkQueryOptions,
  ListEventsOptions,
  ListObjectsOptions,
  MemoryEvent,
  MemoryObject,
  MemoryQueue,
  MemorySearchOptions,
  MemorySearchResult,
  NlqOptions,
  NlqResult,
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
  TimeSeriesOptions,
  TimeSeriesResult,
  VectorSearchHit,
  VectorSearchOptions,
} from "./types.js";

export type ThingdClientOptions = {
  url: string;
  authToken?: string;
};

/**
 * Zero-dependency HTTP client for thingd REST API.
 *
 * Works in any runtime with `fetch()` — browsers, Cloudflare Workers,
 * AWS Lambda, Bun, Deno, Node.js 18+.
 *
 * @example
 * ```ts
 * const client = new ThingdClient({
 *   url: "https://api.thingd.cloud",
 *   authToken: "sk-...",
 * });
 * await client.put("notes", { id: "1", text: "hello" });
 * ```
 */
export class ThingdClient {
  private readonly base: string;

  constructor(private readonly options: ThingdClientOptions) {
    const base = options.url.replace(/\/+$/, "");
    this.base = base.endsWith("/v1") ? base : `${base}/v1`;
  }

  private get token(): string | undefined {
    return this.options.authToken;
  }

  private async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const headers: Record<string, string> = {};
    if (body) {
      headers["content-type"] = "application/json";
    }
    if (this.token) {
      headers.authorization = `Bearer ${this.token}`;
    }

    const res = await fetch(`${this.base}${path}`, {
      method,
      headers,
      body: body ? JSON.stringify(body) : undefined,
    });

    const json = await res.json();

    if (!res.ok) {
      const msg = json?.error?.message ?? `HTTP ${res.status}`;
      throw new Error(msg);
    }

    return json.data as T;
  }

  // ── Objects ──────────────────────────────────────────

  async put(collection: string, object: MemoryObject): Promise<StoredMemoryObject> {
    return this.request(
      "PUT",
      `/objects/${encodeURIComponent(collection)}/${encodeURIComponent(object.id)}`,
      object
    );
  }

  async get<T = StoredMemoryObject>(collection: string, id: string): Promise<T | null> {
    try {
      return await this.request<T>(
        "GET",
        `/objects/${encodeURIComponent(collection)}/${encodeURIComponent(id)}`
      );
    } catch (err) {
      if (err instanceof Error && err.message.includes("404")) {
        return null;
      }
      throw err;
    }
  }

  async delete(collection: string, id: string): Promise<ThingDeleteResult> {
    return this.request(
      "DELETE",
      `/objects/${encodeURIComponent(collection)}/${encodeURIComponent(id)}`
    );
  }

  async listObjects<T = StoredMemoryObject>(
    collection: string,
    options?: ListObjectsOptions
  ): Promise<T[]> {
    const params = new URLSearchParams({ collection });
    if (options?.limit) {
      params.set("limit", String(options.limit));
    }
    if (options?.offset) {
      params.set("offset", String(options.offset));
    }
    if (options?.filter) {
      for (const [k, v] of Object.entries(options.filter)) {
        params.set(`filter.${k}`, String(v));
      }
    }
    if (options?.sortBy) {
      params.set("sortBy", options.sortBy.field);
      if (options.sortBy.direction) {
        params.set("sortDir", options.sortBy.direction);
      }
    }
    return this.request("GET", `/objects?${params}`);
  }

  async putBatch(collection: string, objects: MemoryObject[]): Promise<StoredMemoryObject[]> {
    return this.request(
      "PUT",
      `/objects/batch?collection=${encodeURIComponent(collection)}`,
      objects
    );
  }

  async deleteBatch(collection: string, ids: string[]): Promise<number> {
    const result = await this.request<{ deleted: number }>(
      "DELETE",
      `/objects/batch?collection=${encodeURIComponent(collection)}`,
      ids
    );
    return result.deleted;
  }

  // ── Search ───────────────────────────────────────────

  async search(query: string, options: MemorySearchOptions = {}): Promise<MemorySearchResult[]> {
    return this.request("POST", "/search", { query, ...options });
  }

  async vectorSearch(
    collection: string,
    queryVector: number[],
    options: VectorSearchOptions = {}
  ): Promise<VectorSearchHit[]> {
    return this.request("POST", "/search/vector", {
      collection,
      vector: queryVector,
      topK: options.topK,
      filter: options.filter,
    });
  }

  // ── Events ───────────────────────────────────────────

  async appendEvent(stream: string, event: MemoryEvent): Promise<StoredMemoryEvent> {
    return this.request("POST", `/events/${encodeURIComponent(stream)}`, event);
  }

  async listEvents<T = StoredMemoryEvent>(
    stream?: string,
    options?: ListEventsOptions
  ): Promise<T[]> {
    const params = new URLSearchParams();
    if (stream) {
      params.set("stream", stream);
    }
    if (options?.fromSequence) {
      params.set("fromSequence", String(options.fromSequence));
    }
    if (options?.limit) {
      params.set("limit", String(options.limit));
    }
    return this.request("GET", `/events?${params}`);
  }

  readonly events = {
    append: (stream: string, event: MemoryEvent) => this.appendEvent(stream, event),
    list: <T = StoredMemoryEvent>(stream?: string, options?: ListEventsOptions) =>
      this.listEvents<T>(stream, options),
  };

  // ── Queues ───────────────────────────────────────────

  async pushJob(
    queue: string,
    payload: QueueJobPayload,
    options: QueueJobOptions = {}
  ): Promise<QueueJob> {
    return this.request("POST", `/queues/${encodeURIComponent(queue)}/push`, {
      payload,
      ...options,
    });
  }

  async claimJob(queue: string, options: QueueClaimOptions = {}): Promise<QueueJob | null> {
    return this.request("POST", `/queues/${encodeURIComponent(queue)}/claim`, options);
  }

  async ackJob(queue: string, jobId: string): Promise<QueueJobResult> {
    return this.request("POST", `/queues/${encodeURIComponent(queue)}/ack`, {
      jobId,
    });
  }

  async nackJob(
    queue: string,
    jobId: string,
    nackOptions: QueueNackOptions = {}
  ): Promise<QueueJobResult> {
    return this.request("POST", `/queues/${encodeURIComponent(queue)}/nack`, {
      jobId,
      ...nackOptions,
    });
  }

  async listJobs(queue: string): Promise<QueueJob[]> {
    return this.request("GET", `/queues/${encodeURIComponent(queue)}/jobs`);
  }

  async listDeadJobs(queue: string): Promise<QueueJob[]> {
    return this.request("GET", `/queues/${encodeURIComponent(queue)}/dead`);
  }

  queue(name: string): MemoryQueue {
    return {
      push: (payload, options) => this.pushJob(name, payload, options),
      claim: (options) => this.claimJob(name, options),
      ack: (jobId) => this.ackJob(name, jobId),
      nack: (jobId, opts) => this.nackJob(name, jobId, opts),
      list: () => this.listJobs(name),
      dead: () => this.listDeadJobs(name),
    };
  }

  // ── Links ────────────────────────────────────────────

  async createLink(
    fromRef: string,
    linkType: string,
    toRef: string,
    weight?: number,
    metadataJson?: string
  ): Promise<Link> {
    return this.request("POST", "/links", {
      fromRef,
      linkType,
      toRef,
      weight,
      metadataJson,
    });
  }

  async deleteLink(id: string): Promise<boolean> {
    const result = await this.request<{ deleted: boolean }>(
      "DELETE",
      `/links/${encodeURIComponent(id)}`
    );
    return result.deleted;
  }

  async getLink(id: string): Promise<Link | null> {
    try {
      return await this.request<Link>("GET", `/links/${encodeURIComponent(id)}`);
    } catch (err) {
      if (err instanceof Error && err.message.includes("404")) {
        return null;
      }
      throw err;
    }
  }

  async getNeighbors(
    reference: string,
    direction: LinkDirection = "Both",
    options: LinkQueryOptions = {}
  ): Promise<Link[]> {
    const params = new URLSearchParams({ reference, direction });
    if (options.linkType) {
      params.set("linkType", options.linkType);
    }
    if (options.limit) {
      params.set("limit", String(options.limit));
    }
    return this.request("GET", `/links?${params}`);
  }

  readonly links = {
    create: (
      fromRef: string,
      linkType: string,
      toRef: string,
      weight?: number,
      metadataJson?: string
    ) => this.createLink(fromRef, linkType, toRef, weight, metadataJson),
    delete: (id: string) => this.deleteLink(id),
    get: (id: string) => this.getLink(id),
    neighbors: (reference: string, direction?: LinkDirection, options?: LinkQueryOptions) =>
      this.getNeighbors(reference, direction, options),
  };

  // ── Counts ───────────────────────────────────────────

  async countObjects(): Promise<number> {
    const result = await this.request<{ count: number }>("GET", "/counts/objects");
    return result.count;
  }

  async countEvents(): Promise<number> {
    const result = await this.request<{ count: number }>("GET", "/counts/events");
    return result.count;
  }

  async countActiveJobs(): Promise<number> {
    return 0;
  }

  async countDeadJobs(): Promise<number> {
    return 0;
  }

  async countLinks(): Promise<number> {
    const result = await this.request<{ count: number }>("GET", "/counts/links");
    return result.count;
  }

  // ── Discovery ────────────────────────────────────────

  async listCollections(): Promise<string[]> {
    return this.request("GET", "/collections");
  }

  async listStreams(): Promise<string[]> {
    return this.request("GET", "/streams");
  }

  async listQueues(): Promise<string[]> {
    return this.request("GET", "/queues");
  }

  // ── Aggregate ────────────────────────────────────────

  private async sendAggregateRequest(
    collection: string,
    options: AggregateOptions
  ): Promise<AggregateResult> {
    return this.request("POST", "/aggregate", { collection, ...options });
  }

  readonly aggregate = {
    count: (
      collection: string,
      options: Omit<AggregateOptions, "function"> = {}
    ): Promise<AggregateResult> =>
      this.sendAggregateRequest(collection, { ...options, function: "count" }),

    sum: (
      collection: string,
      field: string,
      options: Omit<AggregateOptions, "function" | "field"> = {}
    ): Promise<AggregateResult> =>
      this.sendAggregateRequest(collection, { ...options, function: "sum", field }),

    avg: (
      collection: string,
      field: string,
      options: Omit<AggregateOptions, "function" | "field"> = {}
    ): Promise<AggregateResult> =>
      this.sendAggregateRequest(collection, { ...options, function: "avg", field }),

    min: (
      collection: string,
      field: string,
      options: Omit<AggregateOptions, "function" | "field"> = {}
    ): Promise<AggregateResult> =>
      this.sendAggregateRequest(collection, { ...options, function: "min", field }),

    max: (
      collection: string,
      field: string,
      options: Omit<AggregateOptions, "function" | "field"> = {}
    ): Promise<AggregateResult> =>
      this.sendAggregateRequest(collection, { ...options, function: "max", field }),
  };

  async timeseries(collection: string, options: TimeSeriesOptions): Promise<TimeSeriesResult> {
    return this.request("POST", "/aggregate/timeseries", {
      collection,
      ...options,
    });
  }

  // ── Schema ───────────────────────────────────────────

  async schema(collection?: string, _options?: SchemaOptions): Promise<CollectionSchema[]> {
    if (collection) {
      return this.request("GET", `/collections/${encodeURIComponent(collection)}/schema`);
    }
    return this.request("GET", "/collections/schema");
  }

  // ── NLQ ──────────────────────────────────────────────

  async nlqQuery(question: string, options?: NlqOptions): Promise<NlqResult> {
    return this.request("POST", "/nlq", { question, ...options });
  }

  readonly nlq = {
    query: (question: string, options?: NlqOptions) => this.nlqQuery(question, options),
  };

  // ── Lifecycle ────────────────────────────────────────

  async close(): Promise<void> {}
}
