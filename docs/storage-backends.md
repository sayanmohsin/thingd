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

ThingDB is currently in **Phase 1B: durable group commit**. Phase 0 and Phase
1A are complete enough for opt-in development, differential testing, and safe
logical repack, but ThingDB is not a production replacement for RocksDB.

| Phase | Goal | Status |
| --- | --- | --- |
| 0. Foundation | WAL, checksums, manifests, ordered access, batches, snapshots, repack, shared Thingd contracts | Complete; experimental and opt-in |
| 1A. WAL hardening | Sync-before-ack writes, WAL timing diagnostics, batch-path measurement, deterministic WAL fault tests | Complete; no promotion claim |
| 1B. Durable group commit | Bounded writer queue, one physical sync for nearby durable frames, grouped-write recovery and diagnostics | Active |
| 1C. Storage engine hardening | Bounded memtables, immutable incremental tables, leveled/size-tiered compaction | Planned after group-commit evidence |
| 2. Correctness and recovery | Crash matrices, interrupted compaction recovery, corruption handling, differential tests, fuzzing | Planned |
| 3. Scale and performance | Large-data benchmarks, memory/disk amplification limits, restart and recovery budgets | Planned |
| 4. Controlled adoption | Soak testing, operational rollback, backup/restore validation, limited opt-in deployments | Planned |
| 5. Default-candidate review | Compare against RocksDB gates and decide whether the default should change | Not scheduled |

Single writes remain synchronously WAL-backed before acknowledgement. Explicit
multi-key batches share one WAL frame and one sync boundary. Phase 1B additionally
groups nearby independent frames into one physical sync while preserving their
individual atomicity. The current implementation still keeps substantial state
in memory and uses a full snapshot table during compaction. Until Phases 1A–4 pass, do not use
ThingDB as the only copy of important production data. Keep the RocksDB source
directory during repack and validate the destination before switching traffic.

See [Benchmarks](./benchmarks.md) for the workload matrix and promotion gates.

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
