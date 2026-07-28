import assert from "node:assert/strict";
import test from "node:test";
import { ThingD } from "../dist/index.js";

runSchedulerSuite("memory", () => ThingD.open(":memory:"));

function runSchedulerSuite(label, openDb) {
  test(`${label}: schedule creates a cron schedule`, async () => {
    const db = await openDb();
    let called = false;

    const schedule = await db.scheduler.schedule("test-cron", {
      expression: "0 * * * *",
      payload: { key: "value" },
      handler: async () => {
        called = true;
      },
    });

    assert.equal(schedule.id, "test-cron");
    assert.equal(schedule.expression, "0 * * * *");
    assert.equal(schedule.enabled, true);
    assert.equal(schedule.runCount, 0);
    assert.equal(schedule.failCount, 0);
    assert.ok(schedule.nextRunAt);
    assert.ok(schedule.createdAt);
    assert.equal(called, false);
    await db.close();
  });

  test(`${label}: scheduleOnce creates a one-time schedule`, async () => {
    const db = await openDb();

    const schedule = await db.scheduler.scheduleOnce("test-once", {
      runAt: new Date(Date.now() + 60_000).toISOString(),
      payload: { msg: "hello" },
      handler: async () => {},
    });

    assert.equal(schedule.id, "test-once");
    assert.equal(schedule.enabled, true);
    assert.equal(schedule.maxConsecutiveFails, 1);
    assert.ok(schedule.expression.startsWith("once:"));
    await db.close();
  });

  test(`${label}: scheduleInterval creates interval schedule`, async () => {
    const db = await openDb();

    const schedule = await db.scheduler.scheduleInterval("test-interval", {
      intervalMs: 5_000,
      payload: { check: true },
      handler: async () => {},
    });

    assert.equal(schedule.id, "test-interval");
    assert.equal(schedule.expression, "5000ms");
    assert.equal(schedule.enabled, true);
    assert.ok(schedule.nextRunAt);
    await db.close();
  });

  test(`${label}: get returns schedule by id`, async () => {
    const db = await openDb();
    await db.scheduler.schedule("get-test", {
      expression: "*/5 * * * *",
      handler: async () => {},
    });

    const found = await db.scheduler.get("get-test");
    assert.ok(found);
    assert.equal(found.id, "get-test");

    const notFound = await db.scheduler.get("nonexistent");
    assert.equal(notFound, null);
    await db.close();
  });

  test(`${label}: list returns all schedules`, async () => {
    const db = await openDb();
    await db.scheduler.schedule("list-a", { expression: "0 * * * *", handler: async () => {} });
    await db.scheduler.schedule("list-b", { expression: "*/10 * * * *", handler: async () => {} });

    const all = await db.scheduler.list();
    assert.ok(all.length >= 2);
    const ids = all.map((s) => s.id);
    assert.ok(ids.includes("list-a"));
    assert.ok(ids.includes("list-b"));
    await db.close();
  });

  test(`${label}: pause disables schedule`, async () => {
    const db = await openDb();
    await db.scheduler.schedule("pause-test", {
      expression: "0 * * * *",
      handler: async () => {},
    });

    const paused = await db.scheduler.pause("pause-test");
    assert.equal(paused.enabled, false);

    const found = await db.scheduler.get("pause-test");
    assert.equal(found?.enabled, false);
    await db.close();
  });

  test(`${label}: resume re-enables schedule and recalculates nextRun`, async () => {
    const db = await openDb();
    const before = await db.scheduler.schedule("resume-test", {
      expression: "0 * * * *",
      handler: async () => {},
    });

    await db.scheduler.pause("resume-test");
    const after = await db.scheduler.resume("resume-test");
    assert.equal(after.enabled, true);
    assert.ok(new Date(after.nextRunAt).getTime() > new Date(before.nextRunAt).getTime() - 60_000);
    await db.close();
  });

  test(`${label}: remove deletes schedule`, async () => {
    const db = await openDb();
    await db.scheduler.schedule("remove-test", {
      expression: "0 * * * *",
      handler: async () => {},
    });

    const deleted = await db.scheduler.remove("remove-test");
    assert.equal(deleted, true);

    const found = await db.scheduler.get("remove-test");
    assert.equal(found, null);
    await db.close();
  });

  test(`${label}: stats returns correct counts`, async () => {
    const db = await openDb();
    await db.scheduler.schedule("stats-a", { expression: "0 * * * *", handler: async () => {} });
    await db.scheduler.schedule("stats-b", {
      expression: "0 * * * *",
      enabled: false,
      handler: async () => {},
    });

    const stats = await db.scheduler.stats();
    assert.ok(stats.total >= 2);
    assert.ok(stats.enabled >= 1);
    assert.ok(stats.disabled >= 1);
    assert.equal(stats.running, 0);
    await db.close();
  });

  test(`${label}: on/off event listeners`, async () => {
    const db = await openDb();
    const events = [];

    const listener = (event) => events.push(event.status);
    db.scheduler.on("completed", listener);

    await db.scheduler.schedule("event-test", {
      intervalMs: 100,
      handler: async () => {},
    });
    await db.scheduler.start();

    await new Promise((r) => setTimeout(r, 1_500));
    await db.scheduler.stop();

    db.scheduler.off("completed", listener);
    assert.ok(events.includes("completed"));
    await db.close();
  });

  test(`${label}: handler execution updates stats`, async () => {
    const db = await openDb();
    let count = 0;

    await db.scheduler.schedule("exec-test", {
      intervalMs: 100,
      handler: async () => {
        count++;
      },
    });

    await db.scheduler.run("exec-test");
    assert.equal(count, 1);

    const schedule = await db.scheduler.get("exec-test");
    assert.equal(schedule?.runCount, 1);
    assert.equal(schedule?.lastStatus, "completed");
    assert.ok(schedule?.lastRunAt);
    assert.ok(schedule?.lastDurationMs !== undefined);
    await db.close();
  });

  test(`${label}: handler failure updates fail stats`, async () => {
    const db = await openDb();

    await db.scheduler.schedule("fail-test", {
      intervalMs: 100,
      maxConsecutiveFails: 3,
      handler: async () => {
        throw new Error("test error");
      },
    });

    await db.scheduler.run("fail-test");

    const schedule = await db.scheduler.get("fail-test");
    assert.equal(schedule?.failCount, 1);
    assert.equal(schedule?.consecutiveFails, 1);
    assert.equal(schedule?.lastStatus, "failed");
    assert.equal(schedule?.lastError, "test error");
    await db.close();
  });

  test(`${label}: consecutive fails auto-disables schedule`, async () => {
    const db = await openDb();

    await db.scheduler.schedule("auto-disable", {
      intervalMs: 100,
      maxConsecutiveFails: 2,
      handler: async () => {
        throw new Error("fail");
      },
    });

    await db.scheduler.run("auto-disable");
    await db.scheduler.run("auto-disable");

    const schedule = await db.scheduler.get("auto-disable");
    assert.equal(schedule?.enabled, false);
    assert.equal(schedule?.consecutiveFails, 2);
    await db.close();
  });

  test(`${label}: successful run resets consecutive fails`, async () => {
    const db = await openDb();

    await db.scheduler.schedule("reset-fails", {
      intervalMs: 100,
      maxConsecutiveFails: 3,
      handler: async () => {
        throw new Error("fail");
      },
    });

    await db.scheduler.run("reset-fails");
    let s = await db.scheduler.get("reset-fails");
    assert.equal(s?.consecutiveFails, 1);

    // Switch handler to succeed
    await db.scheduler.remove("reset-fails");
    await db.scheduler.schedule("reset-fails", {
      intervalMs: 100,
      maxConsecutiveFails: 3,
      handler: async () => {},
    });
    await db.scheduler.run("reset-fails");

    s = await db.scheduler.get("reset-fails");
    assert.equal(s?.consecutiveFails, 0);
    assert.equal(s?.runCount, 1);
    await db.close();
  });

  test(`${label}: schedule requires expression or intervalMs`, async () => {
    const db = await openDb();

    await assert.rejects(
      () => db.scheduler.schedule("no-expr", { handler: async () => {} }),
      { message: /Either expression or intervalMs is required/ }
    );
    await db.close();
  });

  test(`${label}: get/ pause/ resume/ run throw for nonexistent schedule`, async () => {
    const db = await openDb();

    await assert.rejects(() => db.scheduler.pause("nope"), { message: /not found/ });
    await assert.rejects(() => db.scheduler.resume("nope"), { message: /not found/ });
    await assert.rejects(() => db.scheduler.run("nope"), { message: /not found/ });
    await db.close();
  });

  test(`${label}: start and stop lifecycle`, async () => {
    const db = await openDb();

    await db.scheduler.schedule("lifecycle", {
      intervalMs: 100,
      handler: async () => {},
    });

    await db.scheduler.start();
    await new Promise((r) => setTimeout(r, 1_500));
    await db.scheduler.stop();

    const schedule = await db.scheduler.get("lifecycle");
    assert.ok(schedule);
    assert.ok(schedule.runCount >= 1);
    await db.close();
  });

  test(`${label}: once schedule auto-disables after execution`, async () => {
    const db = await openDb();

    await db.scheduler.scheduleOnce("once-auto", {
      runAt: new Date(Date.now() - 1_000).toISOString(),
      handler: async () => {},
    });

    await db.scheduler.start();
    await new Promise((r) => setTimeout(r, 500));
    await db.scheduler.stop();

    const schedule = await db.scheduler.get("once-auto");
    assert.equal(schedule?.enabled, false);
    assert.equal(schedule?.runCount, 1);
    await db.close();
  });
}
