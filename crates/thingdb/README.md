# ThingDB

ThingDB is an experimental embedded key-value database written in Rust. It is
available as the standalone `thingdb` crate and can be used without the
`thingd` crate. It stores opaque byte keys and values in ordered keyspaces,
with atomic batches, snapshots, and ordered prefix/range scans. It is intended
for Rust applications that need an embedded key/value engine—not a SQL server
or a full application-level object database.

## Install

Add the crate to a Rust project:

```sh
cargo add thingdb
```

ThingDB is published independently as a Rust crate. It does not require Thingd,
RocksDB, a server process, or a C++ toolchain.

## In-memory usage

```rust
use thingdb::{Database, KeyspaceCreateOptions};

let db = Database::builder(":memory:").open()?;
let users = db.keyspace("users", KeyspaceCreateOptions::default)?;
users.insert(b"alice", br#"{"active":true}"#)?;
assert_eq!(users.get(b"alice")?, Some(br#"{"active":true}"#.to_vec()));
# Ok::<(), thingdb::Error>(())
```

## Durable usage

Pass a filesystem path to open the experimental durable mode:

```rust
use thingdb::{Database, KeyspaceCreateOptions, PersistMode};

let db = Database::open("./data")?;
let users = db.keyspace("users", KeyspaceCreateOptions::default)?;
users.insert(b"alice", br#"{"active":true}"#)?;
db.persist(PersistMode::SyncAll)?;
# Ok::<(), thingdb::Error>(())
```

Successful durable writes sync their WAL data before acknowledging success.
ThingDB supports atomic batches across keyspaces, consistent snapshots, and
ordered iteration, prefix, and range scans. Values are opaque bytes; ThingDB
does not parse JSON or provide SQL queries.

`:memory:` opens process-local RAM storage, performs no filesystem I/O, and
loses all data when the process exits. Durable ThingDB uses a checksummed WAL,
recoverable manifests, and immutable ordered tables. Its file format is not
compatible with RocksDB, and no durable format stability promise is made before
ThingDB 1.0. Keep independent backups and do not use it as the only copy of
important data.

`MemoryCache` is a separate bounded, process-local cache with TTL and LRU
eviction. It is not a durable database and does not change Thingd's backend
selection.

## ThingDB in Thingd

Thingd uses ThingDB RAM for disposable native/server memory operation and
provides higher-level objects, events, queues, links, vectors, and search in
its own semantic layer. RocksDB remains Thingd's default durable backend;
durable ThingDB is opt-in and experimental. This integration is one use of
the database, not a requirement for using the standalone crate.

ThingDB remains experimental and should not be the only copy of important
data. See the [ThingDB API documentation](https://docs.rs/thingdb), the [ThingDB
API contract](https://github.com/sayanmohsin/thingd/blob/main/docs/api-spec/thingdb.md),
the [Thingd storage backend guide](https://github.com/sayanmohsin/thingd/blob/main/docs/storage-backends.md),
and the [benchmark methodology](https://github.com/sayanmohsin/thingd/blob/main/docs/benchmarks.md).
