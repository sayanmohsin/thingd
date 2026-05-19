# memoryd-core

Rust core primitives for `memoryd`.

This crate owns the storage boundary for object-shaped memory, append-only
events, and queues. The default engine is in-memory for API design and tests.
The optional `sqlite` feature enables the `rusqlite`-backed
`SqliteMemoryStore` for durable object, event, and queue storage.

Queue persistence in `SqliteMemoryStore` currently covers idempotent push,
claim, ack, nack, retry, and dead-letter state. Delayed jobs and configurable
lease expiration are planned for the Rust model/API alignment work before the
Node native adapter becomes the default store.
