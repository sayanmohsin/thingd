# @sayanmohsin/memoryd

Node.js SDK for `memoryd`.

This package currently exposes the intended object/event/queue abstractions with an in-memory store. The durable Rust-backed engine will replace the default store once bindings are implemented.

Current SDK surface:

- object `put`, `get`, and `delete`
- event `append` and `list`
- basic search over objects and events
- queue `push`, `claim`, `ack`, `nack`, `list`, and `dead`

The in-memory store is for API design and local integration testing. It is not persistent storage.

`memoryd` is open source under the Apache-2.0 license.
