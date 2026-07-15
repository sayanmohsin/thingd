import { spawn } from "node:child_process";
import * as crypto from "node:crypto";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import readline from "node:readline";
import { type MemorySearchOptions, ThingD, type ThingDDriver } from "@thingd/sdk";
import pc from "picocolors";
import { deriveRestUrl, listInstances, listProjects } from "./lib/cloud-api.js";
import {
  readCloudConfig,
  removeCloudConfig,
  resolveCloudUrl,
  writeCloudConfig,
} from "./lib/cloud-config.js";
import { logoText } from "./logo.js";
import { defaultThingdDbPath } from "./paths.js";

// ── Helpers ──────────────────────────────────────────────────────────

function highlightJson(val: unknown): string {
  const str = JSON.stringify(val, null, 2);
  return str.replace(
    /("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+-]?\d+)?)/g,
    (match) => {
      if (/^"/.test(match)) {
        return /:$/.test(match) ? pc.cyan(match) : pc.green(match);
      }
      if (/true|false/.test(match)) {
        return pc.magenta(match);
      }
      if (/null/.test(match)) {
        return pc.dim(match);
      }
      return pc.yellow(match);
    }
  );
}

/** Strip ANSI escape codes to get the visible character count. */
function stripAnsi(s: string): string {
  // biome-ignore lint/suspicious/noControlCharactersInRegex: ANSI escapes require matching the ESC character
  return s.replace(/\u001B\[[0-9;]*[a-zA-Z]/g, "");
}

/** Measure the visible width of a string accounting for wide characters (CJK, emoji). */
function visibleWidth(s: string): number {
  const clean = stripAnsi(s);
  let w = 0;
  for (const ch of clean) {
    const cp = ch.codePointAt(0) ?? 0;
    // Emoji (surrogate pairs / high codepoints) and CJK fullwidth ranges
    if (
      cp > 0xffff ||
      (cp >= 0x1100 && cp <= 0x115f) ||
      (cp >= 0x2e80 && cp <= 0xa4cf) ||
      (cp >= 0xac00 && cp <= 0xd7a3) ||
      (cp >= 0xf900 && cp <= 0xfaff) ||
      (cp >= 0xfe10 && cp <= 0xfe6f) ||
      (cp >= 0xff01 && cp <= 0xff60) ||
      (cp >= 0xffe0 && cp <= 0xffe6) ||
      (cp >= 0x20000 && cp <= 0x2fffd) ||
      (cp >= 0x30000 && cp <= 0x3fffd) ||
      (cp >= 0xfe00 && cp <= 0xfe0f) ||
      (cp >= 0x200d && cp <= 0x200d) ||
      (cp >= 0xe0100 && cp <= 0xe01ef)
    ) {
      w += 2;
    } else {
      w += 1;
    }
  }
  return w;
}

// ── State ────────────────────────────────────────────────────────────// Connection State
let db: ThingD;
let driver = "";
let dbPath = "";
let connected = false;
let authToken = "";
let collections: string[] = [];
let streams: string[] = [];
let queues: string[] = [];
let objectsByCollection = new Map<string, string[]>();
const expandedSet = new Set<string>(["cat:collections", "cat:streams", "cat:queues"]);
let cursorIndex = 0;
let maintenanceCursor = 0;
let scrollOffset = 0;
let startedAt = 0; // ms since epoch when we connected
let totalObjects = 0;
let totalEventsCount = 0;
let totalActiveJobsCount = 0;
let totalDeadJobsCount = 0;
let totalLinksCount = 0;
let cloudError: string | null = null;
const eventsByStream = new Map<
  string,
  { id: string; type: string; createdAt: string; stream: string; text?: string }[]
>();
const jobsByQueue = new Map<
  string,
  {
    active: {
      id: string;
      status: string;
      payload: Record<string, unknown>;
      attempts: number;
      maxAttempts: number;
      createdAt: string;
      lastError?: string;
    }[];
    dead: {
      id: string;
      payload: Record<string, unknown>;
      attempts: number;
      maxAttempts: number;
      createdAt: string;
      lastError?: string;
    }[];
  }
>();
let objectsHistory: number[] = [];
let eventsHistory: number[] = [];
let activeJobsHistory: number[] = [];
let deadJobsHistory: number[] = [];
let dbSizeHistory: number[] = [];
let objectWriteRateHistory: number[] = [];
let eventAppendRateHistory: number[] = [];

let viewerLines: string[] = ["Select an item to view details."];
let viewerScroll = 0;
let lastNeighborsRef = "";
let loadedItemId = "";
let loadTimer: ReturnType<typeof setTimeout> | null = null;
let pollTimer: ReturnType<typeof setInterval> | null = null;
let polling = false;
let keypressHandler: ((str: string, key: Record<string, unknown>) => void) | null = null;

// ── Form State ───────────────────────────────────────────────────────

interface FormField {
  id: string;
  label: string;
  value: string;
  placeholder?: string;
  isSecret?: boolean;
  dirty?: boolean;
  options?: string[];
  allowCustom?: boolean;
}

interface FormState {
  active: boolean;
  title: string;
  fields: FormField[];
  activeIndex: number;
  error?: string;
  isSubmitting?: boolean;
  onSubmit: (values: Record<string, string>) => Promise<void>;
  onCancel: () => void;
}

let formState: FormState | null = null;

function openForm(
  title: string,
  fields: (Partial<FormField> & { id: string; label: string })[],
  onSubmit: (vals: Record<string, string>) => Promise<void>,
  keepViewer?: boolean
) {
  formState = {
    active: true,
    title,
    fields: fields.map((f) => ({
      id: f.id,
      label: f.label,
      value: f.value || (f.options?.[0] ?? ""),
      placeholder: f.placeholder,
      isSecret: f.isSecret,
      dirty: false,
      options: f.options,
      allowCustom: f.allowCustom,
    })),
    activeIndex: 0,
    onCancel: () => {
      formState = null;
      viewerLines = [];
      loadedItemId = ""; // Force reload
      draw();
      const n = buildTree()[cursorIndex];
      if (n) {
        scheduleLoad(n);
      }
    },
    onSubmit: async (vals) => {
      if (!formState) {
        return;
      }
      formState.isSubmitting = true;
      formState.error = undefined;
      draw();
      try {
        await onSubmit(vals);
        formState = null;
        if (keepViewer) {
          draw();
          return;
        }
        viewerLines = [];
        loadedItemId = ""; // Force reload
        await fetchResources();
        draw();
        const n = buildTree()[cursorIndex];
        if (n) {
          scheduleLoad(n);
        }
      } catch (err: unknown) {
        if (formState) {
          formState.error =
            err instanceof Error ? err.message : String(err) || "Unknown error occurred";
          formState.isSubmitting = false;
          draw();
        }
      }
    },
  };
  viewerScroll = 0;
  draw();
}

// ── Data Fetching ────────────────────────────────────────────────────

const SPARK_WIDTH = 30;

function drawSparkline(data: number[], baselineMax = 0, width = SPARK_WIDTH): string {
  const dataChars = ["\u2581", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
  const track = "\u2581"; // Lower 1/8 block as baseline

  if (data.length === 0) {
    return track.repeat(width);
  }

  const recent = data.slice(-width);
  const padLen = width - recent.length;
  const max = Math.max(baselineMax, ...recent);

  // Left pad = no data yet
  let result = track.repeat(padLen);

  if (max === 0) {
    result += track.repeat(recent.length);
    return result;
  }

  result += recent
    .map((v) => {
      if (v === 0) {
        return track;
      }
      const ratio = v / max;
      const idx = Math.max(0, Math.min(dataChars.length - 1, Math.floor(ratio * dataChars.length)));

      return dataChars[idx] ?? dataChars[0] ?? "▁";
    })
    .join("");

  return result;
}

function formatUptime(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) {
    return `${s}s`;
  }
  const m = Math.floor(s / 60);
  if (m < 60) {
    return `${m}m ${s % 60}s`;
  }
  const h = Math.floor(m / 60);
  return `${h}h ${m % 60}m`;
}

async function fetchResourcesFallback() {
  cloudError = null;

  // Collections and streams — parallel fetch
  let nativeCollections: string[];
  let nativeStreams: string[];
  try {
    [nativeCollections, nativeStreams] = await Promise.all([
      db.listCollections(),
      db.listStreams(),
    ]);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    cloudError = `Failed to load resources: ${msg}`;
    nativeCollections = [];
    nativeStreams = [];
    if (viewerLines.length === 1 && viewerLines[0] === "Select an item to view details.") {
      viewerLines = [pc.yellow(cloudError), pc.dim("Press 'r' to retry or 's' to switch driver.")];
    }
  }

  collections = [...new Set(nativeCollections)].sort();
  streams = [...new Set(nativeStreams)].sort();

  // Counts, queues, links — all parallel, each with independent error handling
  const [objCount, evtCount, activeCount, deadCount, listedQueues, linkCount] = await Promise.all([
    db.countObjects().catch(() => 0),
    db.countEvents().catch(() => 0),
    db.countActiveJobs().catch(() => 0),
    db.countDeadJobs().catch(() => 0),
    db.listQueues().catch(() => {
      cloudError = cloudError ?? "Failed to load queues";
      return [] as string[];
    }),
    db.countLinks().catch(() => 0),
  ]);

  totalObjects = objCount;
  totalEventsCount = evtCount;
  totalActiveJobsCount = activeCount;
  totalDeadJobsCount = deadCount;
  queues = [...new Set(listedQueues ?? [])].sort();
  totalLinksCount = linkCount;

  // Objects per collection — parallel
  objectsByCollection.clear();
  await Promise.all(
    collections.map(async (col) => {
      try {
        const list = await db.listObjects(col);
        objectsByCollection.set(
          col,
          list.map((o: { id: string }) => o.id)
        );
      } catch {
        objectsByCollection.set(col, []);
      }
    })
  );
}

async function fetchResources(): Promise<void> {
  if (driver === "native" && dbPath) {
    try {
      // Override the tracked totals with the actual exact DB count!
      const [
        objCount,
        evtCount,
        activeCount,
        deadCount,
        linkCount,
        nativeCollections,
        nativeStreams,
        nativeQueues,
      ] = await Promise.all([
        db.countObjects(),
        db.countEvents(),
        db.countActiveJobs(),
        db.countDeadJobs(),
        db.countLinks(),
        db.listCollections(),
        db.listStreams(),
        db.listQueues?.() ?? Promise.resolve([]),
      ]);

      totalObjects = Number.isNaN(objCount) || objCount === 0 ? totalObjects : objCount;
      totalEventsCount = Number.isNaN(evtCount) || evtCount === 0 ? totalEventsCount : evtCount;
      totalActiveJobsCount =
        Number.isNaN(activeCount) || activeCount === 0 ? totalActiveJobsCount : activeCount;
      totalDeadJobsCount =
        Number.isNaN(deadCount) || deadCount === 0 ? totalDeadJobsCount : deadCount;
      totalLinksCount = Number.isNaN(linkCount) ? totalLinksCount : linkCount;

      collections = nativeCollections.length > 0 ? nativeCollections : [];
      streams = nativeStreams.length > 0 ? nativeStreams : [];
      queues = nativeQueues.length > 0 ? nativeQueues : [];
    } catch {
      // Fallback if sqlite3 fails
      await fetchResourcesFallback();
    }
  } else {
    await fetchResourcesFallback();
  }

  // Populate objectsByCollection from live data
  objectsByCollection.clear();
  for (const col of collections) {
    try {
      const list = await db.listObjects(col);
      objectsByCollection.set(
        col,
        list.map((o: { id: string }) => o.id)
      );
    } catch {
      objectsByCollection.set(col, []);
    }
  }

  // Populate events per stream
  eventsByStream.clear();
  await Promise.all(
    streams.map(async (s) => {
      try {
        const evts = await db.events.list(s, { limit: 20 });
        eventsByStream.set(s, evts);
      } catch {
        eventsByStream.set(s, []);
      }
    })
  );

  // Populate jobs per queue
  jobsByQueue.clear();
  await Promise.all(
    queues.map(async (q) => {
      try {
        const active = await db.queue(q).list();
        const dead = await db.queue(q).dead();
        jobsByQueue.set(q, { active, dead });
      } catch {
        jobsByQueue.set(q, { active: [], dead: [] });
      }
    })
  );

  // Calculate Deltas for Operations Throughput Rates
  const prevObjects =
    objectsHistory.length > 0
      ? (objectsHistory[objectsHistory.length - 1] ?? totalObjects)
      : totalObjects;
  const prevEvents =
    eventsHistory.length > 0
      ? (eventsHistory[eventsHistory.length - 1] ?? totalEventsCount)
      : totalEventsCount;

  const objectWriteRate = Math.max(0, Math.round((totalObjects - prevObjects) / 10));
  const eventAppendRate = Math.max(0, Math.round((totalEventsCount - prevEvents) / 10));

  // Push Histories with Initial Pre-population to prevent misleading growth wiggles
  if (objectsHistory.length === 0) {
    objectsHistory = new Array(60).fill(totalObjects);
  } else {
    objectsHistory.push(totalObjects);
    if (objectsHistory.length > 60) {
      objectsHistory.shift();
    }
  }

  if (eventsHistory.length === 0) {
    eventsHistory = new Array(60).fill(totalEventsCount);
  } else {
    eventsHistory.push(totalEventsCount);
    if (eventsHistory.length > 60) {
      eventsHistory.shift();
    }
  }

  if (activeJobsHistory.length === 0) {
    activeJobsHistory = new Array(60).fill(totalActiveJobsCount);
  } else {
    activeJobsHistory.push(totalActiveJobsCount);
    if (activeJobsHistory.length > 60) {
      activeJobsHistory.shift();
    }
  }

  if (deadJobsHistory.length === 0) {
    deadJobsHistory = new Array(60).fill(totalDeadJobsCount);
  } else {
    deadJobsHistory.push(totalDeadJobsCount);
    if (deadJobsHistory.length > 60) {
      deadJobsHistory.shift();
    }
  }

  if (objectWriteRateHistory.length === 0) {
    objectWriteRateHistory = new Array(60).fill(objectWriteRate);
  } else {
    objectWriteRateHistory.push(objectWriteRate);
    if (objectWriteRateHistory.length > 60) {
      objectWriteRateHistory.shift();
    }
  }

  if (eventAppendRateHistory.length === 0) {
    eventAppendRateHistory = new Array(60).fill(eventAppendRate);
  } else {
    eventAppendRateHistory.push(eventAppendRate);
    if (eventAppendRateHistory.length > 60) {
      eventAppendRateHistory.shift();
    }
  }

  // Database Size (only if native)
  let sizeKb = 0;
  if (driver === "native" && dbPath) {
    try {
      sizeKb = Math.round(fs.statSync(dbPath).size / 1024);
    } catch {
      sizeKb = 0;
    }
  }

  if (dbSizeHistory.length === 0) {
    dbSizeHistory = new Array(60).fill(sizeKb);
  } else {
    dbSizeHistory.push(sizeKb);
    if (dbSizeHistory.length > 60) {
      dbSizeHistory.shift();
    }
  }
}

// ── Connection helpers ───────────────────────────────────────────────

async function connectToDriver(
  selectedDriver: ThingDDriver,
  resolvedPath: string,
  url?: string,
  token?: string,
  instanceSlug?: string
): Promise<void> {
  db = await ThingD.open({
    path: resolvedPath,
    url,
    driver: selectedDriver,
    authToken: token,
    instanceSlug,
  });

  driver = selectedDriver;
  dbPath = resolvedPath;
  authToken = typeof token === "string" ? token : "";

  connected = true;
  startedAt = Date.now();
  cursorIndex = 0;
  scrollOffset = 0;
  loadedItemId = "";

  await fetchResources();
  draw();
  const t = buildTree();
  const first = t[cursorIndex];
  if (first) {
    scheduleLoad(first);
  }
}

// ── Tree Model ───────────────────────────────────────────────────────

interface TreeNode {
  id: string;
  parentId?: string;
  type:
    | "category"
    | "collection"
    | "object"
    | "stream"
    | "queue"
    | "event"
    | "job"
    | "link"
    | "status"
    | "driver"
    | "maintenance";
  label: string;
  depth: number;
  expandable: boolean;
  ref?: Record<string, unknown>;
  children?: { id: string; label: string }[];
}

function buildTree(): TreeNode[] {
  if (!connected) {
    return [
      {
        id: "drv:memory",
        type: "driver",
        label: `${pc.cyan("●")} ${pc.bold("Memory")}    ${pc.dim("ephemeral")}`,
        depth: 0,
        expandable: false,
        ref: { driver: "memory" },
      },
      {
        id: "drv:native",
        type: "driver",
        label: `${pc.cyan("●")} ${pc.bold("Native")}    ${pc.dim("SQLite file")}`,
        depth: 0,
        expandable: false,
        ref: { driver: "native" },
      },
      {
        id: "drv:cloud",
        type: "driver",
        label: `${pc.cyan("●")} ${pc.bold("Cloud")}     ${pc.dim("remote")}`,
        depth: 0,
        expandable: false,
        ref: { driver: "cloud" },
      },
    ];
  }

  const nodes: TreeNode[] = [];

  // Collections
  const colsOpen = expandedSet.has("cat:collections");
  nodes.push({
    id: "cat:collections",
    type: "category",
    label: `${colsOpen ? pc.cyan("▾") : pc.dim("▸")} ${pc.bold("Collections")}`,
    depth: 0,
    expandable: true,
  });
  if (colsOpen) {
    if (collections.length === 0) {
      nodes.push({
        id: "empty:collections",
        type: "status",
        label: pc.dim("(empty)"),
        depth: 1,
        expandable: false,
      });
    }
    for (const col of collections) {
      const colId = `col:${col}`;
      const colOpen = expandedSet.has(colId);
      nodes.push({
        id: colId,
        parentId: "cat:collections",
        type: "collection",
        label: `${colOpen ? pc.cyan("▾") : pc.dim("▸")} ${pc.cyan(col)}`,
        depth: 1,
        expandable: true,
        ref: { name: col },
      });
      if (colOpen) {
        const objs = objectsByCollection.get(col) ?? [];
        if (objs.length === 0) {
          nodes.push({
            id: `empty:${col}`,
            parentId: colId,
            type: "status",
            label: pc.dim("(no objects)"),
            depth: 2,
            expandable: false,
          });
        }
        for (const objId of objs) {
          nodes.push({
            id: `obj:${col}:${objId}`,
            parentId: colId,
            type: "object",
            label: `${pc.cyan("○")} ${objId}`,
            depth: 2,
            expandable: false,
            ref: { collection: col, id: objId },
          });
        }
      }
    }
  }

  // Streams
  const strsOpen = expandedSet.has("cat:streams");
  nodes.push({
    id: "cat:streams",
    type: "category",
    label: `${strsOpen ? pc.cyan("▾") : pc.dim("▸")} ${pc.bold("Streams")}`,
    depth: 0,
    expandable: true,
  });
  if (strsOpen) {
    if (streams.length === 0) {
      nodes.push({
        id: "empty:streams",
        type: "status",
        label: pc.dim("(empty)"),
        depth: 1,
        expandable: false,
      });
    }
    for (const stream of streams) {
      const sOpen = expandedSet.has(`stream:${stream}`);
      nodes.push({
        id: `stream:${stream}`,
        parentId: "cat:streams",
        type: "stream",
        label: `${sOpen ? pc.cyan("▾") : pc.dim("▸")} ${pc.green(stream)}`,
        depth: 1,
        expandable: true,
        ref: { name: stream },
      });
      if (sOpen) {
        const evts = eventsByStream.get(stream) ?? [];
        if (evts.length === 0) {
          nodes.push({
            id: `empty:evt:${stream}`,
            parentId: `stream:${stream}`,
            type: "status",
            label: pc.dim("(no events)"),
            depth: 2,
            expandable: false,
          });
        }
        for (const evt of evts) {
          nodes.push({
            id: `evt:${stream}:${evt.id}`,
            parentId: `stream:${stream}`,
            type: "event",
            label: `${pc.dim("·")} ${pc.dim(evt.type || "unknown")}`,
            depth: 2,
            expandable: false,
            ref: { stream: stream, eventId: evt.id, eventData: evt },
          });
        }
      }
    }
  }

  // Queues
  const qOpen = expandedSet.has("cat:queues");
  nodes.push({
    id: "cat:queues",
    type: "category",
    label: `${qOpen ? pc.cyan("▾") : pc.dim("▸")} ${pc.bold("Queues")}`,
    depth: 0,
    expandable: true,
  });
  if (qOpen) {
    if (queues.length === 0) {
      nodes.push({
        id: "empty:queues",
        type: "status",
        label: pc.dim("(empty)"),
        depth: 1,
        expandable: false,
      });
    }
    for (const q of queues) {
      const qOpen = expandedSet.has(`queue:${q}`);
      nodes.push({
        id: `queue:${q}`,
        parentId: "cat:queues",
        type: "queue",
        label: `${qOpen ? pc.cyan("▾") : pc.dim("▸")} ${pc.magenta(q)}`,
        depth: 1,
        expandable: true,
        ref: { name: q },
      });
      if (qOpen) {
        const jobData = jobsByQueue.get(q);
        const activeJobs = jobData?.active ?? [];
        const deadJobs = jobData?.dead ?? [];

        // Active jobs subcategory
        const activeOpen = expandedSet.has(`queue:${q}:active`);
        nodes.push({
          id: `queue:${q}:active`,
          parentId: `queue:${q}`,
          type: "category",
          label: `${activeOpen ? pc.cyan("▾") : pc.dim("▸")} ${pc.bold("Active")}  ${pc.dim(`(${activeJobs.length})`)}`,
          depth: 2,
          expandable: true,
        });
        if (activeOpen) {
          if (activeJobs.length === 0) {
            nodes.push({
              id: `empty:active:${q}`,
              parentId: `queue:${q}:active`,
              type: "status",
              label: pc.dim("(no active jobs)"),
              depth: 3,
              expandable: false,
            });
          }
          for (const job of activeJobs) {
            nodes.push({
              id: `job:${q}:active:${job.id}`,
              parentId: `queue:${q}:active`,
              type: "job",
              label: `${pc.cyan("●")} ${pc.dim(job.id.slice(0, 12))}`,
              depth: 3,
              expandable: false,
              ref: { queue: q, jobId: job.id, status: "active", jobData: job },
            });
          }
        }

        // Dead jobs subcategory
        const deadOpen = expandedSet.has(`queue:${q}:dead`);
        nodes.push({
          id: `queue:${q}:dead`,
          parentId: `queue:${q}`,
          type: "category",
          label: `${deadOpen ? pc.cyan("▾") : pc.dim("▸")} ${pc.bold("Dead")}  ${pc.dim(`(${deadJobs.length})`)}`,
          depth: 2,
          expandable: true,
        });
        if (deadOpen) {
          if (deadJobs.length === 0) {
            nodes.push({
              id: `empty:dead:${q}`,
              parentId: `queue:${q}:dead`,
              type: "status",
              label: pc.dim("(no dead jobs)"),
              depth: 3,
              expandable: false,
            });
          }
          for (const job of deadJobs) {
            nodes.push({
              id: `job:${q}:dead:${job.id}`,
              parentId: `queue:${q}:dead`,
              type: "job",
              label: `${pc.red("○")} ${pc.dim(job.id.slice(0, 12))}`,
              depth: 3,
              expandable: false,
              ref: { queue: q, jobId: job.id, status: "dead", jobData: job },
            });
          }
        }
      }
    }
  }

  // Links
  nodes.push({
    id: "node:links",
    type: "link",
    label: `${pc.blue("◈")} ${pc.dim("Links")}  ${pc.dim(`(${totalLinksCount})`)}`,
    depth: 0,
    expandable: false,
  });

  // Metrics
  nodes.push({
    id: "node:status",
    type: "status",
    label: `${pc.cyan("◉")} ${pc.dim("Metrics")}`,
    depth: 0,
    expandable: false,
  });

  // Maintenance
  nodes.push({
    id: "node:maintenance",
    type: "maintenance",
    label: `${pc.yellow("⬡")} ${pc.bold("Maintenance")}`,
    depth: 0,
    expandable: true,
    children: [
      { id: "maintenance:integrity", label: "Run Health Check" },
      { id: "maintenance:checkpoint", label: "WAL Checkpoint" },
      { id: "maintenance:backup", label: "Create Backup" },
    ],
  });

  return nodes;
}

function scheduleLoad(node: TreeNode) {
  if (!connected) {
    // Show driver info in viewer
    if (node.type === "driver" && node.ref) {
      const d = node.ref.driver as string;
      const info = [
        ` ${logoText()} ${pc.dim("— local data engine")}`,
        "",
        ` ${pc.bold(d === "memory" ? "Memory Driver" : d === "native" ? "Native Driver" : "Cloud Driver")}`,
        "",
        d === "memory"
          ? ` Ephemeral in-memory database.\n All data is destroyed on exit.\n\n ${pc.dim("Best for: testing, prototyping")}\n\n ${pc.dim("Press")} ${pc.bold("Enter")} ${pc.dim("to connect.")}`
          : d === "native"
            ? ` Persistent SQLite database.\n Data is stored on disk.\n\n ${pc.dim("Best for: local development, single-node")}\n\n ${pc.dim("Press")} ${pc.bold("Enter")} ${pc.dim("to connect.")}`
            : (() => {
                const hasCfg = !!readCloudConfig()?.token;
                return hasCfg
                  ? ` Connect to a remote thingd instance.\n Requires a URL and optional auth token.\n\n ${pc.dim("Best for: production, multi-node")}\n\n ${pc.dim("Press")} ${pc.bold("Enter")} ${pc.dim("to connect.")}`
                  : ` ${pc.yellow("Not logged in to thingd Cloud.")}\n\n Run ${pc.cyan("thingd cloud login")} to authenticate, or\n press ${pc.bold("Enter")} to connect with a URL and token manually.`;
              })(),
      ].join("\n");
      viewerLines = info.split("\n");
      loadedItemId = node.id;
    }
    return;
  }
  if (loadTimer) {
    clearTimeout(loadTimer);
  }
  if (loadedItemId === node.id) {
    return;
  }

  loadedItemId = node.id;
  viewerLines = [pc.dim("Loading...")];
  viewerScroll = 0;
  draw();

  loadTimer = setTimeout(async () => {
    await loadContent(node);
  }, 80);
}

async function loadContent(node: TreeNode): Promise<void> {
  const snapId = node.id;
  try {
    let content = "";

    if (node.type === "object" && node.ref) {
      const ref = node.ref as { collection: string; id: string };
      const data = await db.get(ref.collection, ref.id);
      content = data ? highlightJson(data) : pc.yellow("Object not found.");
    } else if (node.type === "collection" && node.ref) {
      const ref = node.ref as { name: string };
      const objs = objectsByCollection.get(ref.name) ?? [];
      let res = `${pc.bold(ref.name)} ${pc.dim(`(${objs.length} objects)`)}\n\n`;

      if (objs.length === 0) {
        res += pc.dim("No objects in this collection.");
      } else {
        const lines = objs.map((id) => ` ${pc.cyan("○")} ${id}`);
        res += lines.join("\n");
      }
      content = res;
    } else if (node.type === "stream" && node.ref) {
      const ref = node.ref as { name: string };
      const evts = eventsByStream.get(ref.name) ?? [];
      let res = `${pc.bold(ref.name)} ${pc.dim(`(${evts.length} events shown)`)}\n\n`;
      res += pc.dim("Expand to browse individual events, or press [c] to append.\n");
      content = res;
    } else if (node.type === "event" && node.ref) {
      const ref = node.ref as {
        stream: string;
        eventId: string;
        eventData: {
          id: string;
          type: string;
          createdAt: string;
          stream: string;
          text?: string;
          [key: string]: unknown;
        };
      };
      const evt = ref.eventData;
      let res = `${pc.bold(evt.type || "unknown")} ${pc.dim(evt.id)}\n`;
      res += ` ${pc.dim("Stream:")} ${pc.green(ref.stream)}\n`;
      res += ` ${pc.dim("Created:")} ${pc.dim(evt.createdAt || "—")}\n\n`;
      const display = { ...evt };
      for (const k of ["id", "stream", "sequence", "createdAt", "idempotencyKey"]) {
        delete (display as Record<string, unknown>)[k];
      }
      if (Object.keys(display).length > 0) {
        res += highlightJson(display);
      } else {
        res += pc.dim("No payload.");
      }
      content = res;
    } else if (node.type === "queue" && node.ref) {
      const ref = node.ref as { name: string };
      const jobData = jobsByQueue.get(ref.name);
      const active = jobData?.active ?? [];
      const dead = jobData?.dead ?? [];
      let res = `${pc.bold(ref.name)}\n\n`;
      res += `${pc.cyan("Active")} ${pc.dim(`(${active.length})`)}\n`;
      res += `${pc.red("Dead")} ${pc.dim(`(${dead.length})`)}\n\n`;
      res += pc.dim("Expand Active or Dead to browse jobs, [c] to push new job.");
      content = res;
    } else if (node.type === "job" && node.ref) {
      const ref = node.ref as {
        queue: string;
        jobId: string;
        status: string;
        jobData: {
          id: string;
          status?: string;
          payload?: Record<string, unknown>;
          attempts: number;
          maxAttempts: number;
          createdAt: string;
          lastError?: string;
        };
      };
      const job = ref.jobData;
      let res = `${pc.bold(job.id)}  ${pc.yellow(job.status || ref.status)}\n`;
      res += ` ${pc.dim("Queue:")} ${pc.magenta(ref.queue)}\n`;
      res += ` ${pc.dim("Attempts:")} ${job.attempts}/${job.maxAttempts}\n`;
      res += ` ${pc.dim("Created:")} ${pc.dim(job.createdAt || "—")}\n`;
      if (job.lastError) {
        res += ` ${pc.dim("Error:")} ${pc.red(job.lastError)}\n`;
      }
      res += "\n";
      if (job.payload && Object.keys(job.payload).length > 0) {
        res += highlightJson(job.payload);
      } else {
        res += pc.dim("No payload.");
      }
      if (ref.status === "dead") {
        res += `\n\n${pc.dim("[e] Retry (ack)  [d] Nack (remove from dead letter)")}`;
      }
      content = res;
    } else if (node.type === "status") {
      const W = process.stdout.columns || 80;
      const sideW = Math.min(40, Math.max(20, Math.floor(W * 0.35)));
      const viewW = Math.max(20, W - sideW - 3);

      const uptime = startedAt ? formatUptime(Date.now() - startedAt) : "--";

      // ── Header
      const pathStr = pc.dim(dbPath || ":memory:");
      const pathRaw = dbPath || ":memory:";
      const titleStr = `${pc.bold("thingd")}  ${pc.cyan("METRICS")}`;
      const gap = Math.max(2, viewW - 2 - 8 - "METRICS".length - pathRaw.length);
      content = ` ${titleStr}${" ".repeat(gap)}${pathStr}\n`;
      content += ` ${pc.dim("uptime")} ${pc.dim(uptime)}\n\n`;

      // ── Physical Store & Driver Logic
      let sizeKb = 0;
      if (driver === "native" && dbPath) {
        try {
          sizeKb = Math.round(fs.statSync(dbPath).size / 1024);
        } catch {
          sizeKb = 0;
        }
      }
      const dbSizeStr = driver === "native" ? `${sizeKb} KB` : "--";

      let driverName = "Unknown";
      if (driver === "memory") {
        driverName = "In-Memory";
      } else if (driver === "native") {
        driverName = "SQLite";
      } else if (driver === "cloud") {
        driverName = "Cloud";
      }

      // ── Metrics Layout (opencode style: clean groups, no horizontal rules)
      content += ` ${pc.bold("Capacity & Storage")}\n`;
      content += ` ${pc.dim("Objects".padEnd(14))} ${pc.cyan(String(totalObjects).padEnd(6))} ${pc.dim("total")}\n`;
      content += ` ${pc.dim("Events".padEnd(14))} ${pc.green(String(totalEventsCount).padEnd(6))} ${pc.dim("total")}\n`;
      content += ` ${pc.dim("Links".padEnd(14))} ${pc.blue(String(totalLinksCount).padEnd(6))} ${pc.dim("total")}\n`;
      content += ` ${pc.dim("Active Jobs".padEnd(14))} ${pc.yellow(String(totalActiveJobsCount).padEnd(6))} ${pc.dim("in flight")}\n`;
      content += ` ${pc.dim("Dead Jobs".padEnd(14))} ${pc.red(String(totalDeadJobsCount).padEnd(6))} ${pc.dim("failed")}\n\n`;

      content += ` ${pc.bold("Connection")}\n`;
      content += ` ${pc.dim("Driver".padEnd(14))} ${driverName}\n`;
      content += ` ${pc.dim("Path".padEnd(14))} ${dbPath || ":memory:"}\n`;
      content += ` ${pc.dim("Size".padEnd(14))} ${dbSizeStr}\n`;
      if (driver === "native") {
        const sizeSpark = drawSparkline(dbSizeHistory, 5, Math.max(10, viewW - 55));
        content += ` ${pc.dim("Size History".padEnd(14))} ${pc.cyan(sizeSpark)}\n`;
      }
      content += `\n`;

      // ── Throughput & Activity Metrics
      const currentWrite = objectWriteRateHistory[objectWriteRateHistory.length - 1] ?? 0;
      const currentAppend = eventAppendRateHistory[eventAppendRateHistory.length - 1] ?? 0;

      // Adjust sparkline width to prevent terminal wrapping.
      const sparkW = Math.max(10, viewW - 55);

      const wLine = drawSparkline(objectWriteRateHistory, 5, sparkW);
      const apLine = drawSparkline(eventAppendRateHistory, 5, sparkW);

      content += ` ${pc.bold("Throughput & Activity")}\n`;
      content += ` ${pc.dim("Writes".padEnd(14))} ${pc.cyan(wLine)}  ${pc.cyan(String(currentWrite).padEnd(4))} ${pc.dim(`w/s`)}\n`;
      content += ` ${pc.dim("Appends".padEnd(14))} ${pc.green(apLine)}  ${pc.green(String(currentAppend).padEnd(4))} ${pc.dim(`e/s`)}\n\n`;

      content += ` ${pc.dim("Shortcuts:")} ${pc.bold("[c]")} Create  ${pc.bold("[r]")} Refresh  ${pc.bold("[/]")} Search\n`;

      if (cloudError) {
        content += `\n ${pc.yellow("⚠")} ${pc.dim(cloudError)}\n`;
      }
    } else if (node.type === "link") {
      content = [
        ` ${pc.bold("Links")}  ${pc.dim(`(${totalLinksCount} total`)}${lastNeighborsRef ? pc.dim(`, last browsed: ${lastNeighborsRef}`) : ""})`,
        "",
        totalLinksCount === 0
          ? ` ${pc.dim("No links yet.")}`
          : ` ${pc.dim("Select an object and press")} ${pc.bold("n")} ${pc.dim("to browse its neighbors.")}`,
        "",
        ` ${pc.bold("Operations")}`,
        ` ${pc.bold("[c]")} Create a new link`,
        ` ${pc.bold("[d]")} Delete a link by ID`,
      ].join("\n");
    } else if (node.type === "category") {
      content = pc.dim("Expand to browse items.");
    } else {
      content = "";
    }

    if (loadedItemId === snapId) {
      viewerLines = content.split("\n");
      draw();
    }
  } catch (err: unknown) {
    if (loadedItemId === snapId) {
      viewerLines = [pc.red(`Error: ${err instanceof Error ? err.message : String(err)}`)];
      draw();
    }
  }
}

// ── Rendering ────────────────────────────────────────────────────────

function draw() {
  const W = process.stdout.columns || 80;
  const H = process.stdout.rows || 24;
  const sideW = Math.min(40, Math.max(20, Math.floor(W * 0.35)));
  const viewW = Math.max(1, W - sideW - 3); // 3 = " | "
  const bodyH = Math.max(1, H - 4); // header(1) + separator(1) + separator(1) + footer(1)

  const tree = buildTree();

  // Clamp cursor
  if (tree.length === 0) {
    cursorIndex = 0;
  } else if (cursorIndex >= tree.length) {
    cursorIndex = tree.length - 1;
  }
  if (cursorIndex < 0) {
    cursorIndex = 0;
  }

  // Scroll sidebar
  if (cursorIndex >= scrollOffset + bodyH) {
    scrollOffset = cursorIndex - bodyH + 1;
  } else if (cursorIndex < scrollOffset) {
    scrollOffset = cursorIndex;
  }
  scrollOffset = Math.max(0, Math.min(scrollOffset, Math.max(0, tree.length - bodyH)));

  let buf = "\u001B[H"; // Move to top-left

  // Header — opencode style: clean, no inverse bar
  if (!connected) {
    buf += ` ${pc.cyan("◈")} ${pc.bold("thingd")}  ${pc.dim("Select Environment")}\n`;
  } else if (formState?.active) {
    buf += ` ${pc.cyan("◈")} ${pc.bold("thingd")}  ${pc.cyan(driver.toUpperCase())}  ${pc.dim("Input Mode")}\n`;
  } else {
    const label = ` ${pc.cyan("◈")} ${pc.bold("thingd")}  ${pc.cyan(driver.toUpperCase())} ${pc.dim(dbPath)}`;
    buf += `${padToWidth(label, W)}\n`;
  }
  buf += `${pc.dim("─".repeat(W))}\n`;

  // Build Form Lines if active
  if (formState?.active) {
    viewerLines = [` ${pc.cyan(formState.title)}`, ""];
    for (let i = 0; i < formState.fields.length; i++) {
      const f = formState.fields[i];
      if (!f) {
        continue;
      }
      const isSel = i === formState.activeIndex;
      let displayLabel = f.label;
      if (f.options && f.allowCustom && f.value && !f.options.includes(f.value)) {
        displayLabel += pc.green(" (New)");
      }
      viewerLines.push(`${isSel ? pc.cyan("▸") : " "} ${pc.bold(displayLabel)}`);

      let displayVal = f.value;
      if (f.isSecret) {
        displayVal = "*".repeat(displayVal.length);
      }
      if (displayVal === "" && f.placeholder) {
        displayVal = pc.dim(f.placeholder);
      }

      if (isSel && !formState.isSubmitting) {
        if (f.options && !f.allowCustom) {
          viewerLines.push(`   ${pc.cyan("◀ ")}${pc.inverse(displayVal || " ")}${pc.cyan(" ▶")}`);
        } else if (f.options && f.allowCustom) {
          const inOptions = f.options.includes(f.value);
          if (inOptions) {
            viewerLines.push(`   ${pc.cyan("◀ ")}${displayVal}${pc.inverse(" ")}${pc.cyan(" ▶")}`);
          } else {
            viewerLines.push(`   ${displayVal}${pc.inverse(" ")}`);
          }
        } else {
          viewerLines.push(`   ${displayVal}${pc.inverse(" ")}`); // cursor block
        }
      } else {
        viewerLines.push(`   ${displayVal}`);
      }
      viewerLines.push("");
    }
    if (formState.error) {
      viewerLines.push(` ${pc.red(formState.error)}`);
    }
    if (formState.isSubmitting) {
      viewerLines.push(` ${pc.cyan("Processing...")}`);
    }
    viewerLines.push("");
    viewerLines.push(pc.dim(" [Enter] Next/Submit   [Esc] Cancel"));
  }

  // Body rows
  for (let r = 0; r < bodyH; r++) {
    // Sidebar
    const treeIdx = r + scrollOffset;
    const node = tree[treeIdx];
    const isActive = treeIdx === cursorIndex;
    let left: string;
    if (!node) {
      left = " ".repeat(sideW);
    } else {
      const indent = "  ".repeat(node.depth);
      const raw = indent + node.label;
      left = fitToWidth(raw, sideW, isActive);
    }

    // Viewer
    const vLine = viewerLines[r + viewerScroll] ?? "";
    const right = fitToWidth(vLine, viewW, false);

    buf += `${left + pc.dim(" │ ") + right}\n`;
  }

  // Footer — opencode style: subtle separator + help
  let help: string;
  if (formState?.active) {
    const hasOptions = formState.fields[formState.activeIndex]?.options;
    help = ` ${pc.dim("↑↓")} focus  ${hasOptions ? `${pc.dim("←→")} select  ` : ""}${pc.dim("enter")} submit  ${pc.dim("ctrl+e")} editor  ${pc.dim("esc")} cancel `;
  } else if (!connected) {
    help = ` ${pc.dim("↑↓")} nav  ${pc.dim("enter")} connect  ${pc.dim("q")} quit `;
  } else {
    help = ` ${pc.dim("↑↓")} nav  ${pc.dim("←→")} toggle  ${pc.dim("c")} create  ${pc.dim("e")} edit  ${pc.dim("d")} delete  ${pc.dim("/")} search  ${pc.dim("n")} neighbors  ${pc.dim("i")} info  ${pc.dim("r")} refresh  ${pc.dim("s")} switch  ${pc.dim("l")} logout  ${pc.dim("q")} quit `;
  }
  buf += `${pc.dim("─".repeat(W))}\n`;
  buf += padToWidth(help, W);

  // Clear to end
  buf += "\u001B[J";

  process.stdout.write(buf);
}

/** Pad/truncate `text` to exactly `width` visible characters. */
function fitToWidth(text: string, width: number, highlight: boolean): string {
  const vw = visibleWidth(text);
  let result: string;
  if (vw > width) {
    // Truncate (crude but safe: just truncate the clean text approach)
    result = truncateToWidth(text, width - 1) + pc.dim("…");
  } else {
    result = text + " ".repeat(Math.max(0, width - vw));
  }
  return highlight ? pc.inverse(result) : result;
}

/** Truncate a string (potentially with ANSI codes) to a target visible width. */
function truncateToWidth(text: string, targetW: number): string {
  let w = 0;
  let i = 0;
  const chars = [...text];
  let result = "";
  while (i < chars.length && w < targetW) {
    const ch = chars[i];
    if (ch === undefined) {
      break;
    }
    if (ch === "\u001B") {
      // Consume ANSI sequence
      let seq = ch;
      i++;
      while (i < chars.length) {
        const next = chars[i];
        if (next === undefined || /[a-zA-Z]/.test(next)) {
          break;
        }
        seq += next;
        i++;
      }
      if (i < chars.length && chars[i] !== undefined) {
        seq += chars[i];
        i++;
      }
      result += seq;
      continue;
    }
    const cp = ch.codePointAt(0);
    if (cp === undefined) {
      break;
    }
    const cw = cp > 0xffff ? 2 : 1;
    if (w + cw > targetW) {
      break;
    }
    result += ch;
    w += cw;
    i++;
  }
  return result;
}

/** Simple pad with visible width awareness. */
function padToWidth(text: string, width: number): string {
  const vw = visibleWidth(text);
  if (vw >= width) {
    return text;
  }
  return text + " ".repeat(width - vw);
}

// ── Utils ────────────────────────────────────────────────────────────

async function launchEditor(f: FormField) {
  if (process.stdin.isTTY) {
    process.stdin.setRawMode(false);
  }
  if (keypressHandler) {
    process.stdin.removeListener("keypress", keypressHandler);
  }
  console.clear();

  const tmpFile = path.join(os.tmpdir(), `thingd-edit-${Date.now()}.json`);
  let initialContent = "";
  if (f.value && f.value !== "") {
    try {
      initialContent = JSON.stringify(JSON.parse(f.value), null, 2);
    } catch {
      initialContent = f.value;
    }
  } else {
    initialContent = "{\n  \n}\n";
  }
  fs.writeFileSync(tmpFile, initialContent);

  const editor = process.env.EDITOR || "vim";

  await new Promise<void>((resolve) => {
    const child = spawn(editor, [tmpFile], { stdio: "inherit" });
    child.on("exit", () => resolve());
    child.on("error", (err) => {
      console.error("Failed to start editor:", err);
      setTimeout(() => resolve(), 2000);
    });
  });

  try {
    const newContent = fs.readFileSync(tmpFile, "utf-8");
    f.value = newContent.trim();
  } catch (_e) {}

  if (process.stdin.isTTY) {
    process.stdin.setRawMode(true);
  }
  if (keypressHandler) {
    process.stdin.on("keypress", keypressHandler);
  }
  draw();
}

function parsePayload(str: string): Record<string, unknown> {
  str = str.trim();
  if (!str) {
    return {};
  }
  if (str.startsWith("{") || str.startsWith("[")) {
    return JSON.parse(str);
  }

  const obj: Record<string, unknown> = {};
  const parts = str.match(/(?:[^\s"]+|"[^"]*")+/g) || [];

  for (const part of parts) {
    const eqIdx = part.indexOf("=");
    if (eqIdx === -1) {
      obj[part] = true;
      continue;
    }
    const k = part.substring(0, eqIdx);
    let v: string | boolean | number = part.substring(eqIdx + 1);

    if (v.startsWith('"') && v.endsWith('"')) {
      v = v.substring(1, v.length - 1);
    } else {
      if (v === "true") {
        v = true;
      } else if (v === "false") {
        v = false;
      } else if (!Number.isNaN(Number(v))) {
        v = Number(v);
      }
    }
    obj[k] = v;
  }
  return obj;
}

// ── Mutation Handlers ────────────────────────────────────────────────

async function handleCreate(selected: TreeNode | undefined) {
  let defaultCol = "";
  let defaultStream = "";
  let defaultQueue = "";

  if (selected) {
    const ref = selected.ref as { name?: string; collection?: string };
    if (selected.type === "collection") {
      defaultCol = ref?.name ?? "";
    } else if (selected.type === "object") {
      defaultCol = ref?.collection ?? "";
    } else if (selected.type === "stream") {
      defaultStream = ref?.name ?? "";
    } else if (selected.type === "queue") {
      defaultQueue = ref?.name ?? "";
    }
  }

  openForm(
    "Create Resource",
    [
      {
        id: "kind",
        label: "Kind (object, event, queue, link)",
        value: defaultStream
          ? "event"
          : defaultQueue
            ? "queue"
            : selected?.type === "link"
              ? "link"
              : "object",
        options: ["object", "event", "queue", "link"],
      },
      {
        id: "target",
        label: "Target (Collection, Stream, Queue, or From Reference)",
        value: defaultCol || defaultStream || defaultQueue,
        options: Array.from(new Set([...collections, ...streams, ...queues])).sort(),
        allowCustom: true,
      },
      {
        id: "objId",
        label: "Object / To Reference ID (auto if blank for objects)",
        placeholder: "Leave blank to auto-generate",
      },
      {
        id: "payload",
        label: "Data, Link Type, or JSON Fields",
        placeholder: 'e.g. name="John" age=30 or {"linkType":"follows","weight":1}',
      },
    ],
    async (vals) => {
      const kind = (vals.kind || "").toLowerCase();
      const target = (vals.target || "").trim();
      if (!target) {
        throw new Error("Target is required.");
      }

      if (kind === "object") {
        let id = vals.objId?.trim();
        if (!id) {
          try {
            id = crypto.randomUUID();
          } catch (_e) {
            id = `obj_${Date.now().toString(36)}${Math.random().toString(36).substring(2)}`;
          }
        }
        const data = parsePayload(vals.payload || "");
        await db.put(target, { id, ...data });
        expandedSet.add("cat:collections");
        expandedSet.add(`col:${target}`);
      } else if (kind === "event") {
        if (!vals.payload?.trim()) {
          throw new Error("Event Type is required (in Data field for events).");
        }
        await db.events.append(target, { type: vals.payload.trim() });
        expandedSet.add("cat:streams");
      } else if (kind === "queue") {
        if (!vals.payload?.trim()) {
          throw new Error("Payload is required.");
        }
        const data = parsePayload(vals.payload);
        await db.queue(target).push(data);
        expandedSet.add("cat:queues");
      } else if (kind === "link") {
        const toRef = (vals.objId || "").trim();
        if (!toRef) {
          throw new Error("To Reference is required (use Object ID field).");
        }
        let linkType = "related";
        let weight: number | undefined;
        let metadataJson: string | undefined;
        try {
          const parsed = JSON.parse(vals.payload || "{}");
          linkType = parsed.linkType || parsed.link_type || "related";
          if (parsed.weight !== undefined) {
            weight = Number(parsed.weight);
          }
          if (parsed.metadata || parsed.metadataJson) {
            metadataJson =
              typeof parsed.metadata === "string"
                ? parsed.metadata
                : JSON.stringify(parsed.metadata);
          }
        } catch {
          linkType = (vals.payload || "").trim() || "related";
        }
        await db.links.create(target, linkType, toRef, weight, metadataJson);
      } else {
        throw new Error("Kind must be 'object', 'event', 'queue', or 'link'.");
      }
    }
  );
}

async function handleEdit(selected: TreeNode | undefined) {
  if (!selected) {
    return;
  }

  if (selected.type === "object" && selected.ref) {
    const ref = selected.ref as { collection: string; id: string };
    const current = await db.get(ref.collection, ref.id);
    const clean = current ? { ...current } : {};
    for (const k of ["id", "collection", "createdAt", "updatedAt", "version"] as const) {
      delete (clean as Record<string, unknown>)[k];
    }

    openForm(
      `Edit Object: ${ref.id}`,
      [{ id: "payload", label: "Data (JSON or key=value)", value: JSON.stringify(clean) }],
      async (vals) => {
        const data = parsePayload(vals.payload || "");
        await db.put(ref.collection, { id: ref.id, ...data });
      }
    );
  } else if (selected.type === "queue" && selected.ref) {
    const ref = selected.ref as { name: string };
    const queue = db.queue(ref.name);

    openForm(
      `Manage Queue: ${ref.name}`,
      [
        { id: "action", label: "Action (claim, push)", value: "claim", options: ["claim", "push"] },
        {
          id: "payload",
          label: "Job Data (JSON or key=value, only for push)",
          placeholder: 'task="email"',
        },
      ],
      async (vals) => {
        const action = vals.action || "";
        if (action === "claim") {
          const job = await queue.claim();
          if (job) {
            throw new Error(`Claimed job: ${job.id}`);
          } else {
            throw new Error("No ready jobs.");
          }
        } else if (action === "push") {
          const data = parsePayload(vals.payload || "");
          await queue.push(data);
        } else {
          throw new Error("Action must be 'claim' or 'push'.");
        }
      }
    );
  } else if (selected.type === "job" && selected.ref) {
    const ref = selected.ref as { queue: string; jobId: string; status: string };
    if (ref.status === "dead") {
      openForm(
        `Retry Dead Job: ${ref.jobId.slice(0, 12)}`,
        [
          {
            id: "action",
            label: "Action",
            value: "ack",
            options: ["ack", "nack"],
          },
          {
            id: "error",
            label: "Error message (for nack)",
            placeholder: "Optional error",
          },
        ],
        async (vals) => {
          const action = vals.action || "";
          if (action === "ack") {
            await db.queue(ref.queue).ack(ref.jobId);
          } else if (action === "nack") {
            await db.queue(ref.queue).nack(ref.jobId, { error: vals.error || "Rejected" });
          } else {
            throw new Error("Action must be 'ack' or 'nack'.");
          }
        }
      );
    } else {
      // Active job — nack to fail it back to ready
      openForm(
        `Nack Job: ${ref.jobId.slice(0, 12)}`,
        [{ id: "error", label: "Error message", placeholder: "Optional" }],
        async (vals) => {
          await db.queue(ref.queue).nack(ref.jobId, { error: vals.error || "Nacked" });
        }
      );
    }
  } else {
    openForm(
      "Edit Not Supported",
      [{ id: "msg", label: "Error", value: "Editing is only available for Objects and Queues." }],
      async () => {}
    );
  }
}

async function handleDelete(selected: TreeNode | undefined) {
  if (!selected) {
    return;
  }

  if (selected.type === "object" && selected.ref) {
    const ref = selected.ref as { collection: string; id: string };
    openForm(
      `Delete Object: ${ref.id}`,
      [{ id: "confirm", label: 'Type "yes" to confirm deletion', placeholder: "yes" }],
      async (vals) => {
        if ((vals.confirm || "").toLowerCase() !== "yes") {
          throw new Error("Canceled");
        }
        const result = await db.delete(ref.collection, ref.id);
        if (result && !result.deleted) {
          throw new Error(`Object '${ref.id}' not found in collection '${ref.collection}'`);
        }
      }
    );
  } else if (selected.type === "queue" && selected.ref) {
    const ref = selected.ref as { name: string };
    openForm(
      `Resolve Queue Job`,
      [
        { id: "jobId", label: "Leased Job ID", placeholder: "job-id" },
        { id: "action", label: "Action (ack, nack)", value: "ack" },
      ],
      async (vals) => {
        const jobId = (vals.jobId || "").trim();
        const action = vals.action || "";
        if (!jobId) {
          throw new Error("Job ID required.");
        }
        if (action === "ack") {
          await db.queue(ref.name).ack(jobId);
        } else if (action === "nack") {
          await db.queue(ref.name).nack(jobId, { error: "Rejected via CLI" });
        } else {
          throw new Error("Action must be 'ack' or 'nack'.");
        }
      }
    );
  } else if (selected.type === "link" || loadedItemId === "neighbors_result") {
    openForm(
      "Delete Link",
      [
        {
          id: "linkId",
          label: "Link ID",
          placeholder: "Paste the link ID from the neighbors view",
        },
        { id: "confirm", label: 'Type "yes" to confirm deletion', placeholder: "yes" },
      ],
      async (vals) => {
        const linkId = (vals.linkId || "").trim();
        if (!linkId) {
          throw new Error("Link ID is required.");
        }
        if ((vals.confirm || "").toLowerCase() !== "yes") {
          throw new Error("Canceled");
        }
        const ok = await db.links.delete(linkId);
        if (!ok) {
          throw new Error(`Link '${linkId}' not found.`);
        }
      }
    );
  } else if (selected.type === "job" && selected.ref) {
    const ref = selected.ref as { queue: string; jobId: string; status: string };
    if (ref.status === "dead") {
      openForm(
        `Remove Dead Job: ${ref.jobId.slice(0, 12)}`,
        [
          {
            id: "action",
            label: "Action",
            value: "nack",
            options: ["ack", "nack"],
          },
          { id: "confirm", label: 'Type "yes" to confirm', placeholder: "yes" },
        ],
        async (vals) => {
          if ((vals.confirm || "").toLowerCase() !== "yes") {
            throw new Error("Canceled");
          }
          if (vals.action === "ack") {
            await db.queue(ref.queue).ack(ref.jobId);
          } else {
            await db.queue(ref.queue).nack(ref.jobId, { error: "Removed from dead letter" });
          }
        }
      );
    } else {
      openForm(
        "Delete Not For Active Jobs",
        [{ id: "msg", label: "Info", value: "Use [e] to nack an active job back to ready state." }],
        async () => {}
      );
    }
  } else {
    openForm(
      "Delete Not Supported",
      [
        {
          id: "msg",
          label: "Error",
          value: "Deletion is only available for Objects, Links, and Queues.",
        },
      ],
      async () => {}
    );
  }
}

async function handleSearch() {
  openForm(
    "Global Search",
    [
      { id: "query", label: "Search Query", placeholder: "text to search" },
      { id: "limit", label: "Limit (optional)", placeholder: "100" },
    ],
    async (vals) => {
      const query = (vals.query || "").trim();
      if (!query) {
        throw new Error("Search query required.");
      }
      const limitStr = vals.limit || "";
      const options: MemorySearchOptions = {};
      if (limitStr) {
        const limit = parseInt(limitStr, 10);
        if (!Number.isNaN(limit)) {
          options.limit = limit;
        }
      }
      const results = await db.search(query, options);

      // Display results in the viewer
      viewerLines = [
        ` ${pc.bold("Search Results:")} ${pc.cyan(query)}`,
        "",
        ...(results.length === 0 ? [" No results found."] : []),
        ...results.map((r) => {
          const res = r as {
            id: string;
            kind: string;
            collection?: string;
            stream?: string;
            value?: { text?: string };
          };
          const id = pc.green(res.id);
          const col = pc.cyan(res.kind === "object" ? (res.collection ?? "") : (res.stream ?? ""));
          const textStr = res.value?.text ? pc.dim(res.value.text.substring(0, 100)) : "";
          return ` ${col} / ${id} ${textStr}`;
        }),
      ];
      loadedItemId = "search_results";
    },
    true
  );
}

async function handleInfo() {
  const lines: string[] = [
    ` ${pc.bold("Connection Status")}`,
    "",
    ` ${pc.dim("Driver")}  ${pc.cyan(driver)}`,
    ` ${pc.dim("Path")}    ${pc.cyan(dbPath)}`,
  ];

  if (driver === "cloud") {
    try {
      const baseUrl = dbPath.startsWith("thingd://")
        ? `http://${dbPath.slice("thingd://".length)}`
        : dbPath;
      const apiRoot = baseUrl.replace(/\/+$/, "");

      const fetchJson = async (p: string) => {
        const u = `${apiRoot}${p}`;
        const headers: Record<string, string> = {};
        if (authToken) {
          headers.Authorization = `Bearer ${authToken}`;
        }
        const res = await fetch(u, { headers });
        const json = await res.json();
        if (!res.ok) {
          const detail = json?.error?.detail ?? json?.error?.message ?? `HTTP ${res.status}`;
          throw new Error(`${res.status}: ${detail}`);
        }
        return json;
      };

      // Health check
      let healthError: string | null = null;
      let health: unknown = null;
      try {
        health = await fetchJson("/v1/health");
      } catch (err) {
        healthError = err instanceof Error ? err.message : String(err);
      }

      // Collections check
      let collectionsResult: string[] | null = null;
      if (!healthError) {
        try {
          const colResp = await fetchJson("/v1/collections");
          collectionsResult = (colResp as { data?: string[] })?.data ?? null;
        } catch {
          // collections endpoint may not be available on all setups
        }
      }

      lines.push("");
      lines.push(` ${pc.bold("Cloud REST API")}`);
      if (healthError) {
        lines.push(` ${pc.red("Health check failed:")} ${healthError}`);
        lines.push(` ${pc.dim("API URL:")} ${apiRoot}/v1/...`);
        lines.push(
          ` ${pc.dim("Auth:")} ${authToken ? pc.green("Bearer token set") : pc.red("No token")}`
        );
      } else {
        const h = health as Record<string, unknown> | null;
        const status =
          (h?.data as Record<string, unknown> | undefined)?.status ?? h?.status ?? "ok";
        lines.push(` ${pc.dim("Status:")} ${pc.green(String(status))}`);
        lines.push(
          ` ${pc.dim("Collections:")} ${collectionsResult ? pc.cyan(String(collectionsResult.length)) : pc.yellow("unknown")}`
        );
        if (collectionsResult && collectionsResult.length > 0) {
          lines.push(` ${pc.dim("Names:")} ${collectionsResult.join(", ")}`);
        }
      }

      if (cloudError) {
        lines.push("");
        lines.push(` ${pc.yellow("⚠ Recent error:")} ${cloudError}`);
      }
    } catch (err) {
      const errMsg = err instanceof Error ? err.message : String(err);
      lines.push("", ` ${pc.red("Cloud Query Failed:")} ${errMsg}`);
    }
  }

  viewerLines = lines;
  loadedItemId = "info_status";
}

async function handleNeighbors(selected: TreeNode | undefined) {
  if (selected?.type !== "object" || !selected.ref) {
    viewerLines = [
      ` ${pc.yellow("Neighbors")}`,
      "",
      ` ${pc.dim("Select an object first, then press")} ${pc.bold("n")} ${pc.dim("to browse its links.")}`,
    ];
    loadedItemId = "neighbors_info";
    draw();
    return;
  }

  const ref = selected.ref as { collection: string; id: string };
  const fromRef = `${ref.collection}/${ref.id}`;

  openForm(
    `Neighbors of ${fromRef}`,
    [
      {
        id: "direction",
        label: "Direction",
        value: "Both",
        options: ["Both", "Outgoing", "Incoming"],
      },
      {
        id: "linkType",
        label: "Link Type (optional, leave blank for all)",
        placeholder: "e.g. follows, owns",
      },
      {
        id: "limit",
        label: "Max Results (optional)",
        placeholder: "50",
      },
    ],
    async (vals) => {
      const direction = (vals.direction || "Both") as "Outgoing" | "Incoming" | "Both";
      const linkType = (vals.linkType || "").trim() || undefined;
      const limitStr = vals.limit || "";
      const limit = limitStr ? parseInt(limitStr, 10) || undefined : undefined;

      const links = await db.links.neighbors(fromRef, direction, { linkType, limit });

      lastNeighborsRef = fromRef;
      viewerLines = [
        ` ${pc.bold("Neighbors of")} ${pc.cyan(fromRef)}`,
        ` ${pc.dim(
          `(${links.length} link${links.length !== 1 ? "s" : ""}${
            direction !== "Both" ? `, ${direction.toLowerCase()}` : ""
          }${linkType ? `, type: ${linkType}` : ""})`
        )}`,
        "",
        ...(links.length === 0 ? [` ${pc.dim("No links found.")}`] : []),
        ...links.flatMap((link) => [
          ` ${pc.blue("◈")} ${pc.dim(link.id)}`,
          `   ${link.fromRef} ${pc.cyan(link.linkType)} ${link.toRef}`,
          link.weight !== undefined ? `   ${pc.dim(`weight: ${link.weight}`)}` : "",
          link.metadataJson && link.metadataJson !== "{}"
            ? `   ${pc.dim(`metadata: ${link.metadataJson}`)}`
            : "",
          "",
        ]),
        pc.dim("[d] Delete a link by ID  [c] Create a new link"),
      ];
      loadedItemId = "neighbors_result";
      draw();
    }
  );
}

async function handleMaintenance() {
  // Cycle through maintenance operations with each press of 'm'
  const operations = ["health", "checkpoint", "backup"];
  const idx = maintenanceCursor % operations.length;
  maintenanceCursor = (maintenanceCursor + 1) % operations.length;
  const op = operations[idx];

  viewerLines = [pc.dim(`Running ${op}...`)];
  draw();

  if (op === "backup") {
    const backupPath = `thingd-backup-${Date.now()}.db`;
    try {
      if (db?.backupTo) {
        db.backupTo(backupPath);
        viewerLines = [` Backup created: ${backupPath}`, ``];
      } else {
        viewerLines = [` Backup not available (cloud driver)`, ``];
      }
    } catch (err) {
      viewerLines = [` Backup failed: ${err instanceof Error ? err.message : String(err)}`, ``];
    }
  } else if (op === "checkpoint") {
    try {
      if (db?.walCheckpoint) {
        const result = db.walCheckpoint();
        viewerLines = [
          ` WAL Checkpoint complete`,
          ` Frames before: ${result.framesBefore ?? "N/A"}`,
          ` Frames after: ${result.framesAfter ?? "N/A"}`,
          ``,
        ];
      } else {
        viewerLines = [` WAL Checkpoint not available (cloud driver)`, ``];
      }
    } catch (err) {
      viewerLines = [
        ` WAL Checkpoint failed: ${err instanceof Error ? err.message : String(err)}`,
        ``,
      ];
    }
  } else {
    // Health check — verify read path
    try {
      if (typeof db !== "undefined" && typeof db.countObjects === "function") {
        const objCount = await db.countObjects();
        const evtCount = await db.countEvents();
        const jobCount = typeof db.countActiveJobs === "function" ? await db.countActiveJobs() : 0;
        viewerLines = [
          ` Health check passed`,
          ` Objects: ${objCount}, Events: ${evtCount}, Active jobs: ${jobCount}`,
          ``,
        ];
      } else {
        viewerLines = [` Health check not available`, ``];
      }
    } catch (err) {
      viewerLines = [
        ` Health check failed: ${err instanceof Error ? err.message : String(err)}`,
        ``,
      ];
    }
  }

  loadedItemId = "maintenance_result";
  draw();
}

// ── Keyboard Listener ────────────────────────────────────────────────

function setupKeypress() {
  process.stdin.removeAllListeners("keypress");
  readline.emitKeypressEvents(process.stdin);
  if (process.stdin.isTTY) {
    process.stdin.setRawMode(true);
  }
  process.stdin.resume();

  keypressHandler = async (str, key) => {
    if (!key) {
      return;
    }

    // Quit
    if ((key.ctrl && key.name === "c") || key.name === "q") {
      if (formState?.active && key.name !== "q") {
        formState.onCancel();
        return;
      } else if (!formState?.active) {
        cleanup();
        return;
      }
    }

    if (formState?.active && !formState.isSubmitting) {
      if (key.ctrl && key.name === "e") {
        const f = formState.fields[formState.activeIndex];
        if (f) {
          await launchEditor(f);
        }
        return;
      } else if (key.name === "escape") {
        formState.onCancel();
      } else if (key.name === "up" || (key.name === "tab" && key.shift)) {
        if (formState.activeIndex > 0) {
          formState.activeIndex--;
        }
        formState.error = undefined;
        draw();
      } else if (key.name === "down" || key.name === "tab") {
        if (formState.activeIndex < formState.fields.length - 1) {
          formState.activeIndex++;
        }
        formState.error = undefined;
        draw();
      } else if (key.name === "return") {
        if (formState.activeIndex < formState.fields.length - 1) {
          formState.activeIndex++;
          draw();
        } else {
          const vals: Record<string, string> = {};
          for (const f of formState.fields) {
            vals[f.id] = f.value;
          }
          formState.onSubmit(vals);
        }
      } else if (key.name === "left" || key.name === "right") {
        const f = formState.fields[formState.activeIndex];
        if (f?.options && f.options.length > 0) {
          const currentIndex = f.options.indexOf(f.value);
          let nextIndex = key.name === "right" ? currentIndex + 1 : currentIndex - 1;
          if (nextIndex < 0) {
            nextIndex = f.options.length - 1;
          }
          if (nextIndex >= f.options.length) {
            nextIndex = 0;
          }
          f.value = f.options[nextIndex] ?? "";
          formState.error = undefined;
          draw();
        }
      } else if (key.name === "backspace") {
        const f = formState.fields[formState.activeIndex];
        if (f && (!f.options || f.allowCustom) && f.value.length > 0) {
          f.value = f.value.slice(0, -1);
          formState.error = undefined;
          draw();
        }
      } else if (str) {
        const f = formState.fields[formState.activeIndex];
        if (f && (!f.options || f.allowCustom)) {
          // biome-ignore lint/suspicious/noControlCharactersInRegex: we need to filter control characters
          const clean = str.replace(/[\x00-\x1F\x7F]/g, "");
          if (clean) {
            if (f.isSecret && !f.dirty && f.value) {
              f.value = clean;
              f.dirty = true;
            } else {
              f.value += clean;
            }
            formState.error = undefined;
            draw();
          }
        }
      }
      return;
    }

    const tree = buildTree();

    // Navigation (works in both connected and disconnected states)
    if (key.name === "up" || str === "k") {
      if (cursorIndex > 0) {
        cursorIndex--;
        draw();
        const n = tree[cursorIndex];
        if (n) {
          scheduleLoad(n);
        }
      }
    } else if (key.name === "down" || str === "j") {
      if (cursorIndex < tree.length - 1) {
        cursorIndex++;
        draw();
        const n = tree[cursorIndex];
        if (n) {
          scheduleLoad(n);
        }
      }
    } else if (!connected) {
      // Driver selection mode — only Enter works
      if (key.name === "return") {
        const node = tree[cursorIndex];
        if (node) {
          await handleConnect(node);
        }
      }
    } else {
      // Connected mode — full set of shortcuts
      if (key.name === "right" || str === "l") {
        const node = tree[cursorIndex];
        if (node?.expandable) {
          if (!expandedSet.has(node.id)) {
            expandedSet.add(node.id);
            if (node.type === "collection") {
              await fetchResources();
            }
            draw();
          } else {
            const newTree = buildTree();
            if (cursorIndex + 1 < newTree.length) {
              cursorIndex++;
              draw();
              const n = newTree[cursorIndex];
              if (n) {
                scheduleLoad(n);
              }
            }
          }
        }
      } else if (key.name === "left" || str === "h") {
        const node = tree[cursorIndex];
        if (node) {
          if (node.expandable && expandedSet.has(node.id)) {
            expandedSet.delete(node.id);
            draw();
          } else if (node.parentId) {
            const parentIdx = tree.findIndex((n) => n.id === node.parentId);
            if (parentIdx !== -1) {
              cursorIndex = parentIdx;
              draw();
              const n = tree[cursorIndex];
              if (n) {
                scheduleLoad(n);
              }
            }
          }
        }
      } else if (key.name === "return") {
        const node = tree[cursorIndex];
        if (node?.expandable) {
          if (expandedSet.has(node.id)) {
            expandedSet.delete(node.id);
          } else {
            expandedSet.add(node.id);
            if (node.type === "collection") {
              await fetchResources();
            }
          }
          draw();
        }
      } else if (str === "r" || str === "R") {
        loadedItemId = "";
        await fetchResources();
        draw();
        const n = tree[cursorIndex];
        if (n) {
          scheduleLoad(n);
        }
      } else if (str === "s" || str === "S") {
        await handleSwitch();
      } else if (str === "c" || str === "C") {
        await handleCreate(tree[cursorIndex]);
      } else if (str === "e" || str === "E") {
        await handleEdit(tree[cursorIndex]);
      } else if (str === "d" || str === "D") {
        await handleDelete(tree[cursorIndex]);
      } else if (str === "/" || str === "f" || str === "F") {
        await handleSearch();
      } else if (str === "i" || str === "I") {
        await handleInfo();
      } else if (str === "n" || str === "N") {
        await handleNeighbors(tree[cursorIndex]);
      } else if (str === "m" || str === "M") {
        await handleMaintenance();
      } else if (str === "l" || str === "L") {
        await handleLogout();
      }
    }
  };

  process.stdin.on("keypress", keypressHandler);

  if (process.stdout.isTTY) {
    process.stdout.on("resize", () => {
      draw();
    });
  }
}

function cleanup() {
  if (pollTimer) {
    clearInterval(pollTimer);
  }
  if (process.stdin.isTTY) {
    process.stdin.setRawMode(false);
  }
  process.stdout.write("\u001B[?1049l\u001B[?25h");
  console.clear();
  const finish = () => {
    process.exit(0);
  };
  if (connected && db) {
    db.close().then(finish);
  } else {
    finish();
  }
}

async function handleConnect(node: TreeNode) {
  if (node.type !== "driver" || !node.ref) {
    return;
  }

  const selectedDriver = node.ref.driver as ThingDDriver;

  // Cloud with saved credentials — fetch projects/instances and let user pick
  if (selectedDriver === "cloud") {
    const cloudCfg = readCloudConfig();
    if (cloudCfg?.token) {
      try {
        const { projects } = await listProjects(cloudCfg);
        const instanceOptions: {
          label: string;
          mcpUrl: string;
          projectSlug: string;
          instanceSlug: string;
        }[] = [];

        for (const project of projects) {
          try {
            const { instances } = await listInstances(cloudCfg, project.id);
            for (const inst of instances) {
              if (inst.mcpUrl) {
                instanceOptions.push({
                  label: `${project.slug}/${inst.slug}`,
                  mcpUrl: inst.mcpUrl,
                  projectSlug: project.slug,
                  instanceSlug: inst.slug,
                });
              }
            }
          } catch {
            // Skip projects that fail to list instances
          }
        }

        if (instanceOptions.length > 0) {
          // Pre-select the saved instance if it exists
          const savedIdx = cloudCfg.instanceUrl
            ? instanceOptions.findIndex((o) => o.mcpUrl === cloudCfg.instanceUrl)
            : -1;
          const defaultVal =
            savedIdx >= 0 ? instanceOptions[savedIdx]?.label : instanceOptions[0]?.label;

          openForm(
            "Connect to Cloud",
            [
              {
                id: "instance",
                label: "Instance",
                value: defaultVal ?? "",
                options: instanceOptions.map((o) => o.label),
              },
            ],
            async (vals) => {
              const selected = instanceOptions.find((o) => o.label === vals.instance);
              if (!selected) {
                viewerLines = [pc.red("No instance selected.")];
                draw();
                return;
              }
              // Save selection to cloud config
              cloudCfg.instanceUrl = selected.mcpUrl;
              cloudCfg.projectSlug = selected.projectSlug;
              cloudCfg.instanceSlug = selected.instanceSlug;
              writeCloudConfig(cloudCfg);

              await connectToDriver(
                "cloud",
                deriveRestUrl(selected.mcpUrl),
                deriveRestUrl(selected.mcpUrl),
                cloudCfg.apiKey ?? cloudCfg.token,
                selected.instanceSlug
              );
            }
          );
          return;
        }

        // No instances found — try saved URL or show error
        if (cloudCfg.instanceUrl) {
          const restUrl = deriveRestUrl(cloudCfg.instanceUrl);
          await connectToDriver(
            "cloud",
            restUrl,
            restUrl,
            cloudCfg.apiKey ?? cloudCfg.token,
            cloudCfg.instanceSlug
          );
          return;
        }
        viewerLines = [
          pc.yellow("No cloud instances found."),
          pc.dim(
            "Create one at https://thingd.cloud, or press l to logout and re-enter credentials."
          ),
        ];
        draw();
        return;
      } catch {
        if (cloudCfg.instanceUrl) {
          const restUrl = deriveRestUrl(cloudCfg.instanceUrl);
          await connectToDriver(
            "cloud",
            restUrl,
            restUrl,
            cloudCfg.apiKey ?? cloudCfg.token,
            cloudCfg.instanceSlug
          );
          return;
        }
        viewerLines = [
          pc.yellow("Could not fetch cloud instances."),
          pc.dim("Check your network or press l to logout and re-enter credentials."),
        ];
        draw();
        return;
      }
    }
  }

  if (selectedDriver === "native" || selectedDriver === "cloud") {
    const cloudCfg = selectedDriver === "cloud" ? readCloudConfig() : null;
    const baseUrl = cloudCfg?.url ?? "https://api.thingd.cloud";
    const isCloudWithConfig = selectedDriver === "cloud" && cloudCfg?.token;

    openForm(
      selectedDriver === "cloud" && !isCloudWithConfig
        ? `Connect to ${selectedDriver} — enter credentials below or run ${pc.cyan("thingd cloud login")} first`
        : `Connect to ${selectedDriver}`,
      [
        ...(selectedDriver === "cloud"
          ? isCloudWithConfig
            ? [
                {
                  id: "project",
                  label: "Cloud Project (slug)",
                  value: "",
                },
                {
                  id: "instance",
                  label: "Cloud Instance (slug)",
                  value: "",
                },
                {
                  id: "token",
                  label: "Bearer Token (optional)",
                  isSecret: true,
                  value: cloudCfg?.token ?? "",
                },
              ]
            : [
                {
                  id: "url",
                  label: "Cloud URL (from thingd.cloud dashboard)",
                  value: "",
                },
                {
                  id: "token",
                  label: "Bearer Token (optional)",
                  isSecret: true,
                  value: cloudCfg?.token ?? "",
                },
              ]
          : [
              {
                id: "path",
                label: "Database Path",
                value: defaultThingdDbPath(),
              },
            ]),
      ],
      async (vals) => {
        let cloudUrl: string;
        let instanceSlugVal: string | undefined;
        if (selectedDriver === "cloud") {
          if (isCloudWithConfig) {
            if (!vals.project || !vals.instance) {
              viewerLines = [pc.red("Project and instance slugs are required.")];
              draw();
              return;
            }
            instanceSlugVal = vals.instance;
            // Construct URL from project + instance slugs
            cloudUrl = `${baseUrl}/mcp/${encodeURIComponent(vals.project)}/${encodeURIComponent(vals.instance)}`;
            // Save selection to cloud config
            const cfg = cloudCfg ?? { token: vals.token, url: baseUrl };
            cfg.instanceUrl = cloudUrl;
            cfg.projectSlug = vals.project;
            cfg.instanceSlug = instanceSlugVal;
            if (vals.token) {
              cfg.token = vals.token;
            }
            writeCloudConfig(cfg);
            // Connect via REST (derive base URL from MCP URL)
            cloudUrl = deriveRestUrl(cloudUrl);
          } else {
            if (!vals.url) {
              viewerLines = [pc.red("Cloud URL is required.")];
              draw();
              return;
            }
            cloudUrl = vals.url;
            // Save manual credentials to cloud config
            writeCloudConfig({ token: vals.token || "", url: cloudUrl });
          }
          await connectToDriver(
            selectedDriver,
            cloudUrl,
            cloudUrl,
            cloudCfg?.apiKey ?? vals.token,
            instanceSlugVal
          );
        } else {
          await connectToDriver(selectedDriver, vals.path || "", undefined, undefined);
        }
      }
    );
  } else {
    // Memory — connect directly without suspending
    driver = selectedDriver;
    dbPath = ":memory:";
    viewerLines = [pc.dim("Connecting...")];
    draw();

    try {
      db = await ThingD.open({
        path: ":memory:",
        driver: "memory",
      });
      connected = true;
      startedAt = Date.now();
      cursorIndex = 0;
      scrollOffset = 0;
      loadedItemId = "";
      await fetchResources();
      draw();
      const tree = buildTree();
      const first = tree[cursorIndex];
      if (first) {
        scheduleLoad(first);
      }
    } catch (error) {
      const errMsg = error instanceof Error ? error.message : String(error);
      viewerLines = [pc.red(`Failed to connect: ${errMsg}`)];
      draw();
    }
  }
}

async function handleSwitch() {
  if (!connected) {
    return;
  }

  // Close current connection
  try {
    await db.close();
  } catch {
    // ignore close errors
  }

  // Reset state
  connected = false;
  driver = "";
  dbPath = "";
  collections = [];
  streams = [];
  queues = [];
  objectsByCollection = new Map();
  cursorIndex = 0;
  scrollOffset = 0;
  loadedItemId = "";
  viewerLines = ["Select an environment to connect."];

  draw();
  const tree = buildTree();
  const first = tree[cursorIndex];
  if (first) {
    scheduleLoad(first);
  }
}

async function handleLogout() {
  // Close current connection if any
  if (connected && db) {
    try {
      await db.close();
    } catch {
      // ignore close errors
    }
  }

  // Remove cloud credentials from disk
  removeCloudConfig();

  // Reset state
  connected = false;
  driver = "";
  dbPath = "";
  authToken = "";
  cloudError = null;
  collections = [];
  streams = [];
  queues = [];
  objectsByCollection = new Map();
  cursorIndex = 0;
  scrollOffset = 0;
  loadedItemId = "";
  viewerLines = [pc.green("Logged out."), pc.dim("Select an environment to connect.")];

  draw();
  const tree = buildTree();
  const first = tree[cursorIndex];
  if (first) {
    scheduleLoad(first);
  }
}

// ── Entry Point ──────────────────────────────────────────────────────

export async function runInteractiveCli(): Promise<void> {
  // Go straight into the TUI — no pre-prompts
  console.clear();
  process.stdout.write("\u001B[?1049h\u001B[H\u001B[?25l");

  // Show the driver selection screen
  viewerLines = [
    ` ${logoText()} ${pc.dim("— local data engine")}`,
    "",
    pc.dim("  Select an environment to connect."),
  ];
  draw();
  const tree = buildTree();
  const first = tree[cursorIndex];
  if (first) {
    scheduleLoad(first);
  }

  // Auto-connect to cloud if credentials exist
  const cloudCfg = readCloudConfig();
  if (cloudCfg?.token) {
    const cloudUrl = resolveCloudUrl(cloudCfg);
    if (cloudUrl) {
      try {
        const restUrl = deriveRestUrl(cloudUrl);
        await connectToDriver(
          "cloud",
          restUrl,
          restUrl,
          cloudCfg.apiKey ?? cloudCfg.token,
          cloudCfg.instanceSlug
        );
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        viewerLines = [
          pc.yellow(`Auto-connect failed: ${msg}`),
          pc.dim("Select an environment to connect, or press 'r' to retry."),
        ];
        draw();
      }
    }
  } else {
    viewerLines = [
      pc.cyan("Not logged in to thingd Cloud."),
      pc.dim("Press 'c' to connect to a local database, or run thingd cloud login first."),
    ];
    draw();
  }

  setupKeypress();

  // Background polling loop for real-time updates
  pollTimer = setInterval(async () => {
    if (!connected || formState?.active || polling) {
      return;
    }
    polling = true;
    try {
      const snapItemId = loadedItemId;
      const snapshot = JSON.stringify([
        totalObjects,
        totalEventsCount,
        totalActiveJobsCount,
        totalDeadJobsCount,
        totalLinksCount,
      ]);
      await fetchResources();
      const changed =
        snapshot !==
        JSON.stringify([
          totalObjects,
          totalEventsCount,
          totalActiveJobsCount,
          totalDeadJobsCount,
          totalLinksCount,
        ]);
      const tree = buildTree();
      const n = tree[cursorIndex];
      if (n && snapItemId === n.id && n.type !== "category") {
        await loadContent(n).catch(() => {});
      }
      if (changed) {
        draw();
      }
    } catch {
      // Prevent unhandled rejection from killing the process
    } finally {
      polling = false;
    }
  }, 10_000);
}
