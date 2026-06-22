console.log("Starting...");

import os from "node:os";
import path from "node:path";
import { ThingD } from "@thingd/sdk";

console.log("Imported ThingD");

async function run() {
  console.log("Opening DB...");
  try {
    const db = await ThingD.open({
      path: path.join(os.homedir(), "Downloads", "data.db"),
      driver: "native",
    });
    console.log("Opened DB");
    await db.close();
  } catch (err) {
    console.error("Error opening:", err);
  }
}

run()
  .then(() => console.log("Done"))
  .catch(console.error);
