import type {
  AggregateOptions,
  AggregateResult,
  CollectionSchema,
  ConnectorAuth,
  ConnectorSchema,
  ConnectorSyncOptions,
  ConnectorSyncResult,
  ListEventsOptions,
  ListObjectsOptions,
  MemoryEvent,
  MemoryObject,
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
  ThingStore,
  TimeSeriesOptions,
  TimeSeriesResult,
} from "../types.js";

export type HttpThingStoreOptions = {
  url: string;
  authToken?: string;
  /** Cloud instance slug for multi-instance routing. Sent as X-Instance-Slug header. */
  instanceSlug?: string;
};

export class HttpThingStore implements ThingStore {
  static async open(urlOrOptions: string | HttpThingStoreOptions): Promise<HttpThingStore> {
    const options = typeof urlOrOptions === "string" ? { url: urlOrOptions } : urlOrOptions;
    return new HttpThingStore(options);
  }

  constructor(private readonly options: HttpThingStoreOptions) {}

  private get base(): string {
    const base = this.options.url.replace(/\/+$/, "");
    return base.endsWith("/v1") ? base : `${base}/v1`;
  }

  private async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const headers: Record<string, string> = {};
    if (body) {
      headers["content-type"] = "application/json";
    }
    if (this.options.authToken) {
      headers.authorization = `Bearer ${this.options.authToken}`;
    }
    if (this.options.instanceSlug) {
      headers["x-instance-slug"] = this.options.instanceSlug;
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
    const result = await this.request<ThingDeleteResult>(
      "DELETE",
      `/objects/${encodeURIComponent(collection)}/${encodeURIComponent(id)}`
    );
    return result;
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
    return this.request<T[]>("GET", `/objects?${params}`);
  }

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
    return this.request<T[]>("GET", `/events?${params}`);
  }

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
    return this.request<QueueJob | null>(
      "POST",
      `/queues/${encodeURIComponent(queue)}/claim`,
      options
    );
  }

  async ackJob(queue: string, jobId: string): Promise<QueueJobResult> {
    return this.request("POST", `/queues/${encodeURIComponent(queue)}/ack`, { jobId });
  }

  async nackJob(
    queue: string,
    jobId: string,
    options: QueueNackOptions = {}
  ): Promise<QueueJobResult> {
    return this.request("POST", `/queues/${encodeURIComponent(queue)}/nack`, { jobId, ...options });
  }

  async listJobs(queue: string): Promise<QueueJob[]> {
    return this.request("GET", `/queues/${encodeURIComponent(queue)}/jobs`);
  }

  async listDeadJobs(queue: string): Promise<QueueJob[]> {
    return this.request("GET", `/queues/${encodeURIComponent(queue)}/dead`);
  }

  async search(query: string, options: MemorySearchOptions = {}): Promise<MemorySearchResult[]> {
    return this.request("POST", "/search", { query, ...options });
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

  async createLink(
    fromRef: string,
    linkType: string,
    toRef: string,
    weight?: number,
    metadataJson?: string
  ): Promise<import("../types.js").Link> {
    return this.request("POST", "/links", { fromRef, linkType, toRef, weight, metadataJson });
  }

  async deleteLink(id: string): Promise<boolean> {
    const result = await this.request<{ deleted: boolean }>(
      "DELETE",
      `/links/${encodeURIComponent(id)}`
    );
    return result.deleted;
  }

  async getLink(id: string): Promise<import("../types.js").Link | null> {
    try {
      return await this.request<import("../types.js").Link>(
        "GET",
        `/links/${encodeURIComponent(id)}`
      );
    } catch (err) {
      if (err instanceof Error && err.message.includes("404")) {
        return null;
      }
      throw err;
    }
  }

  async getNeighbors(
    reference: string,
    direction: import("../types.js").LinkDirection = "Both",
    options: import("../types.js").LinkQueryOptions = {}
  ): Promise<import("../types.js").Link[]> {
    const params = new URLSearchParams({ reference, direction });
    if (options.linkType) {
      params.set("linkType", options.linkType);
    }
    if (options.limit) {
      params.set("limit", String(options.limit));
    }
    return this.request("GET", `/links?${params}`);
  }

  async countObjects(): Promise<number> {
    const result = await this.request<{ count: number }>("GET", "/counts/objects");
    return result.count;
  }

  async countEvents(): Promise<number> {
    const result = await this.request<{ count: number }>("GET", "/counts/events");
    return result.count;
  }

  async countActiveJobs(): Promise<number> {
    const res = await this.request<unknown>("GET", "/health");
    const counts = res as { queues?: number } | undefined;
    return counts?.queues ?? 0;
  }

  async countDeadJobs(): Promise<number> {
    return 0;
  }

  async countLinks(): Promise<number> {
    const result = await this.request<{ count: number }>("GET", "/counts/links");
    return result.count;
  }

  async listCollections(): Promise<string[]> {
    return this.request("GET", "/collections");
  }

  async listStreams(): Promise<string[]> {
    return this.request("GET", "/streams");
  }

  async listQueues(): Promise<string[]> {
    return this.request("GET", "/queues");
  }

  async close(): Promise<void> {}

  async listConnectors(): Promise<string[]> {
    return this.request<string[]>("GET", "/connectors");
  }

  async discoverConnectorSchema(
    type: string,
    query: string,
    auth?: ConnectorAuth
  ): Promise<ConnectorSchema> {
    const body: Record<string, unknown> = { query };
    if (auth) {
      body.auth = auth;
    }
    return this.request<ConnectorSchema>(
      "POST",
      `/connectors/${encodeURIComponent(type)}/schema`,
      body
    );
  }

  async connectorSync(type: string, options: ConnectorSyncOptions): Promise<ConnectorSyncResult> {
    return this.request<ConnectorSyncResult>(
      "POST",
      `/connectors/${encodeURIComponent(type)}/pull`,
      options as Record<string, unknown>
    );
  }

  async aggregate(collection: string, options: AggregateOptions): Promise<AggregateResult> {
    return this.request("POST", "/aggregate", { collection, ...options });
  }

  async timeseries(collection: string, options: TimeSeriesOptions): Promise<TimeSeriesResult> {
    return this.request("POST", "/aggregate/timeseries", { collection, ...options });
  }

  async schema(collection?: string, _options?: SchemaOptions): Promise<CollectionSchema[]> {
    if (collection) {
      return this.request("GET", `/collections/${encodeURIComponent(collection)}/schema`);
    }
    return this.request("GET", "/collections/schema");
  }

  async nlqQuery(question: string, options?: NlqOptions): Promise<NlqResult> {
    return this.request("POST", "/nlq", { question, ...options });
  }
}
