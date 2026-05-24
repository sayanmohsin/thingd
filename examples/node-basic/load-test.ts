import { ThingD } from "thingd";

async function run() {
  const db = await ThingD.open({ 
    path: "/Users/sayanmohsin/Space/Programming/personal/thingd/data.db", 
    driver: "native" 
  });
  
  console.log("Generating continuous load to root data.db... Press Ctrl+C to stop.");
  
  const queue = db.queue("load-queue");

  setInterval(async () => {
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
      // Try to claim up to 5 jobs
      for (let i = 0; i < 5; i++) {
        const job = await queue.claim({ leaseMs: 5000 });
        if (job) {
          // Randomly Ack (80% chance) or Fail (20% chance) to simulate real-world processing
          if (Math.random() > 0.2) {
            await queue.ack(job.id);
          } else {
            await queue.fail(job.id, "Random simulated failure");
          }
        }
      }
    } catch {}

  }, 1000);
}

run().catch(console.error);
