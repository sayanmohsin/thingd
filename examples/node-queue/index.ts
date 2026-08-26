import { ThingD } from "@thingd/sdk";

// Helper for formatted, colored console logging
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
  console.log("\n🚀 Starting thingd Node.js Queue Example...");
  const db = await ThingD.open({ path: "./data.db", driver: "native" });

  const queue = db.queue("worker-queue");

  log("1. Push Jobs", "Pushing new background jobs to the queue...");
  await queue.push({ task: "resize-image", imageId: "img-123" });
  await queue.push({ task: "send-email", email: "user@example.com" });

  const activeJobs = await queue.list();
  log("2. View Queue", `There are currently ${activeJobs.length} active jobs.`, activeJobs);

  log("3. Process Jobs", "Claiming and processing jobs one by one...");
  while (true) {
    const job = await queue.claim({ leaseMs: 5000 });
    if (!job) {
      break;
    }
    log("Processing", `Claimed job: ${job.id}`, job.payload);
    await queue.ack(job.id);
    log("Acknowledged", `Job ${job.id} completed successfully!`);
  }

  await db.close();
  console.log("\n🎉 Queue Example completed!\n");
}

main().catch((error) => {
  console.error("❌ Example failed with error:", error);
  process.exit(1);
});
