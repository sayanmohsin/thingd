# @sayanmohsin/memoryd

Node.js SDK for `memoryd`.

This package currently exposes the intended object/event/queue abstractions with an in-memory store by default. Inside this repo it can also use the private Rust-backed native driver for local SQLite persistence.

Current SDK surface:

- object `put`, `get`, and `delete`
- event `append` and `list`
- basic search over objects and events
- queue `push`, `claim`, `ack`, `nack`, `list`, and `dead`

The in-memory store is for API design and local integration testing. It is not persistent storage.

To test the Rust-backed path locally:

```bash
npm run build --workspace @sayanmohsin/memoryd-native
```

```ts
import { MemoryD } from "@sayanmohsin/memoryd";

const db = await MemoryD.open({
  path: "./memoryd.db",
  driver: "native",
});
```

The native driver is private for now. Do not rely on native prebuilds or npm
installation until the release strategy is added.

`memoryd` is open source under the Apache-2.0 license.
