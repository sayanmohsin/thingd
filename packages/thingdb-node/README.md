# @thingd/thingdb

Standalone embedded ordered byte key/value storage for Node.js. The package
does not start Thingd or a network service.

Requires Node.js 16 or newer.

```sh
npm install @thingd/thingdb
```

```js
import { ThingDB } from "@thingd/thingdb";

const db = new ThingDB("./data");
const users = db.keyspace("users");
await users.put(Buffer.from("alice"), Buffer.from('{"active":true}'));
const value = await users.get(Buffer.from("alice"));
await db.putMany([
  { keyspace: "users", key: Buffer.from("bob"), value: Buffer.from("active") },
  { keyspace: "users", key: Buffer.from("alice"), delete: true },
]);
await db.persist();
db.close();
```

Pass `":memory:"` for process-local storage that disappears when the process
exits. Filesystem paths use the durable engine. A durable database path is
owned by one process at a time; database operations are safe to call from
multiple threads. `putMany` is atomic across its keyspaces. Keys and values
are opaque bytes and scans return entries in byte order.
Methods accept `Buffer` or `Uint8Array` inputs and return byte buffers.

The on-disk format is experimental until ThingDB 1.0. Keep independent backups
and export/reimport logical records if a future format change requires it.
ThingDB is not SQL and this package does not include a standalone CLI.
Thingd's existing CLI remains the place for Thingd operation commands.

See the [ThingDB guide](https://engine.thingd.cloud/api-spec/thingdb) for the
Rust API and durability details.
