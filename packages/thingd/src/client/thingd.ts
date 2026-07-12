import { HttpThingStore } from "./http-thing-store.js";
import { InMemoryThingStore } from "./in-memory-thing-store.js";

export type {
  ThingDDriver,
  ThingDOpenConfig,
  ThingDOpenOptions,
} from "../thingd.js";

export type ThingDClientOptions = {
  driver?: "memory" | "cloud";
  url?: string;
  authToken?: string;
  apiKey?: string;
};

function resolveToken(options: ThingDClientOptions): string | undefined {
  return options.authToken ?? options.apiKey;
}

/**
 * Open a thingd connection from a browser/edge-compatible environment.
 *
 * Unlike ThingD.open() from the main entry point, this does NOT read from
 * process.env — all configuration must be passed explicitly.
 *
 * @example
 * ```ts
 * const db = await openThingD({ url: "http://localhost:8757" });
 * await db.put("notes", { id: "1", text: "hello" });
 * ```
 */
export async function openThingD(
  options: ThingDClientOptions = {}
): Promise<HttpThingStore | InMemoryThingStore> {
  if (options.driver === "memory" || !options.url) {
    return new InMemoryThingStore();
  }

  return HttpThingStore.open({
    url: options.url,
    authToken: resolveToken(options),
  });
}

export { HttpThingStore, InMemoryThingStore };
