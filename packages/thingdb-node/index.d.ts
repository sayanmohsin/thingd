/// Embedded byte key/value database. Durable paths allow one owning process.
export class ThingDB {
  constructor(path: string);
  keyspace(name: string): ThingDBKeyspace;
  putMany(
    operations: Array<{ keyspace: string; key: Uint8Array; value?: Uint8Array; delete?: boolean }>
  ): Promise<void>;
  persist(): Promise<void>;
  compact(): Promise<void>;
  snapshot(): Promise<ThingDBSnapshot>;
  close(): void;
}

export class ThingDBSnapshot {
  private constructor();
  get(keyspace: string, key: Uint8Array): Uint8Array | null;
  close(): void;
}

export class ThingDBKeyspace {
  private constructor();
  get(key: Uint8Array): Promise<Uint8Array | null>;
  put(key: Uint8Array, value: Uint8Array): Promise<void>;
  delete(key: Uint8Array): Promise<void>;
  entries(): Promise<Array<{ key: Uint8Array; value: Uint8Array }>>;
  prefix(prefix: Uint8Array): Promise<Array<{ key: Uint8Array; value: Uint8Array }>>;
}

export const loadedPath: string;
