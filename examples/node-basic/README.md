# 📦 thingd Node.js Basic Example

A runnable, clean demonstration of how you can treat `thingd` as a local, fully-featured application memory layer for Node.js apps.

This example runs in disposable **in-memory mode** by default. When the native
addon is available, that mode uses ThingDB RAM storage; otherwise the SDK uses
its portable TypeScript reference store. It can also be configured to run with
the native Rust durable driver or in remote sidecar mode.

---

## 🚀 Quick Start

Ensure you have built the workspace packages first (from the workspace root):

```bash
# Build workspace
pnpm build
```

Then, navigate to this directory, install dependencies, and run the script:

```bash
# Navigate to the example
cd examples/node-basic

# Install dependencies (links local package)
pnpm install

# Run the example in production mode
pnpm start

# Run the example in development mode with auto-reload (watch mode)
pnpm dev
```

---

## 🛠️ What This Example Demonstrates

The [`index.ts`](./index.ts) script takes you step-by-step through the core features of `thingd`:

### 1. Unified Client Open
Instantiates the database client. By default, it runs in process-local memory:
```ts
import { ThingD } from "@thingd/sdk";
const db = await ThingD.open();
```

### 2. Document & Object Store
Put and retrieve schema-less object documents. Every document receives auto-incrementing `version` control and timestamp tracking:
```ts
const decision = await db.put("decisions", {
  id: "rust-core",
  text: "Use Rust for the engine and TypeScript for the developer API.",
});

const stored = await db.get("decisions", "rust-core");
```

### 3. Ordered Event Sourcing
Append and retrieve immutable event logs organized by stream channels (ideal for auditing, pub/sub notifications, and history tracking):
```ts
await db.events.append("project:thingd", {
  type: "decision.made",
  text: "thingd should be object-shaped and MCP-native.",
});

const eventLogs = await db.events.list("project:thingd");
```

### 4. Background Job Queues
Robust FIFO job queueing with lease timers, manual acknowledgments (`ack`), negative acknowledgments (`nack`), idempotency keys, delayed execution, and automatic dead-letter queueing:
```ts
const queue = db.queue("embed");

// Push a job
const job = await queue.push({ object: "decisions/rust-core" });

// Claim a job (leases it for 10 seconds)
const claimedJob = await queue.claim({ leaseMs: 10000 });

// Acknowledge completion
if (claimedJob) {
  await queue.ack(claimedJob.id);
}
```

### 5. Multi-Source Search
Perform high-performance full-text search query mapping across both stored object documents and event streams concurrently:
```ts
const hits = await db.search("rust");
```

---

## 💾 Alternative Storage Drivers

You don't need to change any controller or application logic to change storage modes:

### Local Persistent File (Native Driver)
Ensure you have built the native crate (`pnpm build` in the workspace root first) and open with the `native` driver:
```ts
const db = await ThingD.open({
  path: "./thingd.db",
  driver: "native",
});
```

The native driver uses RocksDB by default. The experimental ThingDB backend can
be selected with `THINGD_STORAGE_BACKEND=thingdb`; it uses a separate format.

### Remote Sidecar / Server Mode
To use `thingd` running as a background service or remote instance, set the following environment variables:
```bash
export THINGD_URL="http://127.0.0.1:8757"
export THINGD_AUTH_TOKEN="your-secret-token"
```
Then initialize the SDK; with `THINGD_URL` set this connects to the sidecar,
otherwise it uses the local in-memory mode described above:
```ts
const db = await ThingD.open();
```
