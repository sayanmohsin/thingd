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
