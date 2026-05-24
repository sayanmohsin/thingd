console.log("Starting...");
import { ThingD } from "thingd";
import os from "os";
import path from "path";

console.log("Imported ThingD");

async function run() {
  console.log("Opening DB...");
  try {
    const db = await ThingD.open({ 
      path: path.join(os.homedir(), "Downloads", "data.db"), 
      driver: "native" 
    });
    console.log("Opened DB");
  } catch (err) {
    console.error("Error opening:", err);
  }
}

run().then(() => console.log("Done")).catch(console.error);
