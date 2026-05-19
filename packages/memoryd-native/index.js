import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const native = require("./dist/memoryd_native.node");

export const { NativeMemoryStore } = native;
