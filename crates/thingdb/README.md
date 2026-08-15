# thingdb

Experimental Rust-native durable ordered key-value storage for Thingd.

The crate is intentionally opt-in while its WAL, table format, recovery,
compaction, and large-store behavior are being validated. RocksDB remains
Thingd's default production backend.
