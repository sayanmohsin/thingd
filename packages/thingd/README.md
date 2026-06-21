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
// Lightweight HTTP client (browser + Node.js, zero dependencies)
import { ThingD } from "@thingd/sdk/client";

// Pure in-memory store (browser + Node.js, zero dependencies)
import { ThingD } from "@thingd/sdk/memory";

// Types only (for type-safe dependency injection)
import type { ThingDConnection } from "@thingd/sdk/types";
```

## API

Full reference: [docs/api-spec/](https://github.com/sayanmohsin/thingd/tree/main/docs/api-spec)
