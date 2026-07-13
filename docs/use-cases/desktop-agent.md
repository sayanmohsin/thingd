---
title: "Frontend-Less Desktop Agent"
description: "A task manager with zero UI — your agent manages everything through MCP tools, with queue-based reminders."
tags: ["desktop", "MCP", "queues", "agents"]
icon: "bot"
repo: "https://github.com/sayanmohsin/desktop-agent"
---

## The problem

Every task manager I tried requires opening an app, clicking buttons, filling
forms. I wanted to manage tasks the way I work: through my terminal and AI
agent. Tell the agent "remind me about X in 2 hours" and have it just work.

## How desktop-agent uses thingd

This is an MCP server that exposes task management tools. The "UI" is the
agent conversation itself. thingd provides persistence (objects), scheduling
(queues with delay), and retrieval (search).

```
┌──────────────┐  MCP tools   ┌──────────┐
│   Agent      │ ────────────→│  thingd  │
│ (Claude, etc)│ ←────────────│  server  │
└──────────────┘              └────┬─────┘
                                   │
                        ┌──────────┴──────────┐
                        │  Objects (tasks)     │
                        │  Queues (reminders)  │
                        │  Search (find tasks) │
                        └─────────────────────┘
```

### Creating tasks

```typescript
server.tool("task_create", {
  title: z.string(),
  description: z.string().optional(),
  dueAt: z.string().optional(),
}, async ({ title, description, dueAt }) => {
  const task = {
    id: crypto.randomUUID(),
    title,
    description: description ?? "",
    status: "pending",
    createdAt: new Date().toISOString(),
    dueAt: dueAt ?? null,
  };
  await db.put("tasks", task);

  // Schedule a reminder if due date is set
  if (dueAt) {
    const delayMs = new Date(dueAt).getTime() - Date.now();
    if (delayMs > 0) {
      await db.queue("reminders").push(
        { taskId: task.id, title },
        { delayMs, idempotencyKey: `reminder:${task.id}` }
      );
    }
  }

  return { content: [{ type: "text", text: JSON.stringify(task) }] };
});
```

### Listing and searching tasks

```typescript
server.tool("task_list", { status: z.string().optional() },
  async ({ status }) => {
    const options = status ? { filter: { status } } : {};
    const tasks = await db.listObjects("tasks", {
      ...options,
      sortBy: { field: "created_at", direction: "desc" },
    });
    return { content: [{ type: "text", text: JSON.stringify(tasks) }] };
  }
);

server.tool("task_search", { query: z.string() },
  async ({ query }) => {
    const results = await db.search(query, { collections: ["tasks"] });
    return { content: [{ type: "text", text: JSON.stringify(results) }] };
  }
);
```

### Queue-based reminders

The reminder queue uses thingd's **delayed jobs**. When a task's due time
arrives, the job becomes available and a reminder tool fires.

```typescript
async function processReminders() {
  const job = await db.queue("reminders").claim({ leaseMs: 10_000 });
  if (job) {
    // Notify the user (desktop notification, webhook, etc.)
    await notify(`Reminder: ${job.payload.title}`);
    await db.queue("reminders").ack(job.id);
  }
}
```

## Why thingd?

This replaces what would normally need a database (tasks), a scheduler
(reminders), and an API layer. thingd bundles all three with the same
interface. No Redis, no cron, no frontend server.

The full project is at [github.com/sayanmohsin/desktop-agent](https://github.com/sayanmohsin/desktop-agent).
