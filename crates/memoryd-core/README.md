# memoryd-core

Rust core primitives for `memoryd`.

This crate owns the storage boundary for object-shaped memory, append-only
events, and queues. The default engine is in-memory for API design and tests.
The optional `sqlite` feature enables the `rusqlite`-backed
`SqliteMemoryStore` for durable object, event, and queue storage.

Queue persistence in `SqliteMemoryStore` currently covers idempotent push,
delayed jobs, configurable lease expiration, ack, nack with retry delay, and
dead-letter state. The Node native adapter is still planned before this durable
store becomes the default public Node.js path.
