# Nice Rep: CLI-first mobile setup

Nice Rep can use thingd Cloud as its application backend without a
nice-rep-owned API server for the normal mobile flow:

```text
Expo / React Native
  -> createThingdAppClient from @thingd/client
  -> thingd Cloud app backend
  -> Thingd runtime

thingd CLI
  -> Cloud project, instance, Publish app, and runtime-schema administration
```

The CLI uses an operator credential for administration. The mobile bundle uses
only a project publishable key and project-user session tokens. Never copy the
CLI token, a project secret API key, or a Thingd runtime token into an Expo
application.

This is an integration pattern, not a copy of Nice Rep's published app
definition or evidence that a Nice Rep Cloud instance is currently deployed.
The `users` and `workouts` schema below is intentionally small. Add the actual
collections and actions required by your app definition before bootstrapping.
Action names, collections, and record IDs in the client and smoke-test examples
are placeholders; replace them with values declared in that definition and
verify them against the target Cloud deployment.

## 1. Create the Cloud resources

Install the public CLI and log in as the operator who owns the project:

```bash
npm install -g @thingd/cli
thingd cloud login
thingd cloud project create nice-rep
thingd cloud instance create nice-rep nice-rep
thingd cloud instance use nice-rep nice-rep
```

If the project or instance already exists, use `thingd cloud project list` or
`thingd cloud instance list nice-rep` and continue with its ID or slug. The app
commands accept either form.

## 2. Create the declarative files

Create an app definition without overwriting an existing file:

```bash
thingd cloud app init \
  --file nice-rep.app.json \
  --name "Nice Rep" \
  --slug nice-rep
```

The generated file uses the public `thingd.app/v1` format. Edit its entities,
audiences, roles, canonical `actions`, views, workflows, integrations, policies,
distribution, and presentation fields. The CLI checks the public envelope and
Cloud performs the complete hosted policy validation.

`nice-rep.app.json` should remain version-controlled. It contains application
configuration and policy declarations, not credentials.

For the runtime data contract, create `schema.thingd`:

```thingd
version 1

project "nice-rep"

collection users {
  id: string @id
  email: string @unique @index
  displayName: string
}

collection workouts {
  id: string @id
  userId: string @index
  title: string
  completedAt: datetime?
}
```

Validate it locally before sending it to Cloud:

```bash
thingd schema check schema.thingd
```

Schema files are explicit runtime contracts. Thingd can still run without one;
using a schema is recommended when the mobile app and Cloud policy need a
reviewable data boundary.

## 3. Bootstrap idempotently

Preview project, instance, schema, and app resolution without creating or
updating Cloud resources:

```bash
thingd cloud app bootstrap \
  --project nice-rep \
  --instance nice-rep \
  --file nice-rep.app.json \
  --schema schema.thingd \
  --dry-run \
  --json
```

Apply the create-or-update operation after reviewing the preview:

```bash
thingd cloud app bootstrap \
  --project nice-rep \
  --instance nice-rep \
  --file nice-rep.app.json \
  --schema schema.thingd \
  --json
```

Bootstrap resolves the project and instance by ID or slug, validates the local
schema, asks Cloud to validate the runtime schema, and creates or updates the
Publish app by its instance-scoped slug. Re-running it does not create a
second app. Publishing is explicit:

```bash
thingd cloud app publish --project nice-rep --instance nice-rep --app nice-rep
```

Other lifecycle commands are available for testing, disabling, and rollback:

```bash
thingd cloud app validate --project nice-rep --instance nice-rep --app nice-rep
thingd cloud app test --project nice-rep --instance nice-rep --app nice-rep
thingd cloud app disable --project nice-rep --instance nice-rep --app nice-rep
thingd cloud app rollback --project nice-rep --instance nice-rep --app nice-rep --version 1
```

## 4. Configure Expo

Retrieve the mobile-safe configuration from Cloud:

```bash
thingd cloud app config --project nice-rep --instance nice-rep --json
```

The response includes `baseUrl`, `appBaseUrl`, `appEndpoint`, and
`publishableKey`. It does not print the operator token. Pass the API base URL
and publishable key to `createThingdAppClient`:

```bash
npm install @thingd/client
npx expo install expo-secure-store
```

```tsx
import * as SecureStore from "expo-secure-store";
import { createThingdAppClient } from "@thingd/client";

const client = createThingdAppClient({
  baseUrl: process.env.EXPO_PUBLIC_THINGD_URL!,
  publishableKey: process.env.EXPO_PUBLIC_THINGD_PUBLISHABLE_KEY!,
  expectedIdentity: {
    projectId: process.env.EXPO_PUBLIC_THINGD_PROJECT_ID!,
    appId: process.env.EXPO_PUBLIC_THINGD_APP_ID!,
    instanceId: process.env.EXPO_PUBLIC_THINGD_INSTANCE_ID!,
  },
  accessToken: await SecureStore.getItemAsync("thingd_access_token") ?? undefined,
  onSessionChange: (session) => {
    void Promise.all(session
      ? [
          SecureStore.setItemAsync("thingd_access_token", session.accessToken),
          SecureStore.setItemAsync("thingd_refresh_token", session.refreshToken),
        ]
      : [
          SecureStore.deleteItemAsync("thingd_access_token"),
          SecureStore.deleteItemAsync("thingd_refresh_token"),
        ]);
  },
});

// Verify project, app, and instance before auth or data requests.
const manifest = await client.manifest();

const session = await client.auth.signIn({
  email: "alice@example.com",
  password: passwordFromYourLoginForm,
});

const result = await client.actions.invoke("<declared-action-key>", {
  goal: "strength",
}, { idempotencyKey: `app-action:${session.user.id}:today` });
const usage = await client.actions.getUsage("<declared-action-key>");
```

On launch, validate `manifest()` before using stored session tokens. Check the
stored access token with `client.auth.getCurrentUser()`; if it is expired, call
`client.auth.refresh(refreshToken)` and persist the rotated session. Logout
clears both SecureStore values. Access and refresh tokens belong in platform
secure storage; the publishable key is the only credential intended for the
application bundle. Keep development and production IDs, URLs, and publishable
keys in separate Expo build environments.

The CLI smoke test exercises the same public client contract:

```bash
thingd cloud app smoke \
  --project nice-rep \
  --instance nice-rep \
  --file nice-rep.app.json \
  --email test@example.com \
  --password "$NICE_REP_SMOKE_PASSWORD" \
  --collection "<declared-collection>" \
  --object-id "<existing-readable-object-id>" \
  --action "<declared-action-key>" \
  --json
```

Replace the placeholders with an entity and action from the app definition,
plus an object that exists in the selected instance. The smoke command does not
create the app data for you.

## Credential boundaries

| Credential | Used by | Bundle-safe? |
|---|---|---|
| Cloud operator CLI token | `thingd cloud ...` administration | No |
| Project secret API key | trusted server-side automation | No |
| Project publishable key | `createThingdAppClient` in Expo/web | Yes, by design |
| Project-user access token | authenticated app requests | No; store securely |
| Thingd runtime token | direct engine/sidecar access | No |

If Nice Rep later needs payment webhooks, provider secrets, privileged
integrations, or trusted scheduled work that Cloud does not expose, add a
small server-side component for those responsibilities. That is separate from
the ordinary user-authenticated app-client flow.

## Release order

Release the public `@thingd/client`, `@thingd/cli`, and documentation contract
first. Then deploy the compatible Cloud API and verify the app smoke flow. The
GitHub Pages workflow builds and deploys this documentation from `main` after
the documentation PR is merged; it does not publish private Cloud credentials
or internal Cloud implementation details.

See [App Backend Contract](./app-backend.md) for the language-agnostic route
and header contract, and the [Expo example](https://github.com/sayanmohsin/thingd/tree/main/examples/react-native-expo)
for a minimal application.
