# memoryd-core

Rust core primitives for `memoryd`.

This crate owns the storage boundary for object-shaped memory, append-only
events, and queues. The default engine is in-memory for API design and tests.
The optional `sqlite` feature enables the `rusqlite`-backed
`SqliteMemoryStore` for durable object and event storage.

Queue persistence in `SqliteMemoryStore` is intentionally not implemented yet.
It is planned for the next phase so leases, retries, and dead-letter behavior
can be implemented transactionally.
