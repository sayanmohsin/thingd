# NestJS Basic Example

This example shows how a NestJS app can treat `memoryd` as a local application memory layer for objects, events, and background jobs.

This example currently uses a small in-memory NestJS adapter with the same shape the SDK exposes. The public SDK also has an in-memory store today; the Rust-backed persistent store is planned next.

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

## Future Rust-backed Integration

Once `@sayanmohsin/memoryd` is backed by the Rust engine, the service can be changed to open a real local database:

```ts
const db = await MemoryD.open("./memoryd.db");
```

The controller layer should not need to know whether `memoryd` is in embedded mode, sidecar mode, or remote primary-writer mode.
