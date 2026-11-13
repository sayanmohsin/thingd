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
const databaseHandles = new WeakMap();
const databaseFinalizer = new FinalizationRegistry((handle) => {
  try {
    binding.closeDatabase(handle);
  } catch {
    // The native registry may already have been cleared during process shutdown.
  }
});
const snapshotFinalizer = new FinalizationRegistry((handle) => {
  try {
    binding.closeSnapshot(handle);
  } catch {
    // The native registry may already have been cleared during process shutdown.
  }
});

export class ThingDB {
  #finalizerToken = {};

  constructor(path) {
    const handle = binding.openDatabase(path);
    databaseHandles.set(this, handle);
    databaseFinalizer.register(this, handle, this.#finalizerToken);
  }

  keyspace(name) {
    const handle = databaseHandle(this);
    binding.createKeyspace(handle, name);
    return new ThingDBKeyspace(this, name);
  }

  putMany(operations) {
    return binding.putMany(
      databaseHandle(this),
      operations.map((operation) => ({
        ...operation,
        key: Buffer.from(operation.key),
        value: operation.value === undefined ? undefined : Buffer.from(operation.value),
      }))
    );
  }

  persist() {
    return binding.persist(databaseHandle(this));
  }

  compact() {
    return binding.compact(databaseHandle(this));
  }

  async snapshot() {
    return new ThingDBSnapshot(await binding.snapshot(databaseHandle(this)));
  }

  close() {
    const handle = databaseHandle(this);
    binding.closeDatabase(handle);
    databaseHandles.delete(this);
    databaseFinalizer.unregister(this.#finalizerToken);
  }
}

export class ThingDBKeyspace {
  #database;
  #name;

  constructor(database, name) {
    this.#database = database;
    this.#name = name;
  }

  get(key) {
    return binding.get(databaseHandle(this.#database), this.#name, Buffer.from(key));
  }

  put(key, value) {
    return binding.put(
      databaseHandle(this.#database),
      this.#name,
      Buffer.from(key),
      Buffer.from(value)
    );
  }

  delete(key) {
    return binding.delete(databaseHandle(this.#database), this.#name, Buffer.from(key));
  }

  entries() {
    return binding.entries(databaseHandle(this.#database), this.#name);
  }

  prefix(prefix) {
    return binding.prefix(databaseHandle(this.#database), this.#name, Buffer.from(prefix));
  }
}

function databaseHandle(database) {
  const handle = databaseHandles.get(database);
  if (handle === undefined) {
    throw new Error("ThingDB is closed");
  }
  return handle;
}

export class ThingDBSnapshot {
  #handle;
  #finalizerToken = {};

  constructor(handle) {
    this.#handle = handle;
    snapshotFinalizer.register(this, handle, this.#finalizerToken);
  }

  get(keyspace, key) {
    if (this.#handle === undefined) {
      throw new Error("ThingDB snapshot is closed");
    }
    return binding.snapshotGet(this.#handle, keyspace, Buffer.from(key));
  }

  close() {
    if (this.#handle === undefined) {
      return;
    }
    const handle = this.#handle;
    binding.closeSnapshot(handle);
    this.#handle = undefined;
    snapshotFinalizer.unregister(this.#finalizerToken);
  }
}

export const loadedPath = bindingPath;
