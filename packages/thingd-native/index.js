import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const native = require("./dist/thingd_native.node");

export const { NativeThingStore } = native;
