# Build performance

Thingd's persistent and native packages embed RocksDB. The first build after a
Rust, Cargo lockfile, RocksDB, or compiler change can therefore compile a
large C++ dependency graph. Subsequent builds reuse Cargo's target directory
and the CI cache.

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

On macOS, RocksDB bindgen also needs LLVM's `libclang`. Set paths for the local
installation, for example:

```bash
export LLVM_CONFIG_PATH="$(brew --prefix llvm)/bin/llvm-config"
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
export DYLD_LIBRARY_PATH="$(brew --prefix llvm)/lib${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
```

Linux CI installs `clang` and `libclang-dev`; release builds additionally use
Zig and `cargo-zigbuild` for the static musl targets.

## CI and release builds

Pull requests keep the Rust runtime and Node/native checks as separate
correctness gates. Their caches are keyed independently by job, platform,
architecture, and lockfile so concurrent jobs do not overwrite one another's
target cache.

The release workflow builds the static Docker server for `linux/amd64` and
`linux/arm64` in parallel, uploads the two binaries, and then assembles the
small scratch image. The Docker image contains no RocksDB service or shared
runtime library. A cache miss still requires a complete native RocksDB build.

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
