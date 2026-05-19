# Node Basic Example

Planned example for the first Node.js SDK release:

```ts
import { MemoryD } from "@sayanmohsin/memoryd";

const db = await MemoryD.open("./memoryd.db");

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
