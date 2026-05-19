# @sayanmohsin/memoryd-native

Private native Node.js binding for `memoryd`.

This package is intentionally private and non-publishable right now. The public package remains `@sayanmohsin/memoryd`.

Current shape:

```txt
@sayanmohsin/memoryd
  TypeScript public API
  MemoryStore interface
  loads this package only for driver: "native"

@sayanmohsin/memoryd-native
  napi-rs binding package
  wraps crates/memoryd-core
  exposes low-level JSON bridge methods
```

Build locally:

```bash
pnpm --filter @sayanmohsin/memoryd-native build
```

Then use it through the public SDK:

```ts
import { MemoryD } from "@sayanmohsin/memoryd";

const db = await MemoryD.open({
  path: "./memoryd.db",
  driver: "native",
});
```

The native package should not define a separate app-facing API. It should satisfy the same SDK behavior tested in `packages/memoryd/test`.

Do not publish this package until it has a prebuild strategy, migration story, and CI coverage for supported Node.js/platform combinations.
