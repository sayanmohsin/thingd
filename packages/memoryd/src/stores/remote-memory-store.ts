import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import type {
  MemoryDeleteResult,
  MemoryEvent,
  MemoryObject,
  MemorySearchOptions,
  MemorySearchResult,
  MemoryStore,
  QueueClaimOptions,
  QueueJob,
  QueueJobOptions,
  QueueJobPayload,
  QueueJobResult,
  QueueNackOptions,
  StoredMemoryEvent,
  StoredMemoryObject,
} from "../types.js";

export type RemoteMemoryStoreOptions = {
  url: string;
  authToken?: string;
  clientName?: string;
  clientVersion?: string;
};

export class RemoteMemoryStore implements MemoryStore {
  static async open(urlOrOptions: string | RemoteMemoryStoreOptions): Promise<RemoteMemoryStore> {
    const options =
      typeof urlOrOptions === "string"
        ? {
            url: urlOrOptions,
          }
        : urlOrOptions;
    const client = new Client({
      name: options.clientName ?? "memoryd-node-sdk",
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

    return new RemoteMemoryStore(client);
  }

  private constructor(private readonly client: Client) {}

  put(collection: string, object: MemoryObject): Promise<StoredMemoryObject> {
    return this.callTool("memory.objects.put", {
      collection,
      object,
    });
  }

  get(collection: string, id: string): Promise<StoredMemoryObject | null> {
    return this.callTool("memory.objects.get", {
      collection,
      id,
    });
  }

  delete(collection: string, id: string): Promise<MemoryDeleteResult> {
    return this.callTool("memory.objects.delete", {
      collection,
      id,
    });
  }

  appendEvent(stream: string, event: MemoryEvent): Promise<StoredMemoryEvent> {
    return this.callTool("memory.events.append", {
      stream,
      event,
    });
  }

  listEvents(stream?: string): Promise<StoredMemoryEvent[]> {
    return this.callTool("memory.events.list", {
      stream,
    });
  }

  pushJob(
    queue: string,
    payload: QueueJobPayload,
    options: QueueJobOptions = {},
  ): Promise<QueueJob> {
    return this.callTool("memory.queue.push", {
      queue,
      payload,
      idempotencyKey: options.idempotencyKey,
      maxAttempts: options.maxAttempts,
      delayMs: options.delayMs,
    });
  }

  claimJob(queue: string, options: QueueClaimOptions = {}): Promise<QueueJob | null> {
    return this.callTool("memory.queue.claim", {
      queue,
      leaseMs: options.leaseMs,
    });
  }

  ackJob(queue: string, jobId: string): Promise<QueueJobResult> {
    return this.callTool("memory.queue.ack", {
      queue,
      id: jobId,
    });
  }

  nackJob(queue: string, jobId: string, options: QueueNackOptions = {}): Promise<QueueJobResult> {
    return this.callTool("memory.queue.nack", {
      queue,
      id: jobId,
      delayMs: options.delayMs,
      error: options.error,
    });
  }

  listJobs(queue: string): Promise<QueueJob[]> {
    return this.callTool("memory.queue.list", {
      queue,
    });
  }

  listDeadJobs(queue: string): Promise<QueueJob[]> {
    return this.callTool("memory.queue.dead", {
      queue,
    });
  }

  search(query: string, options: MemorySearchOptions = {}): Promise<MemorySearchResult[]> {
    return this.callTool("memory.search", {
      query,
      collections: options.collections,
      limit: options.limit,
    });
  }

  async close(): Promise<void> {
    await this.client.close();
  }

  private async callTool<T>(name: string, args: Record<string, unknown>): Promise<T> {
    return parseJsonToolResult<T>(
      (await this.client.callTool({
        name,
        arguments: args,
      })) as CallToolResult,
    );
  }
}

function resolveMcpUrl(value: string): string {
  const normalized = value.startsWith("memoryd://")
    ? `http://${value.slice("memoryd://".length)}`
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
      part.type === "text" && typeof part.text === "string",
  )?.text;

  if (!text) {
    throw new Error("memoryd remote tool did not return JSON text content");
  }

  return JSON.parse(text) as T;
}
