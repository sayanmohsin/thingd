# Scheduler Recipes

Practical patterns for common scheduling use cases.

## Database cleanup

Remove old records on a schedule:

```ts
await db.scheduler.schedule("cleanup-logs", {
  expression: "0 2 * * *", // daily at 2 AM
  payload: { collection: "logs", olderThanDays: 30 },
  handler: async (schedule, context) => {
    const cutoff = new Date(Date.now() - 86_400_000 * schedule.payload.olderThanDays).toISOString();
    const old = await db.listObjects(schedule.payload.collection, {
      filter: { createdAt: { $lt: cutoff } },
    });
    if (old.length > 0) {
      await db.deleteBatch(schedule.payload.collection, old.map((o) => o.id));
    }
    context.log(`Cleaned up ${old.length} old records`);
  },
});
```

## Health check with alerting

Monitor a service and alert on failure:

```ts
await db.scheduler.scheduleInterval("api-health", {
  intervalMs: 60_000, // every minute
  payload: { url: "https://api.example.com/health" },
  handler: async (schedule, context) => {
    try {
      const res = await fetch(schedule.payload.url);
      if (!res.ok) {
        context.fail(`HTTP ${res.status}`);
      }
    } catch (err) {
      context.fail(err instanceof Error ? err.message : "Network error");
    }
  },
});

db.scheduler.on("failed", async (event) => {
  if (event.scheduleId === "api-health") {
    await sendAlert(`API health check failed: ${event.error}`);
  }
});
```

## Auto-disable on repeated failures

The scheduler auto-disables after `maxConsecutiveFails`:

```ts
await db.scheduler.schedule("flaky-sync", {
  expression: "*/5 * * * *",
  maxConsecutiveFails: 3, // disable after 3 consecutive failures
  handler: async () => {
    await syncExternalData(); // might fail
  },
});

db.scheduler.on("disabled", async (event) => {
  await notifyAdmin(`Schedule "${event.scheduleId}" was auto-disabled`);
});
```

## Usage metering reset

Reset counters for billing periods:

```ts
await db.scheduler.schedule("reset-hourly-meter", {
  expression: "0 * * * *", // every hour
  payload: { period: "hourly" },
  handler: async (schedule, context) => {
    const users = await db.listObjects("users");
    for (const user of users) {
      await db.put("usage", {
        id: `${user.id}:hourly`,
        userId: user.id,
        period: "hourly",
        calls: 0,
        resetAt: new Date().toISOString(),
      });
    }
    context.log(`Reset meter for ${users.length} users`);
  },
});
```

## Data sync from external API

Keep local data fresh:

```ts
await db.scheduler.schedule("sync-products", {
  expression: "*/15 * * * *", // every 15 minutes
  handler: async (schedule, context) => {
    const res = await fetch("https://api.store.com/products");
    const products = await res.json();
    await db.putBatch(
      "products",
      products.map((p) => ({ id: p.sku, ...p }))
    );
    context.log(`Synced ${products.length} products`);
  },
});
```

## Report generation

Generate and store reports on a schedule:

```ts
await db.scheduler.schedule("weekly-report", {
  expression: "0 9 * * 1", // Monday at 9 AM
  timezone: "America/New_York",
  payload: { recipients: ["team@company.com"] },
  handler: async (schedule, context) => {
    const stats = await db.aggregate.count("orders", {
      filter: { createdAt: { $gte: lastWeek() } },
    });
    const report = await generateReport(stats);
    await db.put("reports", { id: `weekly-${Date.now()}`, ...report });
    await sendEmail(schedule.payload.recipients, report);
    context.log(`Weekly report sent to ${schedule.payload.recipients.length} recipients`);
  },
});
```

## Manual trigger from API

Expose a manual trigger endpoint:

```ts
// Express/Fastify route
app.post("/api/scheduler/:id/trigger", async (req, res) => {
  await db.scheduler.run(req.params.id);
  res.json({ triggered: true, id: req.params.id });
});
```

## Graceful shutdown pattern

```ts
const db = await ThingD.open("./my-data");

// Register schedules
await db.scheduler.schedule("task1", { expression: "*/5 * * * *", handler: async () => {} });
await db.scheduler.start();

// Graceful shutdown
async function shutdown() {
  console.log("Shutting down scheduler...");
  await db.scheduler.stop(); // waits for running handlers
  await db.close();
  process.exit(0);
}

process.on("SIGTERM", shutdown);
process.on("SIGINT", shutdown);
```
