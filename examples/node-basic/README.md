# Node Basic Example

Planned example for the first Node.js SDK release:

```ts
import { MemoryD } from "@sayanmohsin/memoryd";

const db = await MemoryD.open();

await db.put("decisions", {
  id: "rust-core",
  text: "Use Rust for the engine and TypeScript for the API.",
});

await db.events.append("project:memoryd", {
  type: "decision.made",
  text: "memoryd should be object-shaped and MCP-native.",
});

await db.queue("embed").push({
  object: "decisions/rust-core",
});

const job = await db.queue("embed").claim({
  leaseMs: 30_000,
});

if (job) {
  await db.queue("embed").ack(job.id);
}
```

Use sidecar mode by setting:

```bash
MEMORYD_URL=http://127.0.0.1:8757
MEMORYD_AUTH_TOKEN=change-me
```

Use the local native SQLite driver explicitly:

```ts
const db = await MemoryD.open({
  path: "./memoryd.db",
  driver: "native",
});
```
