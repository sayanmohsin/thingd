import os from "node:os";
import path from "node:path";
import { ThingD } from "@thingd/sdk";

async function run() {
  const db = await ThingD.open({
    path: path.join(os.homedir(), "Downloads", "data.db"),
    driver: "native",
  });

  console.log("Generating continuous load to root data.db... Press Ctrl+C to stop.");

  const queue = db.queue("load-queue");

  while (true) {
    console.log(process.memoryUsage());
    // Generate a random burst of 5 to 15 operations every second
    const count = Math.floor(Math.random() * 10) + 5;

    for (let i = 0; i < count; i++) {
      const now = Date.now();

      // 1. Objects
      await db.put("load-test", { id: `obj-${now}-${i}`, val: Math.random() });

      // 2. Streams
      await db.events.append("load-events", { type: "ping", val: Math.random() });

      // 3. Queues (Push)
      await queue.push({ task: `process-${now}-${i}` });
    }

    // Process some queue items to make the Active/Dead metrics jump
    try {
      for (let i = 0; i < 5; i++) {
        const job = await queue.claim({ leaseMs: 5000 });
        if (job) {
          // Randomly Ack (80% chance). The other 20% we just ignore,
          // causing their lease to expire and eventually become dead jobs!
          if (Math.random() > 0.2) {
            await queue.ack(job.id);
          }
        }
      }
    } catch {}

    // Wait 1 second before next burst (prevents async callback overlap leaks)
    await new Promise((r) => setTimeout(r, 1000));
  }
}

run().catch(console.error);
