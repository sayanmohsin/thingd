import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import type {
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
      version: options.clientVersion ?? "0.1.0",
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

    return new CloudThingStore(client);
  }

  private constructor(private readonly client: Client) {}

  put(collection: string, object: MemoryObject): Promise<StoredMemoryObject> {
    return this.callTool("thing_put", {
      collection,
      object,
    });
  }

  get(collection: string, id: string): Promise<StoredMemoryObject | null> {
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

  listObjects(collection: string): Promise<StoredMemoryObject[]> {
    return this.callTool("thing_objects_list", {
      collection,
    });
  }

  appendEvent(stream: string, event: MemoryEvent): Promise<StoredMemoryEvent> {
    return this.callTool("thing_events_append", {
      stream,
      event,
    });
  }

  listEvents(stream?: string): Promise<StoredMemoryEvent[]> {
    return this.callTool("thing_events_list", {
      stream,
    });
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
    const result = (await this.client.callTool({
      name,
      arguments: args,
    })) as CallToolResult;

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
