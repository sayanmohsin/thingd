# @thingd/sdk

[![npm](https://img.shields.io/npm/v/@thingd/sdk?label=@thingd/sdk&logo=npm&color=ff6a00)](https://www.npmjs.com/package/@thingd/sdk)

Node.js SDK for [thingd](https://github.com/sayanmohsin/thingd) — a fast object-first data engine for applications and AI agents.

## Install

```bash
npm install @thingd/sdk
```

## Quick start

```typescript
import { ThingD } from "@thingd/sdk";

const db = await ThingD.open();
await db.put("notes", { id: "hello", text: "Hello world" });
const obj = await db.get("notes", "hello");
console.log(obj); // { id: "hello", text: "Hello world", collection: "notes", version: 1, ... }
```

### Optional native storage encryption

Native persistent storage accepts a 64-character hexadecimal representation of
a 32-byte encryption key:

```typescript
const db = await ThingD.open({
  driver: "native",
  path: "./thingd-data",
  encryption: { key: process.env.THINGD_ENCRYPTION_KEY },
});
```

The key is required every time an encrypted database is reopened. Memory and
cloud drivers reject this local option. Do not commit keys or put them in MCP
configuration; inject them through the deployment environment. Storage
encryption does not change object, event, queue, graph, vector, search, REST,
or MCP APIs.

Native persistence uses RocksDB by default. The experimental Rust-native
ThingDB backend can be selected with `THINGD_STORAGE_BACKEND=thingdb`; it uses
a separate format and requires logical repack when changing an existing store.
For disposable Node.js memory mode, `ThingD.open(":memory:")` uses ThingDB RAM
when the native addon is available; the explicit `memory` subpath remains the
portable TypeScript/reference implementation. ThingDB RAM is the full
process-local Thingd database; for a smaller TTL/LRU key/value cache, use the
separate ThingDB cache API. Both modes lose data when the process exits.

## Subpath imports

```typescript
// Full SDK (Node.js: MCP + REST + stores + native binding)
import { ThingD } from "@thingd/sdk";

// Lightweight HTTP client (Node.js + Bun, works in browsers via @thingd/client)
import { openThingD, HttpThingStore } from "@thingd/sdk/client";

// Pure in-memory store (browser + Node.js + Bun)
import { openMemoryThingD, InMemoryThingStore } from "@thingd/sdk/memory";

// Types only (for type-safe dependency injection)
import type { ThingDConnection } from "@thingd/sdk/types";
```

## Browser / Edge

For browser, Cloudflare Workers, and other edge runtimes, use the standalone
`@thingd/client` package — a zero-dependency REST client:

```bash
npm install @thingd/client
```

```ts
import { ThingdClient } from "@thingd/client";

const db = new ThingdClient({
  url: "https://api.thingd.cloud",
  authToken: "sk-...",
});
```

Or use the subpath import (works in Node.js and Bun, bundled for browser):

```ts
import { HttpThingStore } from "@thingd/sdk/client";
const store = await HttpThingStore.open({
  url: "http://localhost:8757",
  authToken: "change-me",
});
```

## API

Full reference: [docs/api-spec/](https://github.com/sayanmohsin/thingd/tree/main/docs/api-spec)
