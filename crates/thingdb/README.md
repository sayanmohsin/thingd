# thingdb

Experimental Rust-native ordered key-value storage with RAM and durable modes.
ThingDB is a low-level engine that can be used without the `thingd` crate.

```rust
use thingdb::{Database, KeyspaceCreateOptions};

let db = Database::builder(":memory:").open()?;
let users = db.keyspace("users", KeyspaceCreateOptions::default)?;
users.insert(b"alice", br#"{"active":true}"#)?;
assert_eq!(users.get(b"alice")?, Some(br#"{"active":true}"#.to_vec()));
# Ok::<(), thingdb::Error>(())
```

RAM databases perform no filesystem I/O and lose their data when the process
exits. A path opens the experimental durable mode, which uses a checksummed
WAL, recoverable manifests, and immutable ordered tables. Successful durable
writes cross the WAL sync boundary before acknowledgement. The durable format
is not compatible with RocksDB files, and no durable file-format stability
promise is made before ThingDB 1.0.

`MemoryCache` is a separate bounded, process-local cache with TTL and LRU
eviction. It is not a durable database and does not change Thingd's backend
selection.

ThingDB is experimental and should not be the only copy of important data.
See the [ThingDB API documentation](https://docs.rs/thingdb), the [Thingd
storage backend guide](https://github.com/sayanmohsin/thingd/blob/main/docs/storage-backends.md),
and the [benchmark methodology](https://github.com/sayanmohsin/thingd/blob/main/docs/benchmarks.md).
