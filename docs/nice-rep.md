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
audiences, roles, named actions, views, workflows, integrations, policies,
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
Publish app by its slug. Re-running it does not create a second app. Publishing
is explicit:

```bash
thingd cloud app publish --project nice-rep --app nice-rep
```

Other lifecycle commands are available for testing, disabling, and rollback:

```bash
thingd cloud app validate --project nice-rep --app nice-rep
thingd cloud app test --project nice-rep --app nice-rep
thingd cloud app disable --project nice-rep --app nice-rep
thingd cloud app rollback --project nice-rep --app nice-rep --version 1
```

Named app-backend functions can be managed from the same CLI:

```bash
thingd cloud app functions list --project nice-rep
thingd cloud app functions create --project nice-rep --file function.json
thingd cloud app functions update --project nice-rep --name generateWorkout --file function.json
thingd cloud app functions test --project nice-rep --name generateWorkout
thingd cloud app functions publish --project nice-rep --name generateWorkout
```

## 4. Configure Expo

Retrieve the mobile-safe configuration from Cloud:

```bash
thingd cloud app config --project nice-rep --json
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
  accessToken: undefined,
  onSessionChange: (session) => {
    void (session
      ? SecureStore.setItemAsync("thingd_access_token", session.accessToken)
      : SecureStore.deleteItemAsync("thingd_access_token"));
  },
});

const session = await client.auth.signIn({
  email: "alice@example.com",
  password: passwordFromYourLoginForm,
});

const workout = await client.functions.invoke("generateWorkout", {
  goal: "strength",
}, { idempotencyKey: `generate-workout:${session.user.id}:today` });
```

Load a previously stored access token before creating the client in a real
application. Access and refresh tokens belong in platform secure storage; the
publishable key is the only credential intended for the application bundle.

The CLI smoke test exercises the same public client contract:

```bash
thingd cloud app smoke \
  --project nice-rep \
  --file nice-rep.app.json \
  --email test@example.com \
  --password "$NICE_REP_SMOKE_PASSWORD" \
  --json
```

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
