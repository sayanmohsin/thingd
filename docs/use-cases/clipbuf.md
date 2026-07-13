---
title: "Clipboard History Over MCP"
description: "Search your entire clipboard history through MCP tools, backed by thingd objects and full-text search."
tags: ["clipboard", "MCP", "objects", "search"]
icon: "clipboard"
repo: "https://github.com/sayanmohsin/clipbuf"
---

## The problem

I copy a lot of things during the day — code snippets, URLs, terminal output,
important emails. And I lose most of them to the next copy.

The system clipboard holds one item. Clipboard managers exist but they're
either proprietary, don't have search, or don't expose their data to agents.

## How clipbuf uses thingd

clipbuf is a lightweight MCP server that sits in your system tray. Every time
you copy something, it stores the text in thingd. When your agent needs to
find something you copied earlier, it calls `clip_search`.

```
┌─────────────┐   clipboard.write   ┌──────────┐
│  clipbuf    │ ──────────────────→ │  thingd  │
│  MCP server │ ←────────────────── │  search  │
└─────────────┘   clip_search       └──────────┘
```

### Storing clipboard items

thingd objects work here because clipboard items are unstructured text with
timestamps — no schema needed.

```typescript
server.tool("clip_write", { text: z.string(), source: z.string().optional() },
  async ({ text, source }) => {
    await db.put("clipboard", {
      id: crypto.randomUUID(),
      text,
      source: source ?? "unknown",
      copiedAt: new Date().toISOString(),
    });
    return { content: [{ type: "text", text: "stored" }] };
  }
);
```

### Searching clipboard history

thingd's full-text search (FTS5 with BM25 ranking) makes this useful —
find that URL you copied yesterday, or that error message from last week.

```typescript
server.tool("clip_search", { query: z.string(), limit: z.number().optional() },
  async ({ query, limit }) => {
    const results = await db.search(query, {
      collections: ["clipboard"],
      limit: limit ?? 10,
    });
    return { content: [{ type: "text", text: JSON.stringify(results) }] };
  }
);
```

### Listing recent clips

```typescript
server.tool("clip_recent", { limit: z.number().optional() },
  async ({ limit }) => {
    const items = await db.listObjects("clipboard", {
      sortBy: { field: "created_at", direction: "desc" },
      limit: limit ?? 20,
    });
    return { content: [{ type: "text", text: JSON.stringify(items) }] };
  }
);
```

## Why thingd?

The alternative would be keeping clipboard items in memory or a JSON file.
thingd gives you search, persistence across restarts, and MCP access — all
with zero infrastructure.

The full project is at [github.com/sayanmohsin/clipbuf](https://github.com/sayanmohsin/clipbuf).
