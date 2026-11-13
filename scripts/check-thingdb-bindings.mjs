import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const [canonical, goCopy] = await Promise.all([
  readFile("bindings/thingdb-ffi/include/thingdb.h", "utf8"),
  readFile("bindings/go/include/thingdb.h", "utf8"),
]);
assert.equal(goCopy, canonical, "Go binding header must match the canonical ThingDB C ABI header");
console.log("ThingDB Go binding header matches the canonical C ABI header");
