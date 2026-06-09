import { ThingD } from "thingd";

// Formatted logger helper
function log(step: string, message: string, data?: unknown) {
  const blue = "\x1b[34m";
  const green = "\x1b[32m";
  const reset = "\x1b[0m";

  console.log(`\n${blue}=== [Step: ${step}] ===${reset}`);
  console.log(`${green}${message}${reset}`);
  if (data !== undefined) {
    console.log(JSON.stringify(data, null, 2));
  }
}

async function main() {
  console.log("\n🚀 Starting thingd Queue Scheduler Heartbeat & Worker Demo...");

  const db = await ThingD.open({
    path: "./agent_memory.db",
    driver: "native",
  });

  // 1. Setup a due recurring schedule
  const now = Date.now();
  const scheduleId = "nightly-report";
  const intervalMs = 24 * 60 * 60 * 1000; // 24 hours

  // Set the first run to be 5 seconds in the past, making it immediately due for execution!
  const runAt = new Date(now - 5000).toISOString();

  await db.put("schedules", {
    id: scheduleId,
    action: "generate_report",
    payload: { format: "pdf", recipient: "team@thingd.io" },
    enabled: true,
    runAt,
    recurringIntervalMs: intervalMs,
  });

  log(
    "1. Register Schedule",
    "Created a recurring schedule in the 'schedules' collection, marked as immediately due:",
    {
      id: scheduleId,
      runAt,
      recurringIntervalMs: intervalMs,
    }
  );

  // 2. Scheduler Heartbeat Routine
  // This simulates an external clock or cron trigger querying due schedules and pushing jobs to the queue.
  log(
    "2. Run Scheduler Heartbeat",
    "Heartbeat query: checking for enabled, due schedules (runAt <= now)..."
  );

  // Query schedules (in a real app, you would query schedules that match the criteria)
  const schedule = await db.get("schedules", scheduleId);
  if (schedule?.enabled && new Date(schedule.runAt as string).getTime() <= Date.now()) {
    const queue = db.queue("scheduler");

    // Push task into queue with a safe idempotency key to prevent double enqueuing
    const idempotencyKey = `schedule:${schedule.id}:${schedule.runAt}`;
    const job = await queue.push(
      {
        scheduleId: schedule.id,
        action: schedule.action,
        payload: schedule.payload,
      },
      {
        idempotencyKey,
      }
    );

    log(
      "2a. Push Task to Queue",
      `Schedule is due! Enqueued a new scheduler job with idempotencyKey "${idempotencyKey}":`,
      job
    );

    // Update the schedule's next run time
    const nextRun = new Date(Date.now() + (schedule.recurringIntervalMs as number)).toISOString();
    await db.put("schedules", {
      ...schedule,
      runAt: nextRun,
    });

    log("2b. Advance Next Run Time", `Advanced schedule next runAt time to: ${nextRun}`);
  }

  // 3. Worker Routine
  // This simulates a worker claiming a leased job, performing the task, and acknowledging it.
  log("3. Claim and Process Task", "Worker checking the 'scheduler' queue for ready jobs...");
  const queue = db.queue("scheduler");
  const claimedJob = await queue.claim({ leaseMs: 15_000 });

  if (claimedJob) {
    log(
      "3a. Task Claimed",
      `Successfully claimed job "${claimedJob.id}". Concurrency lock is active for 15s.`,
      claimedJob
    );

    // Simulate task execution
    const taskPayload = claimedJob.payload;
    console.log(
      `\n⏳ [Executing Action] Running "${taskPayload.action}" with payload:`,
      taskPayload.payload
    );
    await new Promise((resolve) => setTimeout(resolve, 1000)); // simulate 1s work

    // Acknowledge task completion
    const acked = await queue.ack(claimedJob.id);
    log(
      "3b. Task Acknowledged",
      "Job completed and acknowledged successfully. Task cleared from queue.",
      acked
    );
  }

  // 4. Clean close
  await db.close();
  log("4. Database Closed", "Closed the scheduler database safely.");

  console.log("\n🎉 Queue Scheduler Heartbeat & Worker Demo completed successfully!\n");
}

main().catch((error) => {
  console.error("❌ Scheduler demo failed with error:", error);
  process.exit(1);
});
