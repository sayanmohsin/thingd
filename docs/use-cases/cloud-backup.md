---
title: "Cross-Device Sync with thingd.cloud"
description: "Sync bookmarks, notes, and configuration across devices using thingd.cloud REST API."
tags: ["cloud", "sync", "REST", "cross-device"]
icon: "cloud"
repo: "https://github.com/sayanmohsin/cloud-backup"
---

## The problem

I use three machines: a desktop, a laptop, and occasionally my phone.
Keeping bookmarks, notes, and config in sync across them is tedious.
Most sync solutions require installing an app on every device, or they're
tied to a specific browser.

I wanted something that works with any device that can make an HTTP request.

## How cloud-backup uses thingd.cloud

cloud-backup is a CLI tool and a set of scripts that use the thingd.cloud
REST API to synchronize data across devices. Each device authenticates
with an API key and reads/writes to the same collection.

```
┌──────────┐          ┌────────────────┐          ┌──────────┐
│ Desktop  │ ──PUT──→│  thingd.cloud  │ ←──GET──│  Laptop  │
│ (imports │         │   REST API     │         │ (reads)  │
│  bookmarks)│       │  /v1/objects/  │         └──────────┘
└──────────┘          └────────────────┘
                      │
                      └──GET──┐
                              ▼
                        ┌──────────┐
                        │  Phone   │
                        │ (browser)│
                        └──────────┘
```

### Importing browser bookmarks

```typescript
import { ThingdClient } from "@thingd/client";

const client = new ThingdClient({
  url: "https://api.thingd.cloud",
  authToken: process.env.THINGD_CLOUD_API_KEY,
});

async function importBookmarks(filePath: string) {
  const bookmarks = parseChromeBookmarks(filePath);
  let count = 0;
  for (const bm of bookmarks) {
    await client.put("bookmarks", {
      id: `bm-${hashUrl(bm.url)}`,
      url: bm.url,
      title: bm.title,
      folder: bm.folder,
      importedAt: new Date().toISOString(),
    });
    count++;
  }
  console.log(`Imported ${count} bookmarks`);
}
```

### Searching across devices

The `@thingd/client` package works from any JavaScript runtime — Node.js,
browsers, Deno, Bun.

```typescript
// From any device:
const results = await client.search("typescript", {
  collections: ["bookmarks"],
});

// Returns bookmarks from all devices
```

### Syncing notes

```typescript
// Write from desktop:
await client.put("notes", {
  id: "note-project-plan",
  text: "Q3 planning notes...",
  updatedAt: new Date().toISOString(),
});

// Read from phone browser:
const note = await client.get("notes", "note-project-plan");
```

## Why thingd.cloud?

The REST API is just HTTP — any device with `fetch()` can participate.
thingd.cloud handles auth, persistence, and search. You don't need to
run a server, set up a database, or manage sync logic.

The full project is at [github.com/sayanmohsin/cloud-backup](https://github.com/sayanmohsin/cloud-backup).
