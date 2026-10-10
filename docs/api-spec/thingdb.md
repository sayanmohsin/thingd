# ThingDB: embedded Rust key-value database

ThingDB is an experimental embedded database for ordered byte-key and
byte-value storage. It is published as the `thingdb` crate and does not
require Thingd. Keyspaces organize data; atomic batches group related writes;
ordered scans and snapshots provide consistent access. It is a low-level
key/value database, not a SQL server or a semantic object/event/queue API.

```toml
[dependencies]
thingdb = "0.92.0"
```

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

The durable format is experimental: no durable file-format stability promise
is made before ThingDB 1.0, and no in-place conversion from RocksDB is
supported. Keep independent backups and do not use experimental durable
ThingDB as the only copy of important data.

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
