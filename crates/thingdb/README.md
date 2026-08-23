# thingdb

Experimental Rust-native durable ordered key-value storage for Thingd.

The crate is intentionally opt-in while its WAL, table format, recovery,
compaction, and large-store behavior are being validated. RocksDB remains
Thingd's default production backend.

The crate also provides `MemoryCache`, a separate bounded RAM-only cache for
process-local read-through use cases. It supports byte values, TTL expiration,
LRU eviction, and cache diagnostics. It does not create durable files or change
Thingd's durable backend selection.

Current lifecycle: the durable engine has completed its initial WAL, recovery,
table-layer, bounded-memtable, and layered-read milestones and is entering
large-data performance validation. It remains experimental and must not be the
only copy of important production data. See the [storage backend phase plan](../../docs/storage-backends.md)
and [benchmark plan](../../docs/benchmarks.md).
