# @thingd/native

[![npm](https://img.shields.io/npm/v/@thingd/native?label=@thingd/native&logo=npm&color=ff6a00)](https://www.npmjs.com/package/@thingd/native)

Native Node.js binding for [thingd](https://github.com/sayanmohsin/thingd) — wraps `crates/thingd` (published on crates.io as `thingd`).

This package is an internal dependency of `@thingd/sdk` and provides the native persistent store via napi-rs. You don't need to install it directly — `@thingd/sdk` pulls it in automatically when using `driver: "native"`.

The addon embeds the RocksDB durable backend; it does not connect to a
database service or require a companion container. The public SDK store
contract remains unchanged.

The native open boundary accepts an optional 64-character hexadecimal key for
authenticated encrypted storage. The key is validated before opening the Rust
engine. Missing or wrong keys are reported as stable open errors without
including key material. MCP and REST callers do not pass this key in requests;
the host process supplies it during startup.
