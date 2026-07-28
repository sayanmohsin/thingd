import type {
  Schedule,
  ScheduleContext,
  ScheduleEvent,
  ScheduleHandler,
  ScheduleIntervalOptions,
  ScheduleOnceOptions,
  ScheduleOptions,
  SchedulerEventType,
  SchedulerFacade,
  SchedulerListener,
  SchedulerStats,
  StoredMemoryObject,
  ThingStore,
} from "./types.js";

const SCHEDULES_COLLECTION = "_schedules";
const HEARTBEAT_INTERVAL_MS = 1_000;

function parseIntervalMs(expression: string): number | null {
  const match = expression.match(/^(\d+)(s|m|h|d)$/);
  if (!match?.[1] || !match[2]) {
    return null;
  }
  const n = Number.parseInt(match[1], 10);
  switch (match[2]) {
    case "s":
      return n * 1_000;
    case "m":
      return n * 60_000;
    case "h":
      return n * 3_600_000;
    case "d":
      return n * 86_400_000;
    default:
      return null;
  }
}

function computeNextRun(expression: string, _timezone?: string): Date {
  const intervalMs = parseIntervalMs(expression);
  if (intervalMs !== null) {
    return new Date(Date.now() + intervalMs);
  }
  // Handle "NNNms" style interval expressions
  const msMatch = expression.match(/^(\d+)ms$/);
  if (msMatch?.[1]) {
    return new Date(Date.now() + Number.parseInt(msMatch[1], 10));
  }
  return computeCronNext(expression);
}

function computeCronNext(expression: string): Date {
  const parts = expression.trim().split(/\s+/);
  if (parts.length < 5 || parts.length > 6) {
    throw new Error(`Invalid cron expression: "${expression}". Expected 5 or 6 fields.`);
  }

  const now = new Date();
  const next = new Date(now);
  next.setMilliseconds(0);
  next.setSeconds(0, 0);

  let minutePart: string;
  let hourPart: string;

  if (parts.length === 6) {
    next.setSeconds(Number.parseInt(parts[0] ?? "0", 10), 0);
    minutePart = parts[1] ?? "*";
    hourPart = parts[2] ?? "*";
  } else {
    minutePart = parts[0] ?? "*";
    hourPart = parts[1] ?? "*";
  }

  // Handle minute
  if (minutePart === "*") {
    // Keep current minute, advance if needed
  } else if (minutePart.includes("/")) {
    const step = Number.parseInt(minutePart.split("/")[1] ?? "1", 10);
    const currentMin = next.getMinutes();
    const nextMin = Math.ceil((currentMin + 1) / step) * step;
    if (nextMin >= 60) {
      next.setMinutes(0);
      next.setHours(next.getHours() + 1);
    } else {
      next.setMinutes(nextMin);
    }
  } else {
    next.setMinutes(Number.parseInt(minutePart, 10));
  }

  // Handle hour
  if (hourPart === "*") {
    // Keep current hour
  } else if (hourPart.includes("/")) {
    const step = Number.parseInt(hourPart.split("/")[1] ?? "1", 10);
    const currentHour = next.getHours();
    const nextHour = Math.ceil((currentHour + 1) / step) * step;
    if (nextHour >= 24) {
      next.setHours(0);
      next.setDate(next.getDate() + 1);
    } else {
      next.setHours(nextHour);
    }
  } else {
    next.setHours(Number.parseInt(hourPart, 10));
  }

  // If the computed time is in the past, advance by 1 minute
  if (next <= now) {
    next.setTime(next.getTime() + 60_000);
  }

  return next;
}

function scheduleToStored(schedule: Schedule): StoredMemoryObject {
  return {
    id: schedule.id,
    collection: SCHEDULES_COLLECTION,
    createdAt: schedule.createdAt,
    updatedAt: schedule.updatedAt,
    version: 0,
    expression: schedule.expression,
    timezone: schedule.timezone,
    payload: schedule.payload,
    enabled: schedule.enabled,
    nextRunAt: schedule.nextRunAt,
    lastRunAt: schedule.lastRunAt,
    lastStatus: schedule.lastStatus,
    lastError: schedule.lastError,
    lastDurationMs: schedule.lastDurationMs,
    runCount: schedule.runCount,
    failCount: schedule.failCount,
    consecutiveFails: schedule.consecutiveFails,
    maxConsecutiveFails: schedule.maxConsecutiveFails,
    metadata: schedule.metadata,
  };
}

function storedToSchedule(stored: StoredMemoryObject): Schedule {
  return {
    id: stored.id,
    expression: stored.expression as string,
    timezone: stored.timezone as string | undefined,
    payload: (stored.payload as Record<string, unknown>) ?? {},
    enabled: stored.enabled as boolean,
    nextRunAt: stored.nextRunAt as string,
    lastRunAt: stored.lastRunAt as string | undefined,
    lastStatus: stored.lastStatus as Schedule["lastStatus"],
    lastError: stored.lastError as string | undefined,
    lastDurationMs: stored.lastDurationMs as number | undefined,
    runCount: stored.runCount as number,
    failCount: stored.failCount as number,
    consecutiveFails: stored.consecutiveFails as number,
    maxConsecutiveFails: (stored.maxConsecutiveFails as number) ?? 5,
    createdAt: stored.createdAt,
    updatedAt: stored.updatedAt,
    metadata: stored.metadata as Record<string, unknown> | undefined,
  };
}

export class Scheduler implements SchedulerFacade {
  private handlers = new Map<string, ScheduleHandler>();
  private listeners = new Map<SchedulerEventType, Set<SchedulerListener>>();
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private running = new Set<string>();
  private started = false;

  constructor(private readonly store: ThingStore) {}

  async schedule(id: string, options: ScheduleOptions): Promise<Schedule> {
    if (!options.expression && !options.intervalMs) {
      throw new Error("Either expression or intervalMs is required");
    }
    const expression = options.expression ?? `${options.intervalMs}ms`;
    const now = new Date().toISOString();
    const intervalMs = options.intervalMs ?? parseIntervalMs(expression);
    const nextRunAt =
      intervalMs !== null
        ? new Date(Date.now() + intervalMs).toISOString()
        : computeNextRun(expression, options.timezone).toISOString();

    const schedule: Schedule = {
      id,
      expression,
      timezone: options.timezone,
      payload: options.payload ?? {},
      enabled: options.enabled ?? true,
      nextRunAt,
      runCount: 0,
      failCount: 0,
      consecutiveFails: 0,
      maxConsecutiveFails: options.maxConsecutiveFails ?? 5,
      createdAt: now,
      updatedAt: now,
      metadata: options.metadata,
    };

    this.handlers.set(id, options.handler);
    await this.store.put(SCHEDULES_COLLECTION, scheduleToStored(schedule));
    return schedule;
  }

  async scheduleOnce(id: string, options: ScheduleOnceOptions): Promise<Schedule> {
    const now = new Date().toISOString();
    const schedule: Schedule = {
      id,
      expression: `once:${options.runAt}`,
      payload: options.payload ?? {},
      enabled: true,
      nextRunAt: options.runAt,
      runCount: 0,
      failCount: 0,
      consecutiveFails: 0,
      maxConsecutiveFails: 1,
      createdAt: now,
      updatedAt: now,
      metadata: options.metadata,
    };

    this.handlers.set(id, options.handler);
    await this.store.put(SCHEDULES_COLLECTION, scheduleToStored(schedule));
    return schedule;
  }

  async scheduleInterval(id: string, options: ScheduleIntervalOptions): Promise<Schedule> {
    const now = new Date().toISOString();
    const nextRunAt = new Date(Date.now() + options.intervalMs).toISOString();
    const schedule: Schedule = {
      id,
      expression: `${options.intervalMs}ms`,
      payload: options.payload ?? {},
      enabled: options.enabled ?? true,
      nextRunAt,
      runCount: 0,
      failCount: 0,
      consecutiveFails: 0,
      maxConsecutiveFails: options.maxConsecutiveFails ?? 5,
      createdAt: now,
      updatedAt: now,
      metadata: options.metadata,
    };

    this.handlers.set(id, options.handler);
    await this.store.put(SCHEDULES_COLLECTION, scheduleToStored(schedule));
    return schedule;
  }

  async get(id: string): Promise<Schedule | null> {
    const stored = await this.store.get<StoredMemoryObject>(SCHEDULES_COLLECTION, id);
    return stored ? storedToSchedule(stored) : null;
  }

  async list(): Promise<Schedule[]> {
    const stored = await this.store.listObjects<StoredMemoryObject>(SCHEDULES_COLLECTION);
    return stored.map(storedToSchedule);
  }

  async pause(id: string): Promise<Schedule> {
    const schedule = await this.get(id);
    if (!schedule) {
      throw new Error(`Schedule "${id}" not found`);
    }
    schedule.enabled = false;
    schedule.updatedAt = new Date().toISOString();
    await this.store.put(SCHEDULES_COLLECTION, scheduleToStored(schedule));
    return schedule;
  }

  async resume(id: string): Promise<Schedule> {
    const schedule = await this.get(id);
    if (!schedule) {
      throw new Error(`Schedule "${id}" not found`);
    }
    schedule.enabled = true;
    schedule.nextRunAt = computeNextRun(schedule.expression, schedule.timezone).toISOString();
    schedule.updatedAt = new Date().toISOString();
    await this.store.put(SCHEDULES_COLLECTION, scheduleToStored(schedule));
    return schedule;
  }

  async remove(id: string): Promise<boolean> {
    const result = await this.store.delete(SCHEDULES_COLLECTION, id);
    this.handlers.delete(id);
    return result.deleted;
  }

  async run(id: string): Promise<void> {
    const schedule = await this.get(id);
    if (!schedule) {
      throw new Error(`Schedule "${id}" not found`);
    }
    const handler = this.handlers.get(id);
    if (!handler) {
      throw new Error(`No handler registered for schedule "${id}"`);
    }
    await this.executeSchedule(schedule, handler);
  }

  async stats(): Promise<SchedulerStats> {
    const all = await this.list();
    const enabled = all.filter((s) => s.enabled);
    const disabled = all.filter((s) => !s.enabled);
    const now = Date.now();
    const future = enabled
      .filter((s) => new Date(s.nextRunAt).getTime() > now)
      .sort((a, b) => new Date(a.nextRunAt).getTime() - new Date(b.nextRunAt).getTime());

    return {
      total: all.length,
      enabled: enabled.length,
      disabled: disabled.length,
      running: this.running.size,
      nextRun:
        future.length > 0 && future[0] ? { id: future[0].id, at: future[0].nextRunAt } : null,
    };
  }

  async start(): Promise<void> {
    if (this.started) {
      return;
    }
    this.started = true;

    const schedules = await this.list();
    for (const s of schedules) {
      if (s.enabled && s.expression.startsWith("once:")) {
        const runAt = new Date(s.expression.replace("once:", ""));
        if (runAt.getTime() <= Date.now()) {
          const handler = this.handlers.get(s.id);
          if (handler) {
            this.executeSchedule(s, handler);
          }
        }
      }
    }

    this.heartbeatTimer = setInterval(() => {
      this.heartbeat();
    }, HEARTBEAT_INTERVAL_MS);
  }

  async stop(): Promise<void> {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
    this.started = false;

    const maxWait = 10_000;
    const start = Date.now();
    while (this.running.size > 0 && Date.now() - start < maxWait) {
      await new Promise((r) => setTimeout(r, 100));
    }
  }

  on(event: SchedulerEventType, listener: SchedulerListener): void {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, new Set());
    }
    this.listeners.get(event)?.add(listener);
  }

  off(event: SchedulerEventType, listener: SchedulerListener): void {
    this.listeners.get(event)?.delete(listener);
  }

  private emit(event: ScheduleEvent): void {
    const listeners = this.listeners.get(event.status);
    if (listeners) {
      for (const fn of listeners) {
        fn(event);
      }
    }
  }

  private async heartbeat(): Promise<void> {
    try {
      const schedules = await this.list();
      const now = Date.now();

      for (const schedule of schedules) {
        if (!schedule.enabled || this.running.has(schedule.id)) {
          continue;
        }

        const nextRunTime = new Date(schedule.nextRunAt).getTime();
        if (nextRunTime <= now) {
          const handler = this.handlers.get(schedule.id);
          if (handler) {
            this.executeSchedule(schedule, handler);
          }
        }
      }
    } catch {
      // Heartbeat errors are non-fatal
    }
  }

  private async executeSchedule(schedule: Schedule, handler: ScheduleHandler): Promise<void> {
    if (this.running.has(schedule.id)) {
      return;
    }
    this.running.add(schedule.id);

    const startMs = Date.now();
    let failed = false;
    let errorMsg: string | undefined;

    const context: ScheduleContext = {
      log: () => {},
      fail: (error: string) => {
        failed = true;
        errorMsg = error;
      },
    };

    this.emit({
      scheduleId: schedule.id,
      expression: schedule.expression,
      status: "started",
      timestamp: new Date().toISOString(),
      runCount: schedule.runCount,
      failCount: schedule.failCount,
    });

    try {
      await handler(schedule, context);
    } catch (err) {
      failed = true;
      errorMsg = err instanceof Error ? err.message : String(err);
    }

    const durationMs = Date.now() - startMs;
    const updated = await this.get(schedule.id);
    if (!updated) {
      this.running.delete(schedule.id);
      return;
    }

    if (failed) {
      updated.failCount += 1;
      updated.consecutiveFails += 1;
      updated.lastStatus = "failed";
      updated.lastError = errorMsg;
      updated.lastRunAt = new Date().toISOString();
      updated.lastDurationMs = durationMs;
      updated.updatedAt = new Date().toISOString();

      if (updated.consecutiveFails >= updated.maxConsecutiveFails) {
        updated.enabled = false;
        this.emit({
          scheduleId: updated.id,
          expression: updated.expression,
          status: "disabled",
          timestamp: new Date().toISOString(),
          durationMs,
          error: errorMsg,
          runCount: updated.runCount,
          failCount: updated.failCount,
        });
      }

      this.emit({
        scheduleId: updated.id,
        expression: updated.expression,
        status: "failed",
        timestamp: new Date().toISOString(),
        durationMs,
        error: errorMsg,
        runCount: updated.runCount,
        failCount: updated.failCount,
      });
    } else {
      updated.runCount += 1;
      updated.consecutiveFails = 0;
      updated.lastStatus = "completed";
      updated.lastError = undefined;
      updated.lastRunAt = new Date().toISOString();
      updated.lastDurationMs = durationMs;
      updated.updatedAt = new Date().toISOString();

      this.emit({
        scheduleId: updated.id,
        expression: updated.expression,
        status: "completed",
        timestamp: new Date().toISOString(),
        durationMs,
        runCount: updated.runCount,
        failCount: updated.failCount,
      });
    }

    if (updated.enabled) {
      if (updated.expression.startsWith("once:")) {
        updated.enabled = false;
      } else {
        updated.nextRunAt = computeNextRun(updated.expression, updated.timezone).toISOString();
      }
    }

    await this.store.put(SCHEDULES_COLLECTION, scheduleToStored(updated));
    this.running.delete(schedule.id);
  }
}
