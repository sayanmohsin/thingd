console.log("Starting...");

import os from "node:os";
import path from "node:path";
import { ThingD } from "thingd";

console.log("Imported ThingD");

async function run() {
  console.log("Calling ThingD.open...");
  const dbPromise = ThingD.open({
    path: path.join(os.homedir(), "Downloads", "data.db"),
    driver: "native",
  });
  console.log("Got promise");
  const db = await dbPromise;
  console.log("Opened DB");
  await db.close();
}

run().catch(console.error);
