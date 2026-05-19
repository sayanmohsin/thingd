# NestJS Basic Example

This example shows how a NestJS app can treat `memoryd` as a local application memory layer for objects, events, and background jobs.

This example currently uses a small in-memory NestJS adapter with the same shape
the SDK exposes. The public SDK also supports in-memory, native, and remote
sidecar drivers.

## Run

```bash
npm install
npm run start:dev
```

The app listens on `http://localhost:3000`.

## Routes

Create a decision object, append an event, and enqueue an embedding job:

```bash
curl -X POST http://localhost:3000/decisions \
  -H "content-type: application/json" \
  -d '{
    "id": "rust-core",
    "text": "Use Rust for the core engine and TypeScript for the developer API.",
    "project": "memoryd"
  }'
```

Read a decision:

```bash
curl http://localhost:3000/decisions/rust-core
```

Enqueue an embedding job directly:

```bash
curl -X POST http://localhost:3000/jobs/embed \
  -H "content-type: application/json" \
  -d '{ "object": "decisions/rust-core" }'
```

List queued embedding jobs:

```bash
curl http://localhost:3000/jobs/embed
```

## SDK Integration

For local Rust-backed storage:

```ts
const db = await MemoryD.open({
  path: "./memoryd.db",
  driver: "native",
});
```

For sidecar mode:

```bash
MEMORYD_URL=http://127.0.0.1:8757
MEMORYD_AUTH_TOKEN=change-me
```

```ts
const db = await MemoryD.open();
```

The controller layer should not need to know whether `memoryd` is in embedded mode, sidecar mode, or remote primary-writer mode.
