# @sayanmohsin/memoryd

Node.js SDK for `memoryd`.

This package currently exposes the intended object/event/queue abstractions with
an in-memory store by default. Inside this repo it can also use the private
Rust-backed native driver for local SQLite persistence and a remote driver for
sidecar/server mode.

Current SDK surface:

- object `put`, `get`, and `delete`
- event `append` and `list`
- basic search over objects and events
- queue `push`, `claim`, `ack`, `nack`, `list`, and `dead`

The in-memory store is for API design and local integration testing. It is not persistent storage.

To test the Rust-backed path locally:

```bash
pnpm --filter @sayanmohsin/memoryd-native build
```

```ts
import { MemoryD } from "@sayanmohsin/memoryd";

const db = await MemoryD.open({
  path: "./memoryd.db",
  driver: "native",
});
```

The native driver is private for now.

You can install the published SDK from npm:

```bash
npm install @sayanmohsin/memoryd
```

To use a running `memoryd` sidecar or Docker runtime:

```bash
MEMORYD_URL=http://127.0.0.1:8757
MEMORYD_AUTH_TOKEN=change-me
```

```ts
import { MemoryD } from "@sayanmohsin/memoryd";

const db = await MemoryD.open();
```

Or configure the remote driver directly:

```ts
const db = await MemoryD.open({
  url: "http://127.0.0.1:8757/mcp",
  driver: "remote",
  authToken: "change-me",
});
```

Remote mode uses the same SDK methods and talks to the sidecar over Streamable
HTTP MCP.

`memoryd` is open source under the Apache-2.0 license.
