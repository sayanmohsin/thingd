import type { MemoryEvent, ThingD } from "thingd";

export type ThingdMcpAuditOptions = {
  enabled?: boolean;
  actor?: string;
  source?: string;
  stream?: string;
};

export type ThingdMcpAuditMetadata = {
  actor?: string;
  source?: string;
};

type ResolvedThingdMcpAuditOptions = {
  enabled: boolean;
  actor: string;
  source: string;
  stream: string;
};

type ThingdMcpAuditEventOptions = {
  action: string;
  target: Record<string, unknown>;
  metadata?: ThingdMcpAuditMetadata;
  result?: Record<string, unknown>;
};

const DEFAULT_AUDIT_STREAM = "__thingd:mcp:audit";
const DEFAULT_AUDIT_ACTOR = "mcp-client";
const DEFAULT_AUDIT_SOURCE = "thingd-mcp";

export function resolveThingdMcpAuditOptions(
  options: ThingdMcpAuditOptions | false | undefined
): ResolvedThingdMcpAuditOptions {
  if (options === false || options?.enabled === false) {
    return {
      enabled: false,
      actor: DEFAULT_AUDIT_ACTOR,
      source: DEFAULT_AUDIT_SOURCE,
      stream: DEFAULT_AUDIT_STREAM,
    };
  }

  return {
    enabled: true,
    actor: options?.actor ?? DEFAULT_AUDIT_ACTOR,
    source: options?.source ?? DEFAULT_AUDIT_SOURCE,
    stream: options?.stream ?? DEFAULT_AUDIT_STREAM,
  };
}

export async function appendMcpAuditEvent(
  db: ThingD,
  options: ResolvedThingdMcpAuditOptions,
  event: ThingdMcpAuditEventOptions
): Promise<void> {
  if (!options.enabled) {
    return;
  }

  const actor = event.metadata?.actor ?? options.actor;
  const source = event.metadata?.source ?? options.source;
  const auditEvent: MemoryEvent = {
    type: `mcp.${event.action}`,
    text: `MCP ${event.action} by ${actor}`,
    actor,
    source,
    target: event.target,
    result: event.result,
    at: new Date().toISOString(),
  };

  await db.events.append(options.stream, auditEvent);
}
