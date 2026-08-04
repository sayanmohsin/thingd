import type { IncomingMessage, ServerResponse } from "node:http";

type SortField = "id" | "collection" | "created_at" | "updated_at" | "version";
type SortDirection = "asc" | "desc";

type LocalSortBy = {
  field: SortField;
  direction?: SortDirection;
};

const MAX_BODY_SIZE = 524_288; // 512KB

function sanitizeForJson(value: unknown): unknown {
  if (value instanceof Error) {
    return { message: value.message };
  }
  if (Array.isArray(value)) {
    return value.map(sanitizeForJson);
  }
  if (value && typeof value === "object") {
    const clean: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value)) {
      if (k !== "stack") {
        clean[k] = sanitizeForJson(v);
      }
    }
    return clean;
  }
  return value;
}

export function readBody(req: IncomingMessage, maxSize = MAX_BODY_SIZE): Promise<string> {
  return new Promise((resolve, reject) => {
    let body = "";
    req.on("data", (chunk: Buffer) => {
      body += chunk.toString();
      if (body.length > maxSize) {
        req.destroy(new Error(`Request body exceeds ${maxSize} bytes`));
        reject(new Error(`Request body exceeds ${maxSize} bytes`));
      }
    });
    req.on("end", () => resolve(body));
    req.on("error", reject);
  });
}

export function sendJson(res: ServerResponse, status: number, data: unknown): void {
  res.writeHead(status, { "Content-Type": "application/json" });
  res.end(JSON.stringify(sanitizeForJson(data)));
}

export function sendData(res: ServerResponse, data: unknown): void {
  sendJson(res, 200, { data });
}

export function sendDataList(res: ServerResponse, data: unknown[], total?: number): void {
  sendJson(res, 200, { data, ...(total !== undefined ? { total } : {}) });
}

export function sendError(
  res: ServerResponse,
  status: number,
  code: string,
  message: string
): void {
  sendJson(res, status, { error: { code, message } });
}

export function parseSortBy(params: URLSearchParams): LocalSortBy | undefined {
  const sort = params.get("sortBy");
  if (!sort) {
    return undefined;
  }
  const parts = sort.split(":");
  const field = parts[0];
  const dir = parts[1] ?? "asc";
  if (!field) {
    return undefined;
  }
  return { field: field as SortField, direction: dir as SortDirection };
}

export function parseFilter(params: URLSearchParams): Record<string, unknown> | undefined {
  const filter: Record<string, unknown> = {};
  let hasFilter = false;
  params.forEach((value, key) => {
    if (key.startsWith("filter.")) {
      const field = key.slice(7);
      filter[field] = value;
      hasFilter = true;
    }
  });
  return hasFilter ? filter : undefined;
}

export function parseIntParam(value: string | null): number | undefined {
  if (!value) {
    return undefined;
  }
  const n = Number(value);
  return Number.isNaN(n) ? undefined : n;
}
