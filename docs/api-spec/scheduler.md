# Scheduler API Reference

## Types

### Schedule

```ts
type Schedule = {
  id: string;                          // user-provided or auto-generated
  expression: string;                  // cron expression, interval (e.g. "5m"), or "once:<ISO>"
  timezone?: string;                   // IANA timezone (default: UTC)
  payload: Record<string, unknown>;    // data passed to the handler
  enabled: boolean;                    // pause/resume toggle
  nextRunAt: string;                   // ISO timestamp — next execution time
  lastRunAt?: string;                  // ISO timestamp — last execution start
  lastStatus?: "completed" | "failed" | "running";
  lastError?: string;                  // error message if last run failed
  lastDurationMs?: number;             // how long the last run took
  runCount: number;                    // total successful runs
  failCount: number;                   // total failed runs
  consecutiveFails: number;            // consecutive failures (reset on success)
  maxConsecutiveFails: number;         // auto-disable after N consecutive fails (default: 5)
  createdAt: string;                   // ISO timestamp
  updatedAt: string;                   // ISO timestamp
  metadata?: Record<string, unknown>;  // arbitrary user data
};
```

### ScheduleOptions

```ts
type ScheduleOptions = {
  expression?: string;                 // cron expression (5 or 6 fields)
  intervalMs?: number;                 // alternative: fixed interval in ms
  timezone?: string;                   // IANA timezone
  payload?: Record<string, unknown>;
  enabled?: boolean;                   // default: true
  maxConsecutiveFails?: number;        // default: 5
  metadata?: Record<string, unknown>;
  handler: ScheduleHandler;            // required: function to execute
};
```

### ScheduleOnceOptions

```ts
type ScheduleOnceOptions = {
  runAt: string;                       // ISO timestamp
  payload?: Record<string, unknown>;
  metadata?: Record<string, unknown>;
  handler: ScheduleHandler;
};
```

### ScheduleIntervalOptions

```ts
type ScheduleIntervalOptions = {
  intervalMs: number;                  // delay between runs in ms
  payload?: Record<string, unknown>;
  enabled?: boolean;                   // default: true
  maxConsecutiveFails?: number;        // default: 5
  metadata?: Record<string, unknown>;
  handler: ScheduleHandler;
};
```

### ScheduleHandler

```ts
type ScheduleHandler = (
  schedule: Schedule,
  context: ScheduleContext,
) => Promise<void>;
```

### ScheduleContext

```ts
type ScheduleContext = {
  log: (message: string) => void;      // log a message (for debugging)
  fail: (error: string) => void;       // mark the run as failed
};
```

### ScheduleEvent

```ts
type ScheduleEvent = {
  scheduleId: string;
  expression: string;
  status: "started" | "completed" | "failed" | "disabled";
  timestamp: string;                   // ISO
  durationMs?: number;
  error?: string;
  runCount: number;
  failCount: number;
};
```

### SchedulerStats

```ts
type SchedulerStats = {
  total: number;                       // all schedules
  enabled: number;                     // active schedules
  disabled: number;                    // paused schedules
  running: number;                     // currently executing
  nextRun: { id: string; at: string } | null;  // soonest upcoming run
};
```

## Methods

### `scheduler.schedule(id, options)`

Create or update a recurring schedule with a cron expression or interval.

```ts
await db.scheduler.schedule("my-schedule", {
  expression: "0 * * * *",
  payload: { key: "value" },
  handler: async (schedule, context) => {
    context.log("Running...");
  },
});
```

Returns: `Promise<Schedule>`

### `scheduler.scheduleInterval(id, options)`

Create a schedule that runs at a fixed interval.

```ts
await db.scheduler.scheduleInterval("health-check", {
  intervalMs: 300_000,
  handler: async () => { /* ... */ },
});
```

Returns: `Promise<Schedule>`

### `scheduler.scheduleOnce(id, options)`

Create a one-time schedule. Auto-disables after execution.

```ts
await db.scheduler.scheduleOnce("reminder", {
  runAt: "2026-08-01T09:00:00Z",
  handler: async () => { /* ... */ },
});
```

Returns: `Promise<Schedule>`

### `scheduler.get(id)`

Get a single schedule by ID. Returns `null` if not found.

```ts
const schedule = await db.scheduler.get("my-schedule");
```

Returns: `Promise<Schedule | null>`

### `scheduler.list()`

List all registered schedules.

```ts
const all = await db.scheduler.list();
```

Returns: `Promise<Schedule[]>`

### `scheduler.pause(id)`

Pause a schedule. It remains stored but stops triggering.

```ts
await db.scheduler.pause("my-schedule");
```

Returns: `Promise<Schedule>`

### `scheduler.resume(id)`

Resume a paused schedule. Recalculates `nextRunAt` from now.

```ts
await db.scheduler.resume("my-schedule");
```

Returns: `Promise<Schedule>`

### `scheduler.remove(id)`

Permanently delete a schedule and its handler.

```ts
await db.scheduler.remove("my-schedule");
```

Returns: `Promise<boolean>`

### `scheduler.run(id)`

Immediately execute a schedule's handler. Useful for testing.

```ts
await db.scheduler.run("my-schedule");
```

Returns: `Promise<void>`

### `scheduler.stats()`

Get aggregate scheduler statistics.

```ts
const stats = await db.scheduler.stats();
```

Returns: `Promise<SchedulerStats>`

### `scheduler.start()`

Start the heartbeat loop. Begins polling for due schedules.

```ts
await db.scheduler.start();
```

Returns: `Promise<void>`

### `scheduler.stop()`

Stop the heartbeat loop. Waits for running handlers to complete (up to 10s).

```ts
await db.scheduler.stop();
```

Returns: `Promise<void>`

### `scheduler.on(event, listener)`

Subscribe to lifecycle events.

```ts
db.scheduler.on("completed", (event) => {
  console.log(`${event.scheduleId} completed in ${event.durationMs}ms`);
});
```

Returns: `void`

### `scheduler.off(event, listener)`

Unsubscribe from lifecycle events.

```ts
db.scheduler.off("completed", myListener);
```

Returns: `void`

## MCP Tools

| Tool | Parameters | Description |
|------|-----------|-------------|
| `thing_scheduler_schedule` | `id`, `expression`, `timezone?`, `payload?`, `enabled?`, `maxConsecutiveFails?` | Create a cron schedule |
| `thing_scheduler_schedule_interval` | `id`, `intervalMs`, `payload?`, `enabled?`, `maxConsecutiveFails?` | Create an interval schedule |
| `thing_scheduler_schedule_once` | `id`, `runAt`, `payload?` | Create a one-time schedule |
| `thing_scheduler_list` | _(none)_ | List all schedules |
| `thing_scheduler_get` | `id` | Get a single schedule |
| `thing_scheduler_stats` | _(none)_ | Get scheduler statistics |
| `thing_scheduler_pause` | `id` | Pause a schedule |
| `thing_scheduler_resume` | `id` | Resume a schedule |
| `thing_scheduler_run` | `id` | Manually trigger a schedule |
| `thing_scheduler_remove` | `id` | Delete a schedule |
