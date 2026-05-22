# thingd-core

Rust core primitives for `thingd`.

This crate owns the storage boundary for object-shaped memory, append-only
events, and queues. The default engine is in-memory for API design and tests.
The optional `sqlite` feature enables the `rusqlite`-backed
`SqliteThingStore` for durable object, event, and queue storage.

Queue persistence in `SqliteThingStore` currently covers idempotent push,
delayed jobs, configurable lease expiration, ack, nack with retry delay, and
dead-letter state. SQLite schema version tracking is stored in
`thingd_schema_migrations`. The private Node native adapter can use this store
locally, but prebuild and release packaging work is still needed before it
becomes the default public Node.js path.
