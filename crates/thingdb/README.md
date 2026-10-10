# ThingDB

ThingDB is an experimental embedded key-value database written in Rust. It is
available as the standalone `thingdb` crate and can be used without the
`thingd` crate. It stores opaque byte keys and values in ordered keyspaces,
with atomic batches, snapshots, and ordered prefix/range scans. It is intended
for Rust applications that need an embedded key/value engine—not a SQL server
or a full application-level object database.

```rust
use thingdb::{Database, KeyspaceCreateOptions};

let db = Database::builder(":memory:").open()?;
let users = db.keyspace("users", KeyspaceCreateOptions::default)?;
users.insert(b"alice", br#"{"active":true}"#)?;
assert_eq!(users.get(b"alice")?, Some(br#"{"active":true}"#.to_vec()));
# Ok::<(), thingdb::Error>(())
```

ThingDB has two storage modes. `:memory:` opens ordered process-local RAM
storage: it performs no filesystem I/O and loses all data when the process
exits. A filesystem path opens the experimental durable mode, which uses a
checksummed WAL, recoverable manifests, and immutable ordered tables.
Successful durable writes cross the WAL sync boundary before acknowledgement.
The durable format is not compatible with RocksDB files, and no durable
file-format stability promise is made before ThingDB 1.0.

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
