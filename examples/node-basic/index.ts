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
  console.log("\n🚀 Starting thingd Node.js Basic TypeScript Example...");

  // 1. Open database (persistent SQLite file via native driver)
  const db = await ThingD.open({
    path: "./data.db",
    driver: "native",
  });
  log("1. Database Open", "Opened a persistent SQLite thingd instance.");

  // 2. Put object
  const decision = await db.put("decisions", {
    id: "rust-core",
    text: "Use Rust for the core engine and TypeScript for the developer API.",
    project: "thingd",
  });
  log("2. Store Object", "Stored a new decision object:", decision);

  // 3. Read object
  const storedDecision = await db.get("decisions", "rust-core");
  log("3. Retrieve Object", "Retrieved the decision object from storage:", storedDecision);

  // 4. Append Event Stream
  const event1 = await db.events.append("project:thingd", {
    type: "decision.made",
    text: "thingd should feel object-shaped and MCP-native.",
  });
  const event2 = await db.events.append("project:thingd", {
    type: "sdk.initialized",
    text: "Initial version of TypeScript SDK initialized.",
  });
  log("4. Append Events", "Appended two events to the stream 'project:thingd':", {
    event1,
    event2,
  });

  // 5. List Events
  const events = await db.events.list("project:thingd");
  log("5. List Events", "Listed all events in stream 'project:thingd':", events);

  // 6. Push job to queue
  const queue = db.queue("embed");
  const job = await queue.push({
    object: "decisions/rust-core",
  });
  log("6. Enqueue Job", "Enqueued a new background job into the 'embed' queue:", job);

  // 7. List queued jobs
  const queuedJobsBefore = await queue.list();
  log("7. List Queued Jobs", "Listed all active jobs in the queue:", queuedJobsBefore);

  // 8. Claim job
  const claimedJob = await queue.claim({ leaseMs: 10_000 });
  log("8. Claim Job", "Claimed the job for processing (leased for 10s):", claimedJob);

  // 9. Ack job
  if (claimedJob) {
    const acked = await queue.ack(claimedJob.id);
    log("9. Ack Job", "Acknowledged and completed the job successfully:", acked);
  }

  // 10. Perform Search
  const searchResults = await db.search("rust");
  log("10. Search", "Searched across all objects and events for query 'rust':", searchResults);

  // 11. Graph Links
  const link = await db.links.create("decisions/rust-core", "authored", "docs/architecture");
  log("11. Create Link", "Created a directed graph link:", link);

  const neighbors = await db.links.neighbors("decisions/rust-core", "Outgoing");
  log("12. Get Neighbors", "Outgoing links from 'decisions/rust-core':", neighbors);

  const linkCount = await db.countLinks();
  log("13. Count Links", `Total links in the store: ${linkCount}`);

  // 14. Batch Operations
  const batchResults = await db.putBatch("tasks", [
    { id: "task-1", title: "Implement search", status: "done" },
    { id: "task-2", title: "Add graph links", status: "done" },
    { id: "task-3", title: "Write docs", status: "active" },
  ]);
  log("14. Batch Put", `Stored ${batchResults.length} objects in a single call:`, batchResults);

  const deletedCount = await db.deleteBatch("tasks", ["task-1"]);
  log("15. Batch Delete", `Deleted ${deletedCount} object(s) in a single call`);

  // 16. Sort and Filter
  const sorted = await db.listObjects("tasks", {
    sortBy: { field: "id", direction: "asc" },
  });
  log("16. Sorted List", "Tasks sorted by ID ascending:", sorted);

  const filtered = await db.listObjects("tasks", {
    filter: { status: "active" },
  });
  log("17. Filtered List", "Tasks with status 'active':", filtered);

  const paged = await db.listObjects("tasks", {
    limit: 1,
    offset: 0,
  });
  log("18. Paginated List", "First 1 task (limit=1, offset=0):", paged);

  // 19. Counts and Discovery
  const objectCount = await db.countObjects();
  const eventCount = await db.countEvents();
  const collections = await db.listCollections();
  const streams = await db.listStreams();
  const queues = await db.listQueues();
  log(
    "19. Counts & Discovery",
    `Objects: ${objectCount}, Events: ${eventCount}, Collections: ${collections.length}, Streams: ${streams.length}, Queues: ${queues.length}`
  );

  // 20. Close the database
  await db.close();
  log("20. Database Closed", "Closed the database instance safely.");

  console.log("\n🎉 Node.js TypeScript Basic Example completed successfully!\n");
}

main().catch((error) => {
  console.error("❌ Example failed with error:", error);
  process.exit(1);
});
