# Storage backends

Thingd uses an embedded storage engine. The production runtime does not
connect to PostgreSQL, Redis, RocksDB, or another database service.

## Runtime modes

| Mode | Durable storage | Process boundary |
| --- | --- | --- |
| `memory` | Process memory | In-process |
| `native` | RocksDB by default; experimental ThingDB opt-in | In-process N-API addon |
| `thingd-server` | RocksDB by default; experimental ThingDB opt-in | One server process/container |
| HTTP SDK / Cloud | Remote Thingd server | HTTP transport |

The `ThingStore` contract is shared by the memory and durable engines. REST,
MCP, the Node SDK, the browser client, and Thingd Cloud use the same public
object, event, queue, link, schema, search, vector, and replication contracts.

## In-memory modes

Thingd has two process-local RAM use cases:

| Use case | Entry point | Provides | Lifetime |
| --- | --- | --- | --- |
| Thingd RAM database | `ThingD.open(":memory:")` | Objects, events, queues, links, schemas, vectors, snapshots, and search | Lost when the process exits |
| ThingDB cache | `MemoryCache` / cache API | Bounded key/value cache with TTL, LRU eviction, and cache diagnostics | Lost when the process exits |

Thingd RAM is a full semantic database mode. It creates no WAL, manifest,
table, lock, search, or temporary database files. It is intended for disposable
runtime state, development, tests, and process-local application memory; it is
not a backup or durable storage mode. The standalone cache is a lighter and
faster primitive for transient key/value caching and should be preferred when
the full Thingd object, event, queue, or search contract is not required.

The portable `memory` SDK subpath and browser/edge runtimes use the
TypeScript/reference in-memory implementation where the native Rust addon is
not available. This has the same public semantic contract and is also
non-durable.

RocksDB is statically built into the native addon and server artifact by
default. Set `THINGD_STORAGE_BACKEND=thingdb` to opt into the experimental
Rust-native ThingDB backend. A sidecar deployment may still use a separate
Thingd server process because HTTP requires a server, but it does not require a
database container or database service.

Rust consumers that do not need RocksDB can compile the experimental durable
ThingDB path with `default-features = false` and the `thingdb-backend` feature.
The compatibility `persistent` feature intentionally keeps enabling both
backends for existing applications. This compile-time separation does not
change the runtime defaults: RocksDB remains the default durable backend, and
ThingDB remains a separate, opt-in format.

ThingDB is a new format with a checksummed WAL, ordered keyspaces, atomic
batches, snapshots, and compacted table files. It does not open RocksDB files
directly. Switching between formats is a logical repack, not a file rename.
Keep RocksDB as the default until the experimental backend passes the
large-store durability and performance gates.

ThingDB can also be consumed independently of Thingd as the low-level
`thingdb` Rust crate. See the [standalone ThingDB API contract](./api-spec/thingdb.md)
for its keyspace, batch, scan, snapshot, recovery, and `MemoryCache` APIs.

## Experimental durable ThingDB

RocksDB remains the default durable backend. ThingDB is a separate, experimental
format selected explicitly with `THINGD_STORAGE_BACKEND=thingdb`; it is not a
production replacement for RocksDB. ThingDB does not open RocksDB files, so
switching formats requires a logical repack rather than renaming a directory or
changing the setting in place.

ThingDB uses a checksummed WAL, ordered keyspaces, atomic batches, snapshots,
and compacted table files. A successful single write is acknowledged after WAL
sync and state application. Background table flushing may continue afterward,
and scans merge immutable layers with tombstone precedence. The implementation
still keeps substantial state in memory. Keep the source database and verify a
repacked destination before switching traffic; do not rely on experimental
ThingDB as the only copy of important production data.

ThingDB RAM is available for disposable process-local state and creates no
database files. It does not change the durable backend default. `MemoryEngine`
remains available as the portable reference implementation.

See [Benchmarks](./benchmarks.md) for local comparison methodology. Benchmark
results are environment-specific and are not a production performance claim.

## Legacy storage formats

The current runtime supports RocksDB and the experimental ThingDB format only.
It does not open older native storage directories. Existing legacy stores must
be recovered with the archived compatibility release that created them, or
through a previously generated logical export; current releases do not perform
automatic format conversion.

Do not rename a storage directory or change `THINGD_STORAGE_BACKEND` in place.
Use the supported logical repack operation for a current RocksDB or ThingDB
store, keep the source untouched, and validate the destination before changing
traffic.

After validation, point the runtime at the new directory with
`THINGD_PATH`/`THINGD_DATABASE` or the native SDK path. To repack a RocksDB
store into ThingDB, set `THINGD_STORAGE_BACKEND=thingdb` and run:

```bash
THINGD_STORAGE_BACKEND=thingdb \
  thingd-server --repack /data/thingd-rocksdb \
  --destination /data/thingd-thingdb
```

For a ThingDB source, add `--source-backend thingdb`. Keep the original
directory until application-level verification is complete.
