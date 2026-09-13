# Standalone ThingDB API

ThingDB is an experimental, low-level Rust ordered key-value engine. It can
be used independently of the `thingd` crate:

```toml
[dependencies]
thingdb = "0.86.8"
```

The standalone crate provides keyspaces, atomic batches, ordered scans,
snapshots, a durable WAL-backed mode, and a bounded `MemoryCache`. It does not
provide Thingd semantic objects, event streams, queues, links, vectors, or
MCP/REST APIs; those belong to the `thingd` crate.

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

## Stability and relationship to Thingd

ThingDB is currently a separate experimental crate maintained in the Thingd
repository. Thingd uses ThingDB RAM for disposable native/server in-memory
operation, while RocksDB remains the default durable backend. Durable ThingDB
is opt-in and separately validated.

See the [ThingDB rustdoc on docs.rs](https://docs.rs/thingdb), the
[storage backend guide](../storage-backends.md), and the
[benchmark methodology](../benchmarks.md).
