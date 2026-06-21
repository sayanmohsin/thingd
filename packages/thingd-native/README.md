# @thingd/native

[![npm](https://img.shields.io/npm/v/@thingd/native?label=@thingd/native&logo=npm&color=ff6a00)](https://www.npmjs.com/package/@thingd/native)

Native Node.js binding for [thingd](https://github.com/sayanmohsin/thingd) — wraps `crates/thingd` (published on crates.io as `thingd`).

This package is an internal dependency of `@thingd/sdk` and provides the native SQLite-backed store via napi-rs. You don't need to install it directly — `@thingd/sdk` pulls it in automatically when using `driver: "native"`.
