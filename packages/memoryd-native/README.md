# @sayanmohsin/memoryd-native

Native Node.js binding scaffold for `memoryd`.

This package is intentionally private and non-publishable right now. The public package remains `@sayanmohsin/memoryd`.

Planned direction:

```txt
@sayanmohsin/memoryd
  TypeScript public API
  MemoryStore interface
  loads native implementation when available

@sayanmohsin/memoryd-native
  napi-rs binding package
  wraps crates/memoryd-core
  exposes a native MemoryStore-compatible adapter
```

The native package should not define a separate public API. It should satisfy the same SDK behavior tested in `packages/memoryd/test`.

Do not publish this package until the native binding has real implementation, prebuild strategy, and CI coverage.
