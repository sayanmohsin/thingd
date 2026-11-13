import { existsSync } from "node:fs";

if (!existsSync(new URL("../index.js", import.meta.url))) {
  throw new Error("@thingd/thingdb is missing index.js");
}
if (!existsSync(new URL("../index.d.ts", import.meta.url))) {
  throw new Error("@thingd/thingdb is missing index.d.ts");
}
if (!existsSync(new URL("../README.md", import.meta.url))) {
  throw new Error("@thingd/thingdb is missing README.md");
}
