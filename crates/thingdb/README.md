# thingdb

Experimental Rust-native durable ordered key-value storage for Thingd.

The crate is intentionally opt-in while its WAL, table format, recovery,
compaction, and large-store behavior are being validated. RocksDB remains
Thingd's default production backend.

The crate also provides `MemoryCache`, a separate bounded RAM-only cache for
process-local read-through use cases. It supports byte values, TTL expiration,
LRU eviction, and cache diagnostics. It does not create durable files and does
not change Thingd's default in-memory or durable backend.

Current lifecycle: Phase 0, experimental foundation and compatibility. The
next phases are bounded incremental tables and compaction, crash/corruption
hardening, differential and fuzz testing, large-data benchmarking, and limited
opt-in soak testing. See the [storage backend phase plan](../../docs/storage-backends.md)
and [benchmark plan](../../docs/benchmarks.md).
