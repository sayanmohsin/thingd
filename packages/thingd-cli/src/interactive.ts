import readline from "node:readline";
import pc from "picocolors";
import { ThingD } from "thingd";
import { spawn } from "node:child_process";
import * as os from "node:os";
import * as fs from "node:fs";
import * as path from "node:path";
import * as crypto from "node:crypto";


// ── Helpers ──────────────────────────────────────────────────────────

function highlightJson(val: any): string {
  const str = JSON.stringify(val, null, 2);
  return str.replace(
    /("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+-]?\d+)?)/g,
    (match) => {
      if (/^"/.test(match)) {
        return /:$/.test(match) ? pc.cyan(match) : pc.green(match);
      }
      if (/true|false/.test(match)) return pc.magenta(match);
      if (/null/.test(match)) return pc.dim(match);
      return pc.yellow(match);
    }
  );
}

/** Strip ANSI escape codes to get the visible character count. */
function stripAnsi(s: string): string {
  return s.replace(/\u001B\[[0-9;]*[a-zA-Z]/g, "");
}

/** Measure the visible width of a string accounting for wide characters (CJK, emoji). */
function visibleWidth(s: string): number {
  const clean = stripAnsi(s);
  let w = 0;
  for (const ch of clean) {
    const cp = ch.codePointAt(0)!;
    // Emoji (surrogate pairs / high codepoints) and CJK fullwidth ranges
    if (cp > 0xffff || (cp >= 0x1100 && cp <= 0x115f) || (cp >= 0x2e80 && cp <= 0xa4cf) ||
        (cp >= 0xac00 && cp <= 0xd7a3) || (cp >= 0xf900 && cp <= 0xfaff) ||
        (cp >= 0xfe10 && cp <= 0xfe6f) || (cp >= 0xff01 && cp <= 0xff60) ||
        (cp >= 0xffe0 && cp <= 0xffe6) || (cp >= 0x20000 && cp <= 0x2fffd) ||
        (cp >= 0x30000 && cp <= 0x3fffd) || (cp >= 0xfe00 && cp <= 0xfe0f) ||
        (cp >= 0x200d && cp <= 0x200d) || (cp >= 0xe0100 && cp <= 0xe01ef)) {
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
let scrollOffset = 0;
let startedAt = 0; // ms since epoch when we connected
let totalObjects = 0;
let totalEventsCount = 0;
let totalActiveJobsCount = 0;
let totalDeadJobsCount = 0;
let objectsHistory: number[] = [];
let eventsHistory: number[] = [];
let activeJobsHistory: number[] = [];
let deadJobsHistory: number[] = [];
let dbSizeHistory: number[] = [];
let objectWriteRateHistory: number[] = [];
let eventAppendRateHistory: number[] = [];
let colHistory = new Map<string, number[]>();
let streamHistory = new Map<string, number[]>();
let queueActiveHistory = new Map<string, number[]>();
let queueDeadHistory = new Map<string, number[]>();

let viewerLines: string[] = ["Select an item to view details."];
let viewerScroll = 0;
let loadedItemId = "";
let loadTimer: ReturnType<typeof setTimeout> | null = null;
let pollTimer: ReturnType<typeof setInterval> | null = null;
let keypressHandler: ((str: string, key: any) => void) | null = null;

// ── Form State ───────────────────────────────────────────────────────

interface FormField {
  id: string;
  label: string;
  value: string;
  placeholder?: string;
  isSecret?: boolean;
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

function openForm(title: string, fields: (Partial<FormField> & {id: string, label: string})[], onSubmit: (vals: Record<string, string>) => Promise<void>) {
  formState = {
    active: true,
    title,
    fields: fields.map(f => ({ id: f.id, label: f.label, value: f.value || (f.options?.[0] ?? ""), placeholder: f.placeholder, isSecret: f.isSecret, options: f.options, allowCustom: f.allowCustom })),
    activeIndex: 0,
    onCancel: () => {
      formState = null;
      viewerLines = [];
      loadedItemId = ""; // Force reload
      draw();
      const n = buildTree()[cursorIndex];
      if (n) scheduleLoad(n);
    },
    onSubmit: async (vals) => {
      if (!formState) return;
      formState.isSubmitting = true;
      formState.error = undefined;
      draw();
      try {
        await onSubmit(vals);
        formState = null;
        viewerLines = [];
        loadedItemId = ""; // Force reload
        await fetchResources();
        draw();
        const n = buildTree()[cursorIndex];
        if (n) scheduleLoad(n);
      } catch (err: any) {
        if (formState) {
          formState.error = err?.message || String(err) || "Unknown error occurred";
          formState.isSubmitting = false;
          draw();
        }
      }
    }
  };
  viewerScroll = 0;
  draw();
}

// ── Data Fetching ────────────────────────────────────────────────────

const SPARK_WIDTH = 30;

function drawSparkline(data: number[], baselineMax = 0, width = SPARK_WIDTH): string {
  const dataChars = ["\u2581", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
  const track = "\u2581"; // Lower 1/8 block as baseline
  
  if (data.length === 0) return track.repeat(width);
  
  const recent = data.slice(-width);
  const padLen = width - recent.length;
  const max = Math.max(baselineMax, ...recent);
  
  // Left pad = no data yet
  let result = track.repeat(padLen);
  
  if (max === 0) {
    result += track.repeat(recent.length);
    return result;
  }

  result += recent.map(v => {
    if (v === 0) return track;
    const ratio = v / max;
    const idx = Math.max(0, Math.min(dataChars.length - 1, Math.floor(ratio * dataChars.length)));
    
    return dataChars[idx]!;
  }).join("");

  return result;
}

function formatUptime(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  const h = Math.floor(m / 60);
  return `${h}h ${m % 60}m`;
}


async function fetchResourcesFallback() {
  try {
    const nativeCollections = await db.listCollections();
    const nativeStreams = await db.listStreams();
    
    // Maintain default collections/streams for UI if none exist
    const defaultCols = new Set<string>(["decisions", "load-test"]);
    const defaultStrs = new Set<string>(["project:thingd", "load-events", "activity-log"]);
    
    for (const c of nativeCollections) defaultCols.add(c);
    for (const s of nativeStreams) defaultStrs.add(s);

    collections = Array.from(defaultCols).sort();
    streams = Array.from(defaultStrs).sort();

    // Fallback totals
    totalObjects = await db.countObjects();
    totalEventsCount = await db.countEvents();
    totalActiveJobsCount = await db.countActiveJobs();
    totalDeadJobsCount = await db.countDeadJobs();
    
  } catch {
    collections = [];
    streams = [];
    totalObjects = 0;
  }

  // Queues
  try {
    const store = (db as any).store;
    if (store?.queues) {
      queues = (Array.from(store.queues.keys()) as string[]).sort();
    } else {
      queues = ["embed", "load-queue", "worker-queue"];
    }
  } catch {
    queues = ["embed", "load-queue", "worker-queue"];
  }
}

async function fetchResources(): Promise<void> {
  if (driver === "native" && dbPath) {
    try {
      // Override the tracked totals with the actual exact DB count!
      const [
        objCount, evtCount, activeCount, deadCount,
        nativeCollections, nativeStreams, nativeQueues
      ] = await Promise.all([
        db.countObjects(),
        db.countEvents(),
        db.countActiveJobs(),
        db.countDeadJobs(),
        db.listCollections(),
        db.listStreams(),
        db.listQueues?.() ?? Promise.resolve([])
      ]);

      totalObjects = isNaN(objCount) || objCount === 0 ? totalObjects : objCount;
      totalEventsCount = isNaN(evtCount) || evtCount === 0 ? totalEventsCount : evtCount;
      totalActiveJobsCount = isNaN(activeCount) || activeCount === 0 ? totalActiveJobsCount : activeCount;
      totalDeadJobsCount = isNaN(deadCount) || deadCount === 0 ? totalDeadJobsCount : deadCount;

      collections = nativeCollections.length > 0 ? nativeCollections : ["decisions", "load-test"];
      streams = nativeStreams.length > 0 ? nativeStreams : ["project:thingd", "load-events", "activity-log"];
      queues = nativeQueues.length > 0 ? nativeQueues : ["embed", "load-queue", "worker-queue"];
    } catch {
      // Fallback if sqlite3 fails
      await fetchResourcesFallback();
    }
  } else {
    await fetchResourcesFallback();
  }



  // Calculate Deltas for Operations Throughput Rates
  const prevObjects = objectsHistory.length > 0 ? objectsHistory[objectsHistory.length - 1]! : totalObjects;
  const prevEvents = eventsHistory.length > 0 ? eventsHistory[eventsHistory.length - 1]! : totalEventsCount;

  // Polling is every 2000ms. Operations per second = delta / 2
  const objectWriteRate = Math.max(0, Math.round((totalObjects - prevObjects) / 2));
  const eventAppendRate = Math.max(0, Math.round((totalEventsCount - prevEvents) / 2));

  // Push Histories with Initial Pre-population to prevent misleading growth wiggles
  if (objectsHistory.length === 0) {
    objectsHistory = new Array(60).fill(totalObjects);
  } else {
    objectsHistory.push(totalObjects);
    if (objectsHistory.length > 60) objectsHistory.shift();
  }

  if (eventsHistory.length === 0) {
    eventsHistory = new Array(60).fill(totalEventsCount);
  } else {
    eventsHistory.push(totalEventsCount);
    if (eventsHistory.length > 60) eventsHistory.shift();
  }

  if (activeJobsHistory.length === 0) {
    activeJobsHistory = new Array(60).fill(totalActiveJobsCount);
  } else {
    activeJobsHistory.push(totalActiveJobsCount);
    if (activeJobsHistory.length > 60) activeJobsHistory.shift();
  }

  if (deadJobsHistory.length === 0) {
    deadJobsHistory = new Array(60).fill(totalDeadJobsCount);
  } else {
    deadJobsHistory.push(totalDeadJobsCount);
    if (deadJobsHistory.length > 60) deadJobsHistory.shift();
  }

  if (objectWriteRateHistory.length === 0) {
    objectWriteRateHistory = new Array(60).fill(objectWriteRate);
  } else {
    objectWriteRateHistory.push(objectWriteRate);
    if (objectWriteRateHistory.length > 60) objectWriteRateHistory.shift();
  }

  if (eventAppendRateHistory.length === 0) {
    eventAppendRateHistory = new Array(60).fill(eventAppendRate);
  } else {
    eventAppendRateHistory.push(eventAppendRate);
    if (eventAppendRateHistory.length > 60) eventAppendRateHistory.shift();
  }

  // Database Size (only if native)
  let sizeKb = 0;
  if (driver === "native" && dbPath) {
    try {
      sizeKb = Math.round(fs.statSync(dbPath).size / 1024);
    } catch {}
  }

  if (dbSizeHistory.length === 0) {
    dbSizeHistory = new Array(60).fill(sizeKb);
  } else {
    dbSizeHistory.push(sizeKb);
    if (dbSizeHistory.length > 60) dbSizeHistory.shift();
  }
}

// ── Tree Model ───────────────────────────────────────────────────────

interface TreeNode {
  id: string;
  parentId?: string;
  type: "category" | "collection" | "object" | "stream" | "queue" | "status" | "driver";
  label: string;
  depth: number;
  expandable: boolean;
  ref?: any;
}

function buildTree(): TreeNode[] {
  if (!connected) {
    return [
      {
        id: "drv:memory",
        type: "driver",
        label: `${pc.dim("●")} ${pc.bold("Memory")}    ${pc.dim("ephemeral")}`,
        depth: 0,
        expandable: false,
        ref: { driver: "memory" },
      },
      {
        id: "drv:native",
        type: "driver",
        label: `${pc.dim("●")} ${pc.bold("Native")}    ${pc.dim("SQLite file")}`,
        depth: 0,
        expandable: false,
        ref: { driver: "native" },
      },
      {
        id: "drv:cloud",
        type: "driver",
        label: `${pc.dim("●")} ${pc.bold("Cloud")}     ${pc.dim("remote")}`,
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
    label: `${colsOpen ? pc.yellow("▾") : pc.dim("▸")} ${pc.bold("Collections")}`,
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
        label: `${colOpen ? pc.yellow("▾") : pc.dim("▸")} ${pc.cyan(col)}`,
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
            label: `${pc.dim("●")} ${objId}`,
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
    label: `${strsOpen ? pc.yellow("▾") : pc.dim("▸")} ${pc.bold("Streams")}`,
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
      nodes.push({
        id: `stream:${stream}`,
        parentId: "cat:streams",
        type: "stream",
        label: `${pc.dim("~")} ${pc.green(stream)}`,
        depth: 1,
        expandable: false,
        ref: { name: stream },
      });
    }
  }

  // Queues
  const qOpen = expandedSet.has("cat:queues");
  nodes.push({
    id: "cat:queues",
    type: "category",
    label: `${qOpen ? pc.yellow("▾") : pc.dim("▸")} ${pc.bold("Queues")}`,
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
      nodes.push({
        id: `queue:${q}`,
        parentId: "cat:queues",
        type: "queue",
        label: `${pc.dim("◆")} ${pc.magenta(q)}`,
        depth: 1,
        expandable: false,
        ref: { name: q },
      });
    }
  }

  // Metrics
  nodes.push({
    id: "node:status",
    type: "status",
    label: `${pc.dim("○")} ${pc.dim("Metrics")}`,
    depth: 0,
    expandable: false,
  });

  return nodes;
}

// ── Content Loading ──────────────────────────────────────────────────

function scheduleLoad(node: TreeNode) {
  if (!connected) {
    // Show driver info in viewer
    if (node.type === "driver" && node.ref) {
      const d = node.ref.driver as string;
      let info = "";
      if (d === "memory") {
        info = `${pc.bold("Memory Driver")}\n\n`;
        info += `  Ephemeral in-memory database.\n`;
        info += `  All data is destroyed on exit.\n\n`;
        info += `  ${pc.dim("Best for: testing, prototyping")}\n\n`;
        info += `  Press ${pc.bold("Enter")} to connect.`;
      } else if (d === "native") {
        info = `${pc.bold("Native Driver")}\n\n`;
        info += `  Persistent SQLite database.\n`;
        info += `  Data is stored on disk.\n\n`;
        info += `  ${pc.dim("Best for: local development, single-node")}\n\n`;
        info += `  Press ${pc.bold("Enter")} to connect.`;
      } else if (d === "cloud") {
        info = `${pc.bold("Cloud Driver")}\n\n`;
        info += `  Connect to a remote thingd instance.\n`;
        info += `  Requires a URL and optional auth token.\n\n`;
        info += `  ${pc.dim("Best for: production, multi-node")}\n\n`;
        info += `  Press ${pc.bold("Enter")} to connect.`;
      }
      viewerLines = info.split("\n");
      loadedItemId = node.id;
    }
    return;
  }
  if (loadTimer) clearTimeout(loadTimer);
  if (loadedItemId === node.id) return;

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
      const data = await db.get(node.ref.collection, node.ref.id);
      content = data ? highlightJson(data) : pc.yellow("Object not found.");
    } else if (node.type === "collection" && node.ref) {
      const objs = objectsByCollection.get(node.ref.name) ?? [];
      const hist = colHistory.get(node.ref.name) ?? [];
      let res = `${pc.bold(node.ref.name)} ${pc.dim(`(${objs.length} objects)`)}\n\n`;
      res += `${pc.bold("Performance")}\n`;
      res += `  Volume    ${pc.cyan(drawSparkline(hist))}\n\n`;

      if (objs.length === 0) {
        res += pc.dim("  No objects in this collection.");
      } else {
        const lines = objs.map((id) => `  ${pc.dim("●")} ${id}`);
        res += lines.join("\n");
      }
      content = res;
    } else if (node.type === "stream" && node.ref) {
      const events = await db.events.list(node.ref.name);
      const hist = streamHistory.get(node.ref.name) ?? [];
      
      let res = `${pc.bold(node.ref.name)} ${pc.dim(`(${events.length} events)`)}\n\n`;
      res += `${pc.bold("Performance")}\n`;
      res += `  Volume    ${pc.green(drawSparkline(hist))}\n\n`;

      if (events.length === 0) {
        res += pc.dim("  No events in this stream.");
      } else {
        const lines = events.map((e: any) => {
          const ts = e.createdAt ? pc.dim(String(e.createdAt)) : "";
          const type = pc.magenta(e.type || "unknown");
          return `  ${ts} ${type}`;
        });
        res += lines.join("\n");
      }
      content = res;
    } else if (node.type === "queue" && node.ref) {
      const queue = db.queue(node.ref.name);
      const [active, dead] = await Promise.all([queue.list(), queue.dead()]);
      
      const aHist = queueActiveHistory.get(node.ref.name) ?? [];
      const dHist = queueDeadHistory.get(node.ref.name) ?? [];

      let res = `${pc.bold(node.ref.name)}\n\n`;
      res += `${pc.bold("Performance")}\n`;
      res += `  Active    ${pc.cyan(drawSparkline(aHist))}\n`;
      res += `  Dead      ${pc.red(drawSparkline(dHist))}\n\n`;
      res += `${pc.cyan("Active")} ${pc.dim(`(${active.length})`)}\n`;
      if (active.length === 0) {
        res += pc.dim("  No active jobs\n");
      } else {
        for (const j of active as any[]) {
          res += `  ${pc.dim("●")} ${j.id} ${pc.yellow(j.status)} ${pc.dim(`${j.attempts}/${j.maxAttempts}`)}\n`;
        }
      }
      res += `\n${pc.red("Dead")} ${pc.dim(`(${dead.length})`)}\n`;
      if (dead.length === 0) {
        res += pc.dim("  No dead jobs\n");
      } else {
        for (const j of dead as any[]) {
          res += `  ${pc.dim("●")} ${j.id} ${pc.dim(`${j.attempts}/${j.maxAttempts}`)}\n`;
        }
      }
      content = res;
    } else if (node.type === "status") {
      const W = process.stdout.columns || 80;
      const sideW = Math.min(40, Math.max(20, Math.floor(W * 0.35)));
      const viewW = Math.max(20, W - sideW - 3);
      const fullRule = pc.dim("─".repeat(Math.max(10, viewW - 2)));

      const uptime = startedAt ? formatUptime(Date.now() - startedAt) : "--";

      // ── Header
      const titleStr = `${pc.bold("thingd")}  ${pc.cyan("METRICS")}`;
      const pathStr = pc.dim(dbPath || ":memory:");
      const pathRaw = dbPath || ":memory:";
      const gap = Math.max(2, viewW - 2 - 8 - "METRICS".length - pathRaw.length);
      content  = `  ${titleStr}${" ".repeat(gap)}${pathStr}\n`;
      content += `  ${pc.dim("uptime")} ${pc.dim(uptime)}\n`;
      content += `  ${fullRule}\n\n`;

      // ── Physical Store & Driver Logic
      let sizeKb = 0;
      if (driver === "native" && dbPath) {
        try {
          sizeKb = Math.round(fs.statSync(dbPath).size / 1024);
        } catch {}
      }
      const dbSizeStr = driver === "native" ? `${sizeKb} KB` : "--";

      let driverName = "Unknown";
      if (driver === "memory") driverName = "SQLite (Memory)";
      else if (driver === "native") driverName = "SQLite (Native)";
      else if (driver === "cloud") driverName = "Cloud (Remote)";

      const objVal = String(totalObjects).padEnd(8);
      const evtVal = String(totalEventsCount).padEnd(8);
      const actVal = String(totalActiveJobsCount).padEnd(8);
      const ddtVal = String(totalDeadJobsCount).padEnd(8);

      // ── Metrics Layout 
      content += `  ${pc.bold("CAPACITY & STORAGE METRICS")}\n`;
      content += `  ${fullRule}\n`;
      content += `  ${pc.dim("Objects").padEnd(20)}  ${pc.cyan(objVal)} ${pc.dim("total objects stored")}\n`;
      content += `  ${pc.dim("Events").padEnd(20)}  ${pc.green(evtVal)} ${pc.dim("total events in streams")}\n`;
      content += `  ${pc.dim("Active Jobs").padEnd(20)}  ${pc.yellow(actVal)} ${pc.dim("jobs currently processing")}\n`;
      content += `  ${pc.dim("Dead Jobs").padEnd(20)}  ${pc.red(ddtVal)} ${pc.dim("failed/dead jobs")}\n\n`;

      content += `  ${pc.bold("PHYSICAL STORE & CONNECTION")}\n`;
      content += `  ${fullRule}\n`;
      content += `  ${pc.dim("Database Size").padEnd(20)}  ${pc.blue(dbSizeStr)}\n`;
      content += `  ${pc.dim("Driver Type").padEnd(20)}  ${driverName}\n`;
      content += `  ${pc.dim("Storage Path").padEnd(20)}  ${dbPath || ":memory:"}\n`;
      content += `  ${pc.dim("CLI Shortcuts").padEnd(20)}  ${pc.bold("[c]")} Create  ${pc.bold("[r]")} Refresh\n\n`;

      // ── Throughput & Activity Metrics
      const currentWrite = objectWriteRateHistory[objectWriteRateHistory.length - 1] ?? 0;
      const currentAppend = eventAppendRateHistory[eventAppendRateHistory.length - 1] ?? 0;

      const peakWrite = Math.max(5, ...objectWriteRateHistory);
      const peakAppend = Math.max(5, ...eventAppendRateHistory);

      // Adjust sparkline width to prevent terminal wrapping. Total fixed chars ~55.
      const sparkW = Math.max(10, viewW - 55);

      const wLine = drawSparkline(objectWriteRateHistory, 5, sparkW);
      const apLine = drawSparkline(eventAppendRateHistory, 5, sparkW);

      content += `  ${pc.bold("THROUGHPUT & ACTIVITY METRICS")}\n`;
      content += `  ${fullRule}\n`;
      
      content += `  ${pc.dim("Object Writes".padEnd(16))} ${pc.cyan(wLine)}  ${pc.cyan(String(currentWrite).padEnd(4))} ${pc.dim(`writes/s  (Peak: ${peakWrite}/s)`)}\n\n`;
      content += `  ${pc.dim("Event Appends".padEnd(16))} ${pc.green(apLine)}  ${pc.green(String(currentAppend).padEnd(4))} ${pc.dim(`appends/s (Peak: ${peakAppend}/s)`)}\n\n`;
    } else if (node.type === "category") {
      content = pc.dim("Expand to browse items.");
    } else {
      content = "";
    }

    if (loadedItemId === snapId) {
      viewerLines = content.split("\n");
      draw();
    }
  } catch (err: any) {
    if (loadedItemId === snapId) {
      viewerLines = [pc.red(`Error: ${err.message}`)];
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
  if (cursorIndex < 0) cursorIndex = 0;

  // Scroll sidebar
  if (cursorIndex >= scrollOffset + bodyH) {
    scrollOffset = cursorIndex - bodyH + 1;
  } else if (cursorIndex < scrollOffset) {
    scrollOffset = cursorIndex;
  }
  scrollOffset = Math.max(0, Math.min(scrollOffset, Math.max(0, tree.length - bodyH)));

  let buf = "\u001B[H"; // Move to top-left

  // Header
  let titleStr: string;
  if (!connected) {
    titleStr = ` thingd  ${pc.dim("|")} Select Environment `;
  } else if (formState?.active) {
    titleStr = ` thingd  ${pc.dim("|")} ${driver.toUpperCase()} ${pc.dim("|")} Input Mode `;
  } else {
    titleStr = ` thingd  ${pc.dim("|")} ${driver.toUpperCase()} ${pc.dim("|")} ${dbPath} `;
  }
  buf += pc.inverse(padToWidth(titleStr, W)) + "\n";

  // Separator
  buf += pc.dim("─".repeat(sideW) + "─┬─" + "─".repeat(viewW)) + "\n";

  // Build Form Lines if active
  if (formState?.active) {
    viewerLines = [
      `${pc.bgCyan(pc.black(` ${formState.title} `))}`,
      ""
    ];
    for (let i = 0; i < formState.fields.length; i++) {
      const f = formState.fields[i];
      if (!f) continue;
      const isSel = i === formState.activeIndex;
      let displayLabel = f.label;
      if (f.options && f.allowCustom && f.value && !f.options.includes(f.value)) {
        displayLabel += pc.green(" (New)");
      }
      viewerLines.push(`${isSel ? pc.yellow("▶") : " "} ${pc.bold(displayLabel)}`);
      
      let displayVal = f.value;
      if (f.isSecret) displayVal = "*".repeat(displayVal.length);
      if (displayVal === "" && f.placeholder) {
        displayVal = pc.dim(f.placeholder);
      }
      
      if (isSel && !formState.isSubmitting) {
        if (f.options && !f.allowCustom) {
          viewerLines.push(`    ${pc.cyan("◀ ")}${pc.inverse(displayVal || " ")}${pc.cyan(" ▶")}`);
        } else if (f.options && f.allowCustom) {
          const inOptions = f.options.includes(f.value);
          if (inOptions) {
            viewerLines.push(`    ${pc.cyan("◀ ")}${displayVal}${pc.inverse(" ")}${pc.cyan(" ▶")}`);
          } else {
            viewerLines.push(`    ${displayVal}${pc.inverse(" ")}`);
          }
        } else {
          viewerLines.push(`    ${displayVal}${pc.inverse(" ")}`); // cursor block
        }
      } else {
        viewerLines.push(`    ${displayVal}`);
      }
      viewerLines.push("");
    }
    if (formState.error) {
      viewerLines.push(pc.red(formState.error));
    }
    if (formState.isSubmitting) {
      viewerLines.push(pc.cyan("Processing..."));
    }
    viewerLines.push("");
    viewerLines.push(pc.dim("  [Enter] Next/Submit   [Esc] Cancel"));
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

    buf += left + pc.dim(" │ ") + right + "\n";
  }

  // Separator
  buf += pc.dim("─".repeat(sideW) + "─┴─" + "─".repeat(viewW)) + "\n";

  // Footer
  let help: string;
  if (formState?.active) {
    const hasOptions = formState.fields[formState.activeIndex]?.options;
    help = ` ${pc.dim("↑↓")} focus  ${hasOptions ? pc.dim("←→") + " select  " : ""}${pc.dim("enter")} submit  ${pc.dim("ctrl+e")} editor  ${pc.dim("esc")} cancel `;
  } else if (!connected) {
    help = ` ${pc.dim("↑↓")} nav  ${pc.dim("enter")} connect  ${pc.dim("q")} quit `;
  } else {
    help = ` ${pc.dim("↑↓")} nav  ${pc.dim("←→")} toggle  ${pc.dim("c")} create  ${pc.dim("e")} edit  ${pc.dim("d")} delete  ${pc.dim("/")} search  ${pc.dim("i")} info  ${pc.dim("r")} refresh  ${pc.dim("s")} switch  ${pc.dim("q")} quit `;
  }
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
    if (ch === undefined) break;
    if (ch === "\u001B") {
      // Consume ANSI sequence
      let seq = ch;
      i++;
      while (i < chars.length) {
        const next = chars[i];
        if (next === undefined || /[a-zA-Z]/.test(next)) break;
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
    if (cp === undefined) break;
    const cw = cp > 0xffff ? 2 : 1;
    if (w + cw > targetW) break;
    result += ch;
    w += cw;
    i++;
  }
  return result;
}

/** Simple pad with visible width awareness. */
function padToWidth(text: string, width: number): string {
  const vw = visibleWidth(text);
  if (vw >= width) return text;
  return text + " ".repeat(width - vw);
}


// ── Utils ────────────────────────────────────────────────────────────

async function launchEditor(f: FormField) {
  if (process.stdin.isTTY) process.stdin.setRawMode(false);
  process.stdin.removeListener("keypress", keypressHandler!);
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
  } catch (e) {}

  if (process.stdin.isTTY) process.stdin.setRawMode(true);
  process.stdin.on("keypress", keypressHandler!);
  draw();
}

function parsePayload(str: string): any {
  str = str.trim();
  if (!str) return {};
  if (str.startsWith("{") || str.startsWith("[")) {
    return JSON.parse(str);
  }
  
  const obj: Record<string, any> = {};
  const parts = str.match(/(?:[^\s"]+|"[^"]*")+/g) || [];
  
  for (const part of parts) {
    const eqIdx = part.indexOf("=");
    if (eqIdx === -1) {
      obj[part] = true;
      continue;
    }
    const k = part.substring(0, eqIdx);
    let v: any = part.substring(eqIdx + 1);
    
    if (v.startsWith('"') && v.endsWith('"')) {
      v = v.substring(1, v.length - 1);
    } else {
      if (v === "true") v = true;
      else if (v === "false") v = false;
      else if (!isNaN(Number(v))) v = Number(v);
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
    if (selected.type === "collection") { defaultCol = selected.ref?.name ?? ""; }
    else if (selected.type === "object") { defaultCol = selected.ref?.collection ?? ""; }
    else if (selected.type === "stream") { defaultStream = selected.ref?.name ?? ""; }
    else if (selected.type === "queue") { defaultQueue = selected.ref?.name ?? ""; }
  }

  openForm("Create Resource", [
    { id: "kind", label: "Kind (object, event, queue)", value: defaultStream ? "event" : defaultQueue ? "queue" : "object", options: ["object", "event", "queue"] },
    { id: "target", label: "Target (Collection, Stream, or Queue Name)", value: defaultCol || defaultStream || defaultQueue, options: Array.from(new Set([...collections, ...streams, ...queues])).sort(), allowCustom: true },
    { id: "objId", label: "Object ID (only for objects, auto if blank)", placeholder: "Leave blank to auto-generate" },
    { id: "payload", label: "Data (JSON or key=value)", placeholder: 'e.g. name="John" age=30' }
  ], async (vals) => {
    const kind = (vals.kind || "").toLowerCase();
    const target = (vals.target || "").trim();
    if (!target) throw new Error("Target is required.");

    if (kind === "object") {
      let id = vals.objId?.trim();
      if (!id) {
        try {
          id = crypto.randomUUID();
        } catch (e) {
          id = "obj_" + Date.now().toString(36) + Math.random().toString(36).substring(2);
        }
      }
      const data = parsePayload(vals.payload || "");
      await db.put(target, { id, ...data });
      expandedSet.add("cat:collections");
      expandedSet.add(`col:${target}`);
    } else if (kind === "event") {
      if (!vals.payload?.trim()) throw new Error("Event Type is required (in Data field for events).");
      await db.events.append(target, { type: vals.payload.trim() });
      expandedSet.add("cat:streams");
    } else if (kind === "queue") {
      if (!vals.payload?.trim()) throw new Error("Payload is required.");
      const data = parsePayload(vals.payload);
      await db.queue(target).push(data);
      expandedSet.add("cat:queues");
    } else {
      throw new Error("Kind must be 'object', 'event', or 'queue'.");
    }
  });
}

async function handleEdit(selected: TreeNode | undefined) {
  if (!selected) return;

  if (selected.type === "object" && selected.ref) {
    const ref = selected.ref;
    const current = await db.get(ref.collection, ref.id);
    const clean = current ? { ...current } : {};
    for (const k of ["id", "collection", "createdAt", "updatedAt", "version"]) {
      delete (clean as any)[k];
    }

    openForm(`Edit Object: ${ref.id}`, [
      { id: "payload", label: "Data (JSON or key=value)", value: JSON.stringify(clean) }
    ], async (vals) => {
      const data = parsePayload(vals.payload || "");
      await db.put(ref.collection, { id: ref.id, ...data });
    });
  } else if (selected.type === "queue" && selected.ref) {
    const ref = selected.ref;
    const queue = db.queue(ref.name);
    
    openForm(`Manage Queue: ${ref.name}`, [
      { id: "action", label: "Action (claim, push)", value: "claim", options: ["claim", "push"] },
      { id: "payload", label: "Job Data (JSON or key=value, only for push)", placeholder: 'task="email"' }
    ], async (vals) => {
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
    });
  } else {
    openForm("Edit Not Supported", [
      { id: "msg", label: "Error", value: "Editing is only available for Objects and Queues." }
    ], async () => {});
  }
}

async function handleDelete(selected: TreeNode | undefined) {
  if (!selected) return;

  if (selected.type === "object" && selected.ref) {
    const ref = selected.ref;
    openForm(`Delete Object: ${ref.id}`, [
      { id: "confirm", label: 'Type "yes" to confirm deletion', placeholder: "yes" }
    ], async (vals) => {
      if ((vals.confirm || "").toLowerCase() !== "yes") throw new Error("Canceled");
      await db.delete(ref.collection, ref.id);
    });
  } else if (selected.type === "queue" && selected.ref) {
    const ref = selected.ref;
    openForm(`Resolve Queue Job`, [
      { id: "jobId", label: "Leased Job ID", placeholder: "job-id" },
      { id: "action", label: "Action (ack, nack)", value: "ack" }
    ], async (vals) => {
      const jobId = (vals.jobId || "").trim();
      const action = vals.action || "";
      if (!jobId) throw new Error("Job ID required.");
      if (action === "ack") {
        await db.queue(ref.name).ack(jobId);
      } else if (action === "nack") {
        await db.queue(ref.name).nack(jobId, { error: "Rejected via CLI" });
      } else {
        throw new Error("Action must be 'ack' or 'nack'.");
      }
    });
  } else {
    openForm("Delete Not Supported", [
      { id: "msg", label: "Error", value: "Deletion is only available for Objects and Queues." }
    ], async () => {});
  }
}

async function handleSearch() {
  openForm("Global Search", [
    { id: "query", label: "Search Query", placeholder: "text to search" },
    { id: "limit", label: "Limit (optional)", placeholder: "100" }
  ], async (vals) => {
    const query = (vals.query || "").trim();
    if (!query) throw new Error("Search query required.");
    const limitStr = vals.limit || "";
    const options: any = {};
    if (limitStr) {
      const limit = parseInt(limitStr, 10);
      if (!isNaN(limit)) options.limit = limit;
    }
    const results = await db.search(query, options);
    
    // Display results in the viewer
    viewerLines = [
      `  ${pc.bold("Search Results:")} ${pc.cyan(query)}`,
      "",
      ...(results.length === 0 ? ["  No results found."] : []),
      ...results.map((r: any) => {
        const id = pc.green(r.id);
        const col = pc.cyan(r.kind === "object" ? r.collection : r.stream);
        const textStr = r.value?.text ? pc.dim(r.value.text.substring(0, 100)) : "";
        return `  ${col} / ${id} ${textStr}`;
      })
    ];
    loadedItemId = "search_results";
  });
}

async function handleInfo() {
  const lines: string[] = [
    `  ${pc.bold("Connection Status")}`,
    "",
    `  Driver: ${pc.cyan(driver)}`,
    `  Path:   ${pc.cyan(dbPath)}`,
  ];

  if (driver === "cloud") {
    try {
      const baseUrl = dbPath.startsWith("thingd://") ? `http://${dbPath.slice("thingd://".length)}` : dbPath;
      const urlObj = new URL(baseUrl);
      if (urlObj.pathname === "/mcp") urlObj.pathname = "/";
      const fetchJson = async (p: string) => {
        const u = new URL(p, urlObj.toString());
        const headers: Record<string, string> = {};
        if (authToken) headers["Authorization"] = `Bearer ${authToken}`;
        const res = await fetch(u, { headers });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json();
      };

      const health = await fetchJson("/healthz");
      const cluster = await fetchJson("/cluster/status");

      lines.push("");
      lines.push(`  ${pc.bold("Cloud Health")}`);
      lines.push(...JSON.stringify(health, null, 2).split("\n").map(l => `  ${pc.dim(l)}`));
      
      lines.push("");
      lines.push(`  ${pc.bold("Cloud Cluster")}`);
      lines.push(...JSON.stringify(cluster, null, 2).split("\n").map(l => `  ${pc.dim(l)}`));
      
    } catch (err: any) {
      lines.push("", `  ${pc.red("Cloud Query Failed:")} ${err.message}`);
    }
  }

  viewerLines = lines;
  loadedItemId = "info_status";
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
    if (!key) return;

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
        if (formState.activeIndex > 0) formState.activeIndex--;
        formState.error = undefined;
        draw();
      } else if (key.name === "down" || key.name === "tab") {
        if (formState.activeIndex < formState.fields.length - 1) formState.activeIndex++;
        formState.error = undefined;
        draw();
      } else if (key.name === "return") {
        if (formState.activeIndex < formState.fields.length - 1) {
          formState.activeIndex++;
          draw();
        } else {
          const vals: Record<string, string> = {};
          for (const f of formState.fields) vals[f.id] = f.value;
          formState.onSubmit(vals);
        }
      } else if (key.name === "left" || key.name === "right") {
        const f = formState.fields[formState.activeIndex];
        if (f && f.options && f.options.length > 0) {
          const currentIndex = f.options.indexOf(f.value);
          let nextIndex = key.name === "right" ? currentIndex + 1 : currentIndex - 1;
          if (nextIndex < 0) nextIndex = f.options.length - 1;
          if (nextIndex >= f.options.length) nextIndex = 0;
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
          const clean = str.replace(/[\x00-\x1F\x7F]/g, "");
          if (clean) {
            f.value += clean;
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
        if (n) scheduleLoad(n);
      }
    } else if (key.name === "down" || str === "j") {
      if (cursorIndex < tree.length - 1) {
        cursorIndex++;
        draw();
        const n = tree[cursorIndex];
        if (n) scheduleLoad(n);
      }
    } else if (!connected) {
      // Driver selection mode — only Enter works
      if (key.name === "return") {
        const node = tree[cursorIndex];
        if (node) await handleConnect(node);
      }
    } else {
      // Connected mode — full set of shortcuts
      if (key.name === "right" || str === "l") {
        const node = tree[cursorIndex];
        if (node?.expandable) {
          if (!expandedSet.has(node.id)) {
            expandedSet.add(node.id);
            if (node.type === "collection") await fetchResources();
            draw();
          } else {
            const newTree = buildTree();
            if (cursorIndex + 1 < newTree.length) {
              cursorIndex++;
              draw();
              const n = newTree[cursorIndex];
              if (n) scheduleLoad(n);
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
              if (n) scheduleLoad(n);
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
            if (node.type === "collection") await fetchResources();
          }
          draw();
        }
      } else if (str === "r" || str === "R") {
        loadedItemId = "";
        await fetchResources();
        draw();
        const n = tree[cursorIndex];
        if (n) scheduleLoad(n);
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
  if (pollTimer) clearInterval(pollTimer);
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
  if (node.type !== "driver" || !node.ref) return;

  const selectedDriver = node.ref.driver as string;

  if (selectedDriver === "native" || selectedDriver === "cloud") {
    openForm(`Connect to ${selectedDriver}`, [
      ...(selectedDriver === "cloud" ? [
        { id: "url", label: "Cloud URL", value: "http://localhost:3000" },
        { id: "token", label: "Bearer Token (optional)", isSecret: true }
      ] : [
        { id: "path", label: "Database Path", value: path.join(os.homedir(), "Downloads", "data.db") }
      ])
    ], async (vals) => {
      const resolvedPath = selectedDriver === "cloud" ? vals.url || "" : vals.path || "";

      // Allow the underlying SDK/SQLite driver to automatically create the file
      // if it does not exist, rather than throwing an error here.

      db = await ThingD.open({
        path: resolvedPath,
        url: selectedDriver === "cloud" ? resolvedPath : undefined,
        driver: selectedDriver as any,
        authToken: vals.token,
      });

      driver = selectedDriver;
      dbPath = resolvedPath;
      
      // Update global authToken safely
      if (typeof vals.token === "string") {
        authToken = vals.token;
      } else {
        authToken = "";
      }
      
      connected = true;
      startedAt = Date.now();
      cursorIndex = 0;
      scrollOffset = 0;
      loadedItemId = "";
      
      await fetchResources();
      draw();
      const tree = buildTree();
      const first = tree[cursorIndex];
      if (first) scheduleLoad(first);
    });
  } else {
    // Memory — connect directly without suspending
    driver = selectedDriver;
    dbPath = ":memory:";
    viewerLines = [pc.dim("Connecting...")];
    draw();

    try {
      db = await ThingD.open({
        path: ":memory:",
        driver: "memory" as any,
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
      if (first) scheduleLoad(first);
    } catch (error: any) {
      viewerLines = [pc.red(`Failed to connect: ${error.message}`)];
      draw();
    }
  }
}

async function handleSwitch() {
  if (!connected) return;

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
  if (first) scheduleLoad(first);
}

// ── Entry Point ──────────────────────────────────────────────────────

export async function runInteractiveCli(): Promise<void> {
  // Go straight into the TUI — no pre-prompts
  console.clear();
  process.stdout.write("\u001B[?1049h\u001B[H\u001B[?25l");

  // Show the driver selection screen
  viewerLines = ["Select an environment to connect."];
  draw();
  const tree = buildTree();
  const first = tree[cursorIndex];
  if (first) scheduleLoad(first);

  setupKeypress();

  // Background polling loop for real-time updates
  pollTimer = setInterval(async () => {
    if (connected && !formState?.active) {
      const snapItemId = loadedItemId;
      await fetchResources();
      draw();
      const tree = buildTree();
      const n = tree[cursorIndex];
      // Silently reload content if the same node is still actively viewed
      if (n && snapItemId === n.id && n.type !== "category") {
        await loadContent(n).catch(() => {});
      }
    }
  }, 2000);
}
