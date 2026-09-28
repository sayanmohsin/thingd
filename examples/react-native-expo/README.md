# React Native / Expo app backend example

This example uses the public `@thingd/client` transport directly from an Expo
application. There is no application server in the mobile project.

For a complete project setup, first follow the [Nice Rep CLI-first mobile
guide](../../docs/nice-rep.md) to create the project, instance, declarative
`nice-rep.app.json`, and `schema.thingd`. This example starts at the Expo
client step.

```bash
pnpm dlx create-expo-app my-thingd-app
cd my-thingd-app
pnpm add @thingd/client
npx expo install expo-secure-store
cp /path/to/thingd/examples/react-native-expo/App.tsx App.tsx
```

```bash
EXPO_PUBLIC_THINGD_URL=https://your-thingd-cloud-host \
EXPO_PUBLIC_THINGD_PUBLISHABLE_KEY=pk_your_project_key \
EXPO_PUBLIC_THINGD_PROJECT_ID=your_project_id \
EXPO_PUBLIC_THINGD_APP_ID=your_app_id \
EXPO_PUBLIC_THINGD_INSTANCE_ID=your_development_instance_id \
EXPO_PUBLIC_DEMO_ACTION=generateWorkout \
pnpm exec expo start
```

Set the expected IDs from `thingd cloud app config` or the published app
manifest. The example checks the manifest identity before restoring a session
or calling authenticated app routes. Keep development and production values in
separate Expo environments; use the production IDs and publishable key only in
the production build profile.

The example signup form asks for a demo email and password at runtime. Use a
throwaway account; do not reuse production credentials.

The example restores the access token from SecureStore, refreshes an expired
session with the stored refresh token, and clears both tokens on logout. It
also demonstrates named action invocation with a stable per-user daily
idempotency key, safe error codes/request IDs, and authenticated usage reads.
The publishable key is intended for app bundles. Never put Cloud operator
tokens, runtime credentials, or project secret keys in the app.
