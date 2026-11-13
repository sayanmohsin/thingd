import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const packageRoot = dirname(fileURLToPath(import.meta.url));
const localBuild = join(packageRoot, "dist", "thingdb_node.node");
const prebuild = join(
  packageRoot,
  "prebuilds",
  `${process.platform}-${process.arch}`,
  "thingdb_node.node"
);
const bindingPath = existsSync(localBuild) ? localBuild : prebuild;

if (!existsSync(bindingPath)) {
  const error = new Error(
    `No @thingd/thingdb binary is available for ${process.platform}-${process.arch}.`
  );
  error.code = "THINGDB_PREBUILD_MISSING";
  throw error;
}

const binding = require(bindingPath);
const { ThingDB: NativeThingDB } = binding;

export class ThingDB {
  #native;

  constructor(path) {
    this.#native = new NativeThingDB(path);
  }

  keyspace(name) {
    return new ThingDBKeyspace(this.#native.keyspace(name));
  }

  putMany(operations) {
    return this.#native.putMany(
      operations.map((operation) => ({
        ...operation,
        key: Buffer.from(operation.key),
        value: operation.value === undefined ? undefined : Buffer.from(operation.value),
      }))
    );
  }

  persist() {
    return this.#native.persist();
  }

  compact() {
    return this.#native.compact();
  }

  async snapshot() {
    return new ThingDBSnapshot(await this.#native.snapshot());
  }

  close() {
    this.#native.close();
  }
}

export class ThingDBKeyspace {
  #native;

  constructor(native) {
    this.#native = native;
  }

  get(key) {
    return this.#native.get(Buffer.from(key));
  }

  put(key, value) {
    return this.#native.put(Buffer.from(key), Buffer.from(value));
  }

  delete(key) {
    return this.#native.delete(Buffer.from(key));
  }

  entries() {
    return this.#native.entries();
  }

  prefix(prefix) {
    return this.#native.prefix(Buffer.from(prefix));
  }
}

export class ThingDBSnapshot {
  #native;

  constructor(native) {
    this.#native = native;
  }

  get(keyspace, key) {
    return this.#native.get(keyspace, Buffer.from(key));
  }
}

export const loadedPath = bindingPath;
