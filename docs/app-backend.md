# thingd App Backend Contract

thingd Cloud can expose a project as an application backend for mobile and web
apps. The open-source `@thingd/client` package contains the portable REST
client; authentication, project users, policies, and hosted named actions are
Cloud capabilities.

## Client setup

```ts
import { createThingdAppClient } from "@thingd/client";

const client = createThingdAppClient({
  baseUrl: "https://api.thingd.cloud",
  publishableKey: "pk_...",
  accessToken: await SecureStore.getItemAsync("thingd_access_token") ?? undefined,
  onSessionChange: (session) => {
    void SecureStore.setItemAsync("thingd_access_token", session?.accessToken ?? "");
  },
});

const result = await client.auth.signUp({
  email: "alice@example.com",
  password: "correct horse battery staple",
  name: "Alice",
});

await client.functions.invoke("createProfile", { timezone: "America/Toronto" }, {
  idempotencyKey: "profile:create:alice",
});
```

Publishable keys are intended for browser and mobile applications. Never put a
Cloud secret API key or a thingd runtime token in an app bundle.

## Contract

Hosted app backends use these routes below `/v1`:

| Method | Route | Purpose |
|---|---|---|
| GET | `/app/manifest` | Discover project capabilities and function versions |
| POST | `/app/auth/signup` | Create a project user |
| POST | `/app/auth/login` | Create a project-user session |
| POST | `/app/auth/refresh` | Rotate a project-user refresh token |
| GET | `/app/auth/me` | Read the authenticated project user |
| POST | `/app/auth/logout` | Revoke the project user's refresh sessions |
| GET | `/app/functions` | List published named actions |
| GET | `/app/functions/:name` | Read one action definition |
| POST | `/app/functions/:name` | Invoke a named action |
| GET | `/app/objects/:collection/:id` | Read an allowed object |
| POST | `/app/search` | Search allowed objects |

Requests use `X-Thingd-Publishable-Key`. Authenticated requests additionally
use `Authorization: Bearer <project-user-access-token>`. Mutating actions may
use `Idempotency-Key`.

Responses use `{ "data": ... }`. Errors contain a stable `error.code`, a safe
message, and may include a `requestId` for support.

## Access model

App clients can read public or user-owned objects. Sensitive writes go through
published named actions with input validation, ownership checks, idempotency,
and audit logging. Arbitrary customer code execution is not part of this
contract.

See the private Cloud Serve plan for publication and policy implementation.

For a mobile-first walkthrough, see the [React Native / Expo example](../examples/react-native-expo/README.md).
