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

  private async callTool<T>(name: string, args: Record<string, unknown>): Promise<T> {
    return this.callToolOnce<T>(name, args, true);
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
