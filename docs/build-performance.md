# Build performance

Thingd's persistent and native packages may embed RocksDB. The first build
after a Rust, Cargo lockfile, RocksDB, or compiler change can therefore compile
a large C++ dependency graph. Subsequent builds reuse Cargo's target directory
and the CI cache. ThingDB RAM does not require RocksDB or perform filesystem I/O.

## Scoped local commands

Use the narrowest command that matches the change:

```bash
# Core engine only
pnpm rust:check:core

# HTTP/MCP server only
pnpm rust:check:server
pnpm rust:build:server

# Native addon only
pnpm rust:build:native

# Inspect compiler timing for a server build
pnpm rust:build:timings
```

Use `pnpm verify:pr` or the full workspace commands before merging. Scoped
commands are for iteration; they do not replace the required CI suites.

## Optional compiler cache

Developers who rebuild frequently may install `sccache` and point Cargo at it:

```bash
export RUSTC_WRAPPER="$(command -v sccache)"
export SCCACHE_DIR="$HOME/.cache/sccache"
```

This is optional. Thingd does not require a cache service at runtime, and no
cache credentials or machine-specific compiler paths belong in the repository.

On macOS, a source build using the RocksDB backend also needs LLVM's `libclang`.
Inspect the local toolchain first:

```bash
pnpm check:native-toolchain
```

If a source build is required, set paths for one matching LLVM installation:

```bash
export LLVM_CONFIG_PATH="$(brew --prefix llvm)/bin/llvm-config"
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
```

Do not set `DYLD_LIBRARY_PATH` globally while invoking `rustc`; it can make
`rustc` load a different `libLLVM` from the one it was built with. The
diagnostic command reports this condition and mismatched LLVM prefixes.

Linux CI installs `clang` and `libclang-dev`; release builds additionally use
Zig and `cargo-zigbuild` for the static musl targets. Node.js users normally
receive a prebuilt `@thingd/native` artifact and do not need these tools.

## CI and release builds

Pull requests keep the Rust runtime and Node/native checks as separate
correctness gates. Their caches are keyed independently by job, platform,
architecture, and lockfile so concurrent jobs do not overwrite one another's
target cache.

The release workflow builds the static Docker server for `linux/amd64` and
`linux/arm64` in parallel, uploads the two binaries, and then assembles the
small scratch image. The Docker image contains no RocksDB service or shared
runtime library. A cache miss for a source build still requires a complete
native RocksDB build. ThingDB-only Rust builds can use the additive
`thingdb-backend` feature and do not compile `librocksdb-sys`; the compatibility
`persistent` feature continues to enable both durable backends.

Docker binary compilation starts after the release tag is available and does
not wait for npm publication. Publication jobs remain independently gated by
their own credentials and verification steps.

## Troubleshooting slow builds

1. Run `pnpm rust:build:timings` and inspect the generated Cargo timing report.
2. Confirm `LLVM_CONFIG_PATH` and `LIBCLANG_PATH` point to the same LLVM
   installation.
3. Check whether the lockfile or Rust toolchain changed, which invalidates
   shared CI caches.
4. Use scoped commands while iterating, then run the full validation command
   before opening a pull request.
