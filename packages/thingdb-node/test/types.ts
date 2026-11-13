import { ThingDB } from "../index.js";

const database = new ThingDB("./data");
const users = database.keyspace("users");
const key = new Uint8Array([1]);
const value: Promise<Uint8Array | null> = users.get(key);
const entries: Promise<Array<{ key: Uint8Array; value: Uint8Array }>> = users.entries();
const snapshot = database.snapshot();
const close = database.close();

void value;
void entries;
void snapshot;
void close;
