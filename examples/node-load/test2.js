console.log("Starting...");
import { ThingD } from "thingd";
import os from "os";
import path from "path";

console.log("Imported ThingD");

async function run() {
  console.log("Calling ThingD.open...");
  const dbPromise = ThingD.open({ 
    path: path.join(os.homedir(), "Downloads", "data.db"), 
    driver: "native" 
  });
  console.log("Got promise");
  const db = await dbPromise;
  console.log("Opened DB");
}

run().catch(console.error);
