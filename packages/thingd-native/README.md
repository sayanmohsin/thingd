# thingd-native

Native Node.js binding for `thingd` — a fast object-first data engine for applications and AI agents.

This package provides the native SQLite persistence layer for the thingd SDK. It is currently private and opt-in, loaded only when `driver: "native"` is requested.

This package is intentionally private and non-publishable right now. The public package remains `thingd`.

Current shape:

```txt
thingd
  TypeScript public API
  ThingStore interface
  loads this package only for driver: "native"

thingd-native
  napi-rs binding package
  wraps crates/thingd-core
  exposes low-level JSON bridge methods
```

Build locally:

```bash
pnpm --filter thingd-native build
```

Then use it through the public SDK:

```ts
import { ThingD } from "thingd";

const db = await ThingD.open({
  path: "./thingd.db",
  driver: "native",
});
```

The native package should not define a separate app-facing API. It should satisfy the same SDK behavior tested in `packages/thingd/test`.

Do not publish this package until it has a prebuild strategy, migration story, and CI coverage for supported Node.js/platform combinations.
