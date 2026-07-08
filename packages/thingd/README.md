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
// Lightweight HTTP client (browser + Node.js + Bun, zero dependencies)
import { ThingD } from "@thingd/sdk/client";

// Pure in-memory store (browser + Node.js + Bun, zero dependencies)
import { ThingD } from "@thingd/sdk/memory";

// Types only (for type-safe dependency injection)
import type { ThingDConnection } from "@thingd/sdk/types";
```

## Bun + Hono

thingd's HTTP client (`@thingd/sdk/client`) uses only web-standard `fetch()` — it runs in **Bun**, Deno, Cloudflare Workers, and browsers. No Node.js dependencies.

Connect to the thingd sidecar (Rust binary) over HTTP:

```ts
import { ThingD } from "@thingd/sdk/client";

const db = await ThingD.open({
  driver: "cloud",
  databaseUrl: "http://localhost:8757",
});
```

Full example: [`examples/bun-hono/`](https://github.com/sayanmohsin/thingd/tree/main/examples/bun-hono)
Guide: [`docs/bun-hono.md`](https://github.com/sayanmohsin/thingd/tree/main/docs/bun-hono.md)

## API

Full reference: [docs/api-spec/](https://github.com/sayanmohsin/thingd/tree/main/docs/api-spec)
