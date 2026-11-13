# ThingDB: embedded Rust key-value database

ThingDB is an embedded database for ordered byte-key and
byte-value storage. It is published as the `thingdb` crate and does not
require Thingd. Keyspaces organize data; atomic batches group related writes;
ordered scans and snapshots provide consistent access. It is a low-level
key/value database, not a SQL server or a semantic object/event/queue API.

```toml
[dependencies]
thingdb = "0.92.9"
```

Node.js applications can use the standalone `@thingd/thingdb` package:

```sh
npm install @thingd/thingdb
```

```js
import { ThingDB } from "@thingd/thingdb";

const db = new ThingDB("./data");
const users = db.keyspace("users");
await users.put(Buffer.from("alice"), Buffer.from("hello"));
const value = await users.get(Buffer.from("alice"));
await db.putMany([
  { keyspace: "users", key: Buffer.from("bob"), value: Buffer.from("world") },
]);
const snapshot = await db.snapshot();
db.close();
```

Node reads, writes, deletes, scans, batches, snapshot creation, persistence, and
compaction are Promise-based. Snapshot lookups and `ThingDB.close()` are
synchronous. `ThingDBSnapshot.close()` releases its snapshot handle. Closing a
database releases its database handle;
operations after close fail. Key and value inputs accept `Buffer` or
`Uint8Array`. The package requires Node.js 16 or newer. The first package
release supports Linux x64 and
arm64, macOS x64 and arm64, and Windows x64.

The C ABI is available in the published `thingdb-ffi` crate with the public
header at `bindings/thingdb-ffi/include/thingdb.h`. It uses opaque database
handles, byte pointer/length pairs, status codes, and library-owned output
buffers released with `thingdb_buffer_free`; paths and keyspace names are
UTF-8, and keys and values accept arbitrary bytes. The versioned Go module wraps that
ABI with `Open`, `Put`, `Get`, `Delete`, and `Close`; it requires the matching
ThingDB shared library to be linked into the Go application.

The crate provides process-local RAM and experimental WAL-backed durable
modes, plus a separate bounded `MemoryCache` primitive. Thingd builds its
higher-level objects, events, queues, links, vectors, and search on its own
semantic layer; these are not part of ThingDB's standalone key/value API.

## Opening a database

Use `:memory:` for a process-local database:

```rust
use thingdb::{Database, KeyspaceCreateOptions};

let db = Database::open(":memory:")?;
let users = db.keyspace("users", KeyspaceCreateOptions::default)?;
# Ok::<(), thingdb::Error>(())
```

RAM mode performs no filesystem I/O, creates no WAL or table files, and loses
all data when the process exits. It is not a backup or durable-storage mode.

Pass a filesystem path for the experimental durable mode:

```rust
let db = Database::open("./data")?;
```

Successful durable writes cross the WAL sync boundary before the operation is
acknowledged. Durable ThingDB uses its own checksummed WAL, manifests, and
ordered table format; it does not open or produce RocksDB-compatible files.

## Keyspaces and values

Keyspaces store opaque byte keys and values with deterministic byte ordering:

```rust
users.insert(b"alice", b"hello")?;
assert_eq!(users.get(b"alice")?, Some(b"hello".to_vec()));
users.remove(b"alice")?;
```

The API supports `get`, `get_shared`, `insert`, `remove`, ordered `iter`,
`prefix`, `first_prefix`, `first_prefix_after`, `range`, and `range_bounds`.
Prefix and range boundaries use byte ordering and preserve deterministic
ordering. Iterators expose errors through `error()` or return all entries and
an error through `into_result()`; storage corruption must not be interpreted as
an empty result.

## Atomic batches

Build one batch for related changes across keyspaces:

```rust
let audit = db.keyspace("audit", KeyspaceCreateOptions::default)?;
db.batch()
    .put(&users, b"alice", b"updated")
    .put(&audit, b"1", b"user-updated")
    .commit()?;
```

A committed batch is applied atomically across all included keyspaces. In
durable mode, its WAL data is synced before success is returned. A failed or
ambiguous durable operation is reported as an error; recovery and reopen rules
remain part of the experimental format contract.

## Persistence, recovery, and diagnostics

`persist(PersistMode::SyncAll)` explicitly flushes durable state, while
`compact()` performs durable maintenance. These operations are rejected in RAM
mode with an explicit error. `snapshot()` creates a consistent read view.

`journal_disk_space`, `journal_count`, `wal_diagnostics`, and `ram_diagnostics`
provide bounded operational measurements. RAM diagnostics report no durable
WAL or disk usage. Checksum, malformed-frame, manifest, table, and recovery
errors are returned explicitly.

The durable format and language bindings are experimental until ThingDB 1.0.
Durable databases are owned by one process at a time, while handles are thread-safe within that
process. No in-place conversion from RocksDB or compatibility promise for
pre-1.0 files is provided; export and reimport logical records when moving
between incompatible formats. Keep independent backups until the v1 format
stability gate is declared.

## `MemoryCache`

`MemoryCache` is separate from the ordered database API. It is a bounded,
process-local byte cache with entry and byte limits, TTL expiration, LRU
eviction, explicit removal, clearing, and statistics:

```rust
use std::time::Duration;
use thingdb::{CacheOptions, MemoryCache};

let cache = MemoryCache::new(CacheOptions::default())?;
cache.insert_with_ttl(b"key", b"value", Duration::from_secs(30))?;
assert_eq!(cache.get(b"key")?, Some(b"value".to_vec()));
# Ok::<(), thingdb::Error>(())
```

The cache is not durable, persistent, replicated, or a semantic Thingd object
store. It creates no filesystem artifacts.

## Stability and Thingd integration

ThingDB is an experimental crate maintained in the Thingd repository. In
Thingd, its RAM mode backs disposable native/server in-memory operation.
RocksDB remains Thingd's default durable backend, while durable ThingDB is
opt-in and separately validated. ThingDB's durable format is independent of
RocksDB and has no file-format stability promise before ThingDB 1.0.

See the [ThingDB rustdoc on docs.rs](https://docs.rs/thingdb), the
[storage backend guide](../storage-backends.md), and the
[benchmark methodology](../benchmarks.md).
