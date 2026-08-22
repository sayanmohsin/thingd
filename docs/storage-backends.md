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

ThingDB is currently in **Phase 4: layered table reads**. Phases 0
through 1C are complete enough for opt-in development, differential testing,
and safe logical repack, but ThingDB is not a production replacement for
RocksDB.

| Phase | Goal | Status |
| --- | --- | --- |
| 0. Foundation | WAL, checksums, manifests, ordered access, batches, snapshots, repack, shared Thingd contracts | Complete; experimental and opt-in |
| 1A. WAL hardening | Sync-before-ack writes, WAL timing diagnostics, batch-path measurement, deterministic WAL fault tests | Complete; no promotion claim |
| 1B. Durable group commit | Bounded writer queue, one physical sync for nearby durable frames, grouped-write recovery and diagnostics | Complete; no promotion claim |
| 1C. Immutable table layers | Incremental immutable tables, tombstones, multi-table manifests, bounded flushes, and safe full compaction | Complete; no promotion claim |
| 2. Manifest and compaction recovery | Atomic manifest replacement, temporary-artifact cleanup, interrupted flush/compaction recovery, corruption validation, and fault-injection tests | Complete; no promotion claim |
| 3. Bounded memtables and flush backpressure | Bound mutable table memory, flush automatically after durable commits, preserve restart recovery, and measure flush cost | Complete; no promotion claim |
| 4. Layered table reads | Retain immutable table indexes, seek point reads by key, merge layers for scans, and reduce startup resident state | Active |
| 5. Scale and performance | Large-data benchmarks, memory/disk amplification limits, restart and recovery budgets | Planned |
| 6. Controlled adoption | Soak testing, operational rollback, backup/restore validation, limited opt-in deployments | Planned |
| 7. Default-candidate review | Compare against RocksDB gates and decide whether the default should change | Not scheduled |

Single writes remain synchronously WAL-backed before acknowledgement. Explicit
multi-key batches share one WAL frame and one sync boundary. Phase 1B additionally
groups nearby independent frames into one physical sync while preserving their
individual atomicity. Phase 1C writes only changed keys and tombstones into new
immutable table layers; explicit compaction merges those layers into one
snapshot. The current implementation still keeps substantial state in memory.
Phase 2 validates old-versus-new manifest recovery, table rename boundaries,
temporary artifact cleanup, manifest path safety, and directory durability.
Phase 3 added explicit mutable-table byte accounting and a bounded automatic
flush boundary. A successful write still waits for its WAL sync and any
required flush before acknowledgement; a failed post-sync flush requires
reopen and recovery rather than allowing ambiguous in-memory state.
Phase 4 retains table paths and sorted key indexes instead of loading table
values into the active write state during open. Point reads seek the newest
matching layer first; scans and compaction materialize a merged view with
tombstone precedence. This phase does not yet claim full memory qualification.
Until Phases 1A–4 pass, do not use
ThingDB as the only copy of important production data. Keep the RocksDB source
directory during repack and validate the destination before switching traffic.

The separate in-memory qualification track is currently in **combined Phase
5/6: benchmark, reliability, and soak validation**. The unified benchmark now
includes `thingdb-memory`, and its reliability preflight covers semantic
operations, search cleanup, concurrent access, repeated-instance isolation,
and zero filesystem/journal usage. This work does not change the in-memory
default: `MemoryEngine` remains the runtime default until repeatable
five-run results, bounded memory behavior, and the complete reliability gates
are reviewed. Phase 7 remains the separate default-adoption decision.

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
