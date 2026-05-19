# NestJS Basic Example

This example shows how a NestJS app can treat `memoryd` as a local application memory layer for objects, events, and background jobs.

Until the Node.js SDK is wired to the Rust engine, this example uses a small in-memory adapter with the same shape the SDK is expected to expose.

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

## Future SDK Integration

The example currently uses `MemorydService` as an in-memory adapter. Once `@sayanmohsin/memoryd` is backed by the Rust engine, the service can be changed to open a real local database:

```ts
const db = await MemoryD.open("./memoryd.db");
```

The controller layer should not need to know whether `memoryd` is in embedded mode, sidecar mode, or remote primary-writer mode.
