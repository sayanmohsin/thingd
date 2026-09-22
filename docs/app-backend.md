# thingd App Backend Contract

thingd Cloud can expose a project as an application backend for mobile and web
apps. The open-source `@thingd/client` package contains the portable REST
client; authentication, project users, policies, and hosted actions are
Cloud capabilities.

For the complete CLI-first Expo walkthrough, see [Nice Rep: CLI-first mobile
setup](./nice-rep.md). This page defines the public client contract shared by
that workflow and other hosted applications.

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

await client.actions.invoke("createProfile", { timezone: "America/Toronto" }, {
  idempotencyKey: "profile:create:alice",
});
```

Publishable keys are intended for browser and mobile applications. Never put a
Cloud secret API key or a thingd runtime token in an app bundle.

## Contract

Hosted app backends use these routes below `/v1`:

| Method | Route | Purpose |
|---|---|---|
| GET | `/app/manifest` | Discover the immutable project/app/instance snapshot |
| POST | `/app/auth/signup` | Create a project user |
| POST | `/app/auth/login` | Create a project-user session |
| POST | `/app/auth/refresh` | Rotate a project-user refresh token |
| GET | `/app/auth/me` | Read the authenticated project user |
| POST | `/app/auth/logout` | Revoke the project user's refresh sessions |
| GET | `/app/functions` | List published actions (compatibility route) |
| GET | `/app/functions/:name` | Read one action definition (compatibility route) |
| POST | `/app/functions/:name` | Invoke an action (compatibility route) |
| GET | `/app/objects/:collection/:id` | Read an allowed object |
| POST | `/app/search` | Search allowed objects |

Requests use `X-Thingd-Publishable-Key`. Authenticated requests additionally
use `Authorization: Bearer <project-user-access-token>`. Mutating actions may
use `Idempotency-Key`.

Responses use `{ "data": ... }`. Errors contain a stable `error.code`, a safe
message, and may include a `requestId` for support.

The manifest is the published `thingd.app/v1` snapshot. It is identified by
the project, app, and instance and contains the app's entities, audiences,
roles, actions, views, policies, distribution, and presentation metadata.
`actions` is canonical. The SDK exposes `functions` as a compatibility alias
while older clients migrate; both names use the same routes and snapshot.
There is no second app-function registry: the checked-in app definition and its
published manifest are the sole action source.

## Access model

App clients can read public or user-owned objects. Sensitive writes go through
published actions with input validation, ownership checks, idempotency,
and audit logging. Arbitrary customer code execution is not part of this
contract.

Cloud owns the hosted publication and policy implementation. The public
repository documents the client contract and CLI integration without exposing
Cloud-only operational details.

For a mobile-first walkthrough, see the [React Native / Expo example](https://github.com/sayanmohsin/thingd/tree/main/examples/react-native-expo).
