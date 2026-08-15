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

RocksDB is statically built into the native addon and server artifact by
default. Set `THINGD_STORAGE_BACKEND=thingdb` to opt into the experimental
Rust-native ThingDB backend. A sidecar deployment may still use a separate
Thingd server process because HTTP requires a server, but it does not require a
database container or database service.

ThingDB is a new format with a checksummed WAL, ordered keyspaces, atomic
batches, snapshots, and compacted table files. It does not open RocksDB files
directly. Switching between formats is a logical repack, not a file rename.
Keep RocksDB as the default until the experimental backend passes the
large-store durability and performance gates.

## ThingDB development phase

ThingDB is currently in **Phase 0: experimental foundation and compatibility**.
This phase is complete enough for opt-in development, differential testing, and
safe logical repack, but it is not a production replacement for RocksDB.

| Phase | Goal | Status |
| --- | --- | --- |
| 0. Foundation | WAL, checksums, manifests, ordered access, batches, snapshots, repack, shared Thingd contracts | Current; implemented and opt-in |
| 1. Storage engine hardening | Bounded memtables, immutable incremental tables, leveled/size-tiered compaction, fault injection | Next |
| 2. Correctness and recovery | Crash matrices, interrupted compaction recovery, corruption handling, differential tests, fuzzing | Planned |
| 3. Scale and performance | Large-data benchmarks, memory/disk amplification limits, restart and recovery budgets | Planned |
| 4. Controlled adoption | Soak testing, operational rollback, backup/restore validation, limited opt-in deployments | Planned |
| 5. Default-candidate review | Compare against RocksDB gates and decide whether the default should change | Not scheduled |

The current implementation still keeps substantial state in memory and uses a
full snapshot table during compaction. Until Phases 1–4 pass, do not use
ThingDB as the only copy of important production data. Keep the RocksDB source
directory during repack and validate the destination before switching traffic.

See [Benchmarks](./benchmarks.md) for the workload matrix and promotion gates.

## Migration from legacy stores

Existing Fjall directories are not opened by the RocksDB runtime. Migration is
an explicit, one-time logical copy that preserves primary records, IDs,
versions, timestamps, events, queues, links, schemas, migrations, idempotency
state, replication state, and encryption markers. Search indexes are derived
state and are rebuilt after migration.

Build and run the isolated migration utility from this repository. It is a
temporary beta compatibility tool: do not add new migration formats or make it
a runtime dependency. It will be removed only after the deprecation gates in
the Cloud handoff are satisfied.

```bash
cargo run -p thingd-migrate -- fjall-to-rocksdb \
  --source /data/thingd-fjall \
  --destination /data/thingd-rocksdb
```

The utility copies in bounded batches and reports counts per keyspace. The
destination is validated before promotion and the source remains untouched.

For encrypted stores, provide the same 64-character hexadecimal key:

```bash
cargo run -p thingd-migrate -- fjall-to-rocksdb \
  --source /data/thingd-fjall \
  --destination /data/thingd-rocksdb \
  --encryption-key "$THINGD_ENCRYPTION_KEY"
```

The utility requires an inactive source, refuses an existing destination and
unsafe paths, validates the destination before promotion, and leaves the
source untouched. This is a logical migration, not a binary conversion or a
runtime dependency. The Fjall crate is linked only into this offline utility;
it is not linked into `thingd-server` or `@thingd/native`.

CI keeps this utility in a separate migration job. Routine server, native, and
Rust runtime checks intentionally exclude `thingd-migrate`, while the migration
job still runs formatting, Clippy, and the Fjall-to-RocksDB round-trip tests.

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
