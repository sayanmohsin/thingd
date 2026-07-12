import { openThingD } from "../client/thingd.js";

export { InMemoryThingStore } from "../client/in-memory-thing-store.js";

/**
 * Open a pure in-memory thingd connection.
 * Browser-safe — no Node.js dependencies.
 */
export const openMemoryThingD = () => openThingD({ driver: "memory" });
