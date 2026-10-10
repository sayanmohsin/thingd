# @thingd/native

[![npm](https://img.shields.io/npm/v/@thingd/native?label=@thingd/native&logo=npm&color=ff6a00)](https://www.npmjs.com/package/@thingd/native)

Native Node.js binding for [thingd](https://github.com/sayanmohsin/thingd) — wraps `crates/thingd` (published on crates.io as `thingd`).

This package is an internal dependency of `@thingd/sdk` and provides the native persistent store via napi-rs. You don't need to install it directly — `@thingd/sdk` pulls it in automatically when using `driver: "native"`.

The addon uses ThingDB RAM for `:memory:` and empty-path operation without
creating temporary files. Real filesystem paths use the RocksDB durable backend
by default; set `THINGD_STORAGE_BACKEND=thingdb` to opt into the experimental
Rust-native durable backend. Published packages include prebuilt native
artifacts for supported targets, so consumers normally do not install LLVM,
Clang, or RocksDB locally. The public SDK store contract remains unchanged.
Local native builds on macOS 26+ select a compatible deployment target when
`MACOSX_DEPLOYMENT_TARGET` is unset. This works around
[rust-lang/rust#157750](https://github.com/rust-lang/rust/issues/157750), where
the macOS loader rejects a misaligned native-library string table. An explicit
deployment target is always respected.

The native open boundary accepts an optional 64-character hexadecimal key for
authenticated encrypted storage. The key is validated before opening the Rust
engine. Missing or wrong keys are reported as stable open errors without
including key material. MCP and REST callers do not pass this key in requests;
the host process supplies it during startup.

## Rust build features

The Cargo package preserves its existing default build: RocksDB and ThingDB
backends, search, and vectors are enabled. Applications embedding this crate
can choose a smaller backend set explicitly:

```toml
thingd-native = { version = "0.91", default-features = false, features = [
  "thingdb-backend",
  "search",
  "vectors",
] }
```

That ThingDB-only build does not compile `librocksdb-sys` or require the
RocksDB/LLVM/Clang toolchain. A filesystem-backed open must select
`THINGD_STORAGE_BACKEND=thingdb`; the runtime still defaults to RocksDB for
durable paths and returns an explicit unavailable-backend error if RocksDB was
not compiled in. The `:memory:` path continues to use ThingDB RAM mode. To keep
the historical durable backend, use the default features or explicitly enable
`rocksdb-backend`. That feature also enables ThingDB support because the shared
Thingd engine uses ThingDB for its RAM implementation.
