import { ThingD } from "thingd";

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
  console.log("\n🚀 Starting thingd Node.js Stream Example...");
  const db = await ThingD.open({ path: "../../data.db", driver: "native" });
  
  const streamName = "activity-log";
  log("1. Append Events", `Appending sample events to stream '${streamName}'...`);
  await db.events.append(streamName, { type: "user.login", userId: "user-1" });
  await db.events.append(streamName, { type: "user.click", userId: "user-1", target: "btn-buy" });
  
  log("2. Read Events", `Reading all events from stream '${streamName}'...`);
  const events = await db.events.list(streamName);
  log("3. Result", `Stream '${streamName}' contains ${events.length} events:`, events);

  await db.close();
  console.log("\n🎉 Stream Example completed!\n");
}

main().catch((error) => {
  console.error("❌ Example failed with error:", error);
  process.exit(1);
});
