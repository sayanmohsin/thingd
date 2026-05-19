# memoryd-core

Rust core primitives for `memoryd`.

This crate owns the storage boundary for object-shaped memory, append-only
events, and queues. The default engine is in-memory for API design and tests.
The optional `sqlite` feature enables the `rusqlite`-backed
`SqliteMemoryStore` for durable object, event, and queue storage.

Queue persistence in `SqliteMemoryStore` currently covers idempotent push,
delayed jobs, configurable lease expiration, ack, nack with retry delay, and
dead-letter state. SQLite schema version tracking is stored in
`memoryd_schema_migrations`. The private Node native adapter can use this store
locally, but prebuild and release packaging work is still needed before it
becomes the default public Node.js path.
