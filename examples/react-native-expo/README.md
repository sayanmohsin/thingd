# React Native / Expo app backend example

This example uses the public `@thingd/client` transport directly from an Expo
application. There is no application server in the mobile project.

```bash
pnpm dlx create-expo-app my-thingd-app
cd my-thingd-app
pnpm add @thingd/client
cp /path/to/thingd/examples/react-native-expo/App.tsx App.tsx
```

```bash
EXPO_PUBLIC_THINGD_URL=https://your-thingd-cloud-host \
EXPO_PUBLIC_THINGD_PUBLISHABLE_KEY=pk_your_project_key \
EXPO_PUBLIC_DEMO_PASSWORD='use-a-local-demo-password' \
pnpm exec expo start
```

`EXPO_PUBLIC_DEMO_PASSWORD` is optional and is required only for the example
signup button. Use a throwaway local account password; never place a real
production credential in the example or an app bundle.

The publishable key is intended for app bundles. Store project-user access and
refresh tokens using Expo SecureStore in production; never put Cloud operator
tokens, runtime credentials, or project secret keys in the app.
