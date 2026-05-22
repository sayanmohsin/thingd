# NestJS Basic Example

This example shows how a NestJS app can treat `thingd` as a local application memory layer for objects, events, and background jobs.

This example currently uses a small in-memory NestJS adapter with the same shape
the SDK exposes. The public SDK also supports in-memory, native, and remote
sidecar drivers.

## Installation

```bash
npm install thingd
```

## Run

```bash
pnpm install
pnpm start:dev
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
    "project": "thingd"
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
const db = await ThingD.open({
  path: "./thingd.db",
  driver: "native",
});
```

For sidecar mode:

```bash
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=change-me
```

```ts
const db = await ThingD.open();
```

The controller layer should not need to know whether `thingd` is in embedded mode, sidecar mode, or remote primary-writer mode.
