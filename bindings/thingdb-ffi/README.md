# ThingDB C ABI

`thingdb-ffi` exposes the embedded ThingDB engine through a C ABI. It does not
start a server or require Thingd.

Build the shared or static library and include `include/thingdb.h`:

```sh
cargo build --release -p thingdb-ffi
```

The Rust artifacts are created under `target/release`. Link the library using
your platform's native linker configuration. Every input is a pointer/length
pair; paths and keyspace names must contain UTF-8, while keys and values accept
arbitrary bytes. `ThingDbBuffer` values returned by `thingdb_get` must be
released with `thingdb_buffer_free`; never free them with another allocator.
Status codes are documented but may change before ThingDB 1.0; the durable
file format and ABI remain experimental until then.

The API supports opening a RAM or durable database, byte-oriented get/put/
delete, and explicit close. Durable paths are single-process; calls on a
database handle may be made concurrently, but callers must not close a handle
while another thread is using it.
