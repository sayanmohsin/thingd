import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
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
import { SDK_VERSION } from "../version.js";

export type CloudThingStoreOptions = {
  url: string;
  authToken?: string;
  clientName?: string;
  clientVersion?: string;
};

export class CloudThingStore implements ThingStore {
  static async open(urlOrOptions: string | CloudThingStoreOptions): Promise<CloudThingStore> {
    const options =
      typeof urlOrOptions === "string"
        ? {
            url: urlOrOptions,
          }
        : urlOrOptions;
    const client = new Client({
      name: options.clientName ?? "thingd-node-sdk",
      version: options.clientVersion ?? SDK_VERSION,
    });
    const transport = new StreamableHTTPClientTransport(new URL(resolveMcpUrl(options.url)), {
      requestInit: options.authToken
        ? {
            headers: {
              Authorization: `Bearer ${options.authToken}`,
            },
          }
        : undefined,
    });

    await client.connect(transport);

    return new CloudThingStore(client, options);
  }

  private constructor(
    private client: Client,
    private readonly connectOptions: CloudThingStoreOptions
  ) {}

  /**
   * Explicitly reconnect the transport — useful if you detect the connection
   * has dropped and want to recover without recreating the store.
   */
  async reconnect(): Promise<void> {
    try {
      await this.client.close();
    } catch {
      // ignore close errors
    }
    const client = new Client({
      name: this.connectOptions.clientName ?? "thingd-node-sdk",
      version: this.connectOptions.clientVersion ?? SDK_VERSION,
    });
    const transport = new StreamableHTTPClientTransport(
      new URL(resolveMcpUrl(this.connectOptions.url)),
      {
        requestInit: this.connectOptions.authToken
          ? { headers: { Authorization: `Bearer ${this.connectOptions.authToken}` } }
          : undefined,
      }
    );
    await client.connect(transport);
    this.client = client;
  }

  put(collection: string, object: MemoryObject): Promise<StoredMemoryObject> {
    return this.callTool("thing_put", {
      collection,
      object,
    });
  }

  get<T = StoredMemoryObject>(collection: string, id: string): Promise<T | null> {
    return this.callTool("thing_get", {
      collection,
      id,
    });
  }

  delete(collection: string, id: string): Promise<ThingDeleteResult> {
    return this.callTool("thing_delete", {
      collection,
      id,
    });
  }

  listObjects<T = StoredMemoryObject>(
    collection: string,
    options?: ListObjectsOptions
  ): Promise<T[]> {
    const params: Record<string, unknown> = { collection };
    if (options?.filter) {
      params.filter = options.filter;
    }
    if (options?.limit) {
      params.limit = options.limit;
    }
    if (options?.offset) {
      params.offset = options.offset;
    }
    return this.callTool("thing_objects_list", params);
  }

  appendEvent(stream: string, event: MemoryEvent): Promise<StoredMemoryEvent> {
    return this.callTool("thing_events_append", {
      stream,
      event,
    });
  }

  listEvents<T = StoredMemoryEvent>(stream?: string, options?: ListEventsOptions): Promise<T[]> {
    const params: Record<string, unknown> = {};
    if (stream) {
      params.stream = stream;
    }
    if (options?.fromSequence) {
      params.fromSequence = options.fromSequence;
    }
    if (options?.limit) {
      params.limit = options.limit;
    }
    return this.callTool("thing_events_list", params);
  }

  pushJob(
    queue: string,
    payload: QueueJobPayload,
    options: QueueJobOptions = {}
  ): Promise<QueueJob> {
    return this.callTool("thing_queue_push", {
      queue,
      payload,
      idempotencyKey: options.idempotencyKey,
      maxAttempts: options.maxAttempts,
      delayMs: options.delayMs,
    });
  }

  claimJob(queue: string, options: QueueClaimOptions = {}): Promise<QueueJob | null> {
    return this.callTool("thing_queue_claim", {
      queue,
      leaseMs: options.leaseMs,
    });
  }

  ackJob(queue: string, jobId: string): Promise<QueueJobResult> {
    return this.callTool("thing_queue_ack", {
      queue,
      id: jobId,
    });
  }

  nackJob(queue: string, jobId: string, options: QueueNackOptions = {}): Promise<QueueJobResult> {
    return this.callTool("thing_queue_nack", {
      queue,
      id: jobId,
      delayMs: options.delayMs,
      error: options.error,
    });
  }

  listJobs(queue: string): Promise<QueueJob[]> {
    return this.callTool("thing_queue_list", {
      queue,
    });
  }

  listDeadJobs(queue: string): Promise<QueueJob[]> {
    return this.callTool("thing_queue_dead", {
      queue,
    });
  }

  search(query: string, options: MemorySearchOptions = {}): Promise<MemorySearchResult[]> {
    return this.callTool("thing_search", {
      query,
      collections: options.collections,
      limit: options.limit,
      filter: options.filter,
    });
  }

  async countObjects(): Promise<number> {
    return this.callTool("thing_count_objects", {});
  }

  async countEvents(): Promise<number> {
    return this.callTool("thing_count_events", {});
  }

  async countActiveJobs(): Promise<number> {
    return this.callTool("thing_count_active_jobs", {});
  }

  async countDeadJobs(): Promise<number> {
    return this.callTool("thing_count_dead_jobs", {});
  }

  async countLinks(): Promise<number> {
    try {
      const res = (await this.restGet("/v1/counts/links")) as { data?: { count?: number } };
      return res.data?.count ?? 0;
    } catch {
      return 0;
    }
  }

  async putBatch(
    collection: string,
    objects: import("../types.js").MemoryObject[]
  ): Promise<import("../types.js").StoredMemoryObject[]> {
    const results: import("../types.js").StoredMemoryObject[] = [];
    for (const obj of objects) {
      const result = await this.put(collection, obj);
      results.push(result);
    }
    return results;
  }

  async deleteBatch(collection: string, ids: string[]): Promise<number> {
    let count = 0;
    for (const id of ids) {
      const deleted = await this.delete(collection, id);
      if (deleted) {
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
  ): Promise<import("../types.js").Link> {
    const body: Record<string, unknown> = { fromRef, linkType, toRef };
    if (weight !== undefined) {
      body.weight = weight;
    }
    if (metadataJson !== undefined) {
      body.metadataJson = metadataJson;
    }
    const res = (await this.restPost("/v1/links", body)) as { data: import("../types.js").Link };
    return res.data;
  }

  async deleteLink(id: string): Promise<boolean> {
    try {
      await this.restDelete(`/v1/links/${encodeURIComponent(id)}`);
      return true;
    } catch {
      return false;
    }
  }

  async getLink(id: string): Promise<import("../types.js").Link | null> {
    try {
      const res = (await this.restGet(`/v1/links/${encodeURIComponent(id)}`)) as {
        data?: import("../types.js").Link;
      };
      return res.data ?? null;
    } catch {
      return null;
    }
  }

  async getNeighbors(
    reference: string,
    direction: import("../types.js").LinkDirection,
    options: import("../types.js").LinkQueryOptions
  ): Promise<import("../types.js").Link[]> {
    const params = new URLSearchParams({ reference, direction: direction || "Both" });
    if (options.linkType) {
      params.set("linkType", options.linkType);
    }
    if (options.limit !== undefined) {
      params.set("limit", String(options.limit));
    }
    try {
      const res = (await this.restGet(`/v1/links?${params.toString()}`)) as {
        data?: import("../types.js").Link[];
      };
      return res.data ?? [];
    } catch {
      return [];
    }
  }

  async listCollections(): Promise<string[]> {
    return this.callTool("thing_list_collections", {});
  }

  async listStreams(): Promise<string[]> {
    return this.callTool("thing_list_streams", {});
  }

  async listQueues(): Promise<string[]> {
    return this.callTool("thing_list_queues", {});
  }

  async close(): Promise<void> {
    await this.client.close();
  }

  walCheckpoint(): { framesBefore: number; framesAfter: number } {
    throw new Error("WAL checkpoint is not supported for cloud storage driver");
  }

  private async callTool<T>(name: string, args: Record<string, unknown>): Promise<T> {
    return this.callToolOnce<T>(name, args, true);
  }

  private restBaseUrl(): string {
    const url = resolveMcpUrl(this.connectOptions.url);
    return url.replace(/\/mcp$/, "");
  }

  private async restGet(path: string): Promise<unknown> {
    const base = this.restBaseUrl();
    const headers: Record<string, string> = { "Content-Type": "application/json" };
    if (this.connectOptions.authToken) {
      headers.Authorization = `Bearer ${this.connectOptions.authToken}`;
    }
    const res = await fetch(`${base}${path}`, { headers });
    if (!res.ok) {
      throw new Error(`REST request failed: ${res.status} ${res.statusText}`);
    }
    return res.json();
  }

  private async restPost(path: string, body: unknown): Promise<unknown> {
    const base = this.restBaseUrl();
    const headers: Record<string, string> = { "Content-Type": "application/json" };
    if (this.connectOptions.authToken) {
      headers.Authorization = `Bearer ${this.connectOptions.authToken}`;
    }
    const res = await fetch(`${base}${path}`, {
      method: "POST",
      headers,
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      throw new Error(`REST request failed: ${res.status} ${res.statusText}`);
    }
    return res.json();
  }

  private async restDelete(path: string): Promise<unknown> {
    const base = this.restBaseUrl();
    const headers: Record<string, string> = { "Content-Type": "application/json" };
    if (this.connectOptions.authToken) {
      headers.Authorization = `Bearer ${this.connectOptions.authToken}`;
    }
    const res = await fetch(`${base}${path}`, {
      method: "DELETE",
      headers,
    });
    if (!res.ok) {
      throw new Error(`REST request failed: ${res.status} ${res.statusText}`);
    }
    return res.json();
  }

  private async callToolOnce<T>(
    name: string,
    args: Record<string, unknown>,
    retryOnTransportError: boolean
  ): Promise<T> {
    let result: CallToolResult;
    try {
      result = (await this.client.callTool({ name, arguments: args })) as CallToolResult;
    } catch (error) {
      // Transport-level error (connection dropped, ECONNRESET, etc.).
      // Attempt one reconnect and retry before propagating.
      if (retryOnTransportError) {
        try {
          await this.reconnect();
        } catch (reconnectError) {
          throw new Error(
            `thingd cloud: transport error calling "${name}" and reconnect failed: ${reconnectError instanceof Error ? reconnectError.message : String(reconnectError)}`
          );
        }
        return this.callToolOnce<T>(name, args, false);
      }
      throw error;
    }

    if (result.isError) {
      const text = result.content.find(
        (part): part is { type: "text"; text: string } => part.type === "text"
      )?.text;
      throw new Error(text ?? `thingd cloud tool "${name}" returned an error`);
    }

    return parseJsonToolResult<T>(result);
  }
}

function resolveMcpUrl(value: string): string {
  const normalized = value.startsWith("thingd://")
    ? `http://${value.slice("thingd://".length)}`
    : value;
  const url = new URL(normalized);

  if (url.pathname === "" || url.pathname === "/") {
    url.pathname = "/mcp";
  }

  return url.toString();
}

function parseJsonToolResult<T>(result: CallToolResult): T {
  const text = result.content.find(
    (part): part is { type: "text"; text: string } =>
      part.type === "text" && typeof part.text === "string"
  )?.text;

  if (!text) {
    throw new Error("thingd cloud tool did not return JSON text content");
  }

  return JSON.parse(text) as T;
}
