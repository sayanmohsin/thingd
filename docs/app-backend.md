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
| GET | `/app/actions/:name/usage` | Read the authenticated user's lifetime action quota |
| GET | `/app/objects/:collection/:id` | Read an allowed object |
| GET | `/app/objects/:collection` | List allowed objects with `limit` and `offset` pagination |
| POST | `/app/search` | Search allowed objects |

Requests use `X-Thingd-Publishable-Key`. Authenticated requests additionally
use `Authorization: Bearer <project-user-access-token>`. Mutating actions may
use `Idempotency-Key`.

Successful responses use `{ "data": ... }`. App errors use the stable envelope
`{ "error": { "code": "...", "message": "...", "details": [...] }, "requestId": "..." }`;
`details` and `requestId` are optional. The Cloud response also carries
`X-Request-Id` during migration. The SDK accepts older `{ "error": "code" }`
responses and reads request IDs from either location. Error messages and
details must not expose prompts, credentials, private records, or provider
responses.

The manifest is the published `thingd.app/v1` snapshot. It is identified by
the project, app, and instance and contains the app's entities, audiences,
roles, actions, views, policies, distribution, and presentation metadata.
`actions` is canonical. The SDK exposes `functions` as a compatibility alias
while older clients migrate; both names use the same routes and snapshot.
There is no second app-function registry: the checked-in app definition and its
published manifest are the sole action source.

## Named AI JSON actions

An action may declare provider-neutral `execution.kind: "ai_json"` policy.
The declaration contains an operation, system prompt, allowlisted input fields,
output schema (`outputSchema`), optional principal lifetime `usageLimit`,
bounded output-token limit, generated-ID paths, retrieval sources, and a
trusted write target. These server-side execution details are not returned in
the client manifest. The manifest exposes the action's public input/output
schemas and idempotency contract.

Retrieval accepts the existing single-source form (`collection`,
`queryFields`, `fields`, `limit`) and the `sources` array form, but not both at
once. A declaration may include up to 8 sources; each source may use up to 12
query fields, return up to 32 named fields, and request at most 50 results.
Cloud must check collection ownership and readable-field policy separately for
each source before using its records as model context. Local
`thingd cloud app check` validates deterministic shape, bounds, and declared
entity/field references; Cloud validation remains authoritative for the
published schema, policy, and authenticated ownership context.

An authenticated `GET /app/actions/:name/usage` response is:

```json
{
  "data": {
    "action": "generateWorkout",
    "limit": 3,
    "used": 1,
    "remaining": 2,
    "period": "lifetime"
  }
}
```

The route reports a principal-scoped lifetime limit declared by the action.
Actions without a configured limit do not have a usage summary. Usage
inspection does not reserve quota or guarantee that a subsequent invocation
will succeed.

## Access model

App clients can read public or user-owned objects. Collection listing and
object reads use the published entity policy: ownership is enforced by the
server and results contain only fields readable by the authenticated audience.
Listing is exact collection enumeration; use search only for full-text queries.
Sensitive writes go through published actions with input validation, ownership
checks, idempotency, and audit logging. Arbitrary customer code execution is
not part of this contract.

Cloud owns the hosted publication and policy implementation. The public
repository documents the client contract and CLI integration without exposing
Cloud-only operational details.

For a mobile-first walkthrough, see the [React Native / Expo example](https://github.com/sayanmohsin/thingd/tree/main/examples/react-native-expo).
