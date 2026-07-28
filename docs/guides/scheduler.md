# Scheduler

thingd has a built-in scheduler for running recurring tasks with persistence, observability, and automatic restart recovery.

## Quick start

```ts
import { ThingD } from "@thingd/sdk";

const db = await ThingD.open("./my-data");

// Create a schedule that runs every hour
await db.scheduler.schedule("hourly-cleanup", {
  expression: "0 * * * *", // cron: every hour at minute 0
  payload: { collection: "logs", olderThanDays: 30 },
  handler: async (schedule, context) => {
    const items = await db.listObjects(schedule.payload.collection, {
      filter: { createdAt: { $lt: new Date(Date.now() - 86400_000 * schedule.payload.olderThanDays).toISOString() } },
    });
    for (const item of items) {
      await db.delete(schedule.payload.collection, item.id);
    }
    context.log(`Cleaned up ${items.length} old log entries`);
  },
});

// Start the scheduler heartbeat
await db.scheduler.start();
```

Schedules persist across restarts. When `start()` is called, any overdue schedules run immediately.

## Schedule types

### Cron schedule

```ts
await db.scheduler.schedule("daily-report", {
  expression: "0 9 * * 1-5", // 9 AM UTC, weekdays
  timezone: "America/New_York",
  handler: async (schedule) => {
    await sendDailyReport(schedule.payload);
  },
});
```

### Interval schedule

```ts
await db.scheduler.scheduleInterval("health-check", {
  intervalMs: 300_000, // every 5 minutes
  handler: async (schedule, context) => {
    const res = await fetch("https://api.example.com/health");
    if (!res.ok) {
      context.fail(`Health check failed: ${res.status}`);
    }
  },
});
```

### One-time schedule

```ts
await db.scheduler.scheduleOnce("deploy-reminder", {
  runAt: "2026-08-01T09:00:00Z",
  payload: { message: "Deploy v2.0 ready" },
  handler: async (schedule) => {
    await notifyTeam(schedule.payload.message);
  },
});
```

## Managing schedules

```ts
// Pause/resume
await db.scheduler.pause("hourly-cleanup");
await db.scheduler.resume("hourly-cleanup");

// Manually trigger
await db.scheduler.run("hourly-cleanup");

// List all schedules
const all = await db.scheduler.list();

// Get stats
const stats = await db.scheduler.stats();
// { total: 3, enabled: 2, disabled: 1, running: 0, nextRun: { id: "health-check", at: "..." } }

// Delete
await db.scheduler.remove("hourly-cleanup");
```

## Events and monitoring

```ts
db.scheduler.on("started", (event) => {
  console.log(`Schedule ${event.scheduleId} started`);
});

db.scheduler.on("completed", (event) => {
  console.log(`Schedule ${event.scheduleId} completed in ${event.durationMs}ms`);
});

db.scheduler.on("failed", (event) => {
  console.error(`Schedule ${event.scheduleId} failed: ${event.error}`);
});

db.scheduler.on("disabled", (event) => {
  console.warn(`Schedule ${event.scheduleId} auto-disabled after consecutive failures`);
});
```

## Graceful shutdown

```ts
process.on("SIGTERM", async () => {
  await db.scheduler.stop(); // waits for running handlers to complete
  await db.close();
});
```

## How it works

1. **Schedules are stored** as objects in the `_schedules` collection
2. **A heartbeat loop** polls every 1 second for due schedules
3. **Handlers execute** with overlap protection (one run per schedule at a time)
4. **Stats update** after each run (runCount, failCount, lastStatus, lastDurationMs)
5. **Auto-disable** after `maxConsecutiveFails` consecutive failures (default: 5)
6. **Restart recovery** — on `start()`, overdue schedules run immediately

## MCP tools

The scheduler is also available via MCP:

| Tool | Description |
|------|-------------|
| `thing_scheduler_schedule` | Create a cron schedule |
| `thing_scheduler_schedule_interval` | Create an interval schedule |
| `thing_scheduler_schedule_once` | Create a one-time schedule |
| `thing_scheduler_list` | List all schedules |
| `thing_scheduler_get` | Get a single schedule |
| `thing_scheduler_stats` | Get scheduler statistics |
| `thing_scheduler_pause` | Pause a schedule |
| `thing_scheduler_resume` | Resume a schedule |
| `thing_scheduler_run` | Manually trigger a schedule |
| `thing_scheduler_remove` | Delete a schedule |

## Next steps

- [Scheduler API Reference](../api-spec/scheduler.md) — full type definitions and method signatures
- [Scheduler Recipes](./scheduler-recipes.md) — practical patterns for common use cases
