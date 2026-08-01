# React Native / Expo app backend example

This example uses the public `@thingd/client` transport directly from an Expo
application. There is no application server in the mobile project.

```bash
npx create-expo-app my-thingd-app
cd my-thingd-app
npm install @thingd/client
cp /path/to/thingd/examples/react-native-expo/App.tsx App.tsx
```

```bash
EXPO_PUBLIC_THINGD_URL=https://your-thingd-cloud-host \
EXPO_PUBLIC_THINGD_PUBLISHABLE_KEY=pk_your_project_key \
npx expo start
```

The publishable key is intended for app bundles. Store project-user access and
refresh tokens using Expo SecureStore in production; never put Cloud operator
tokens, runtime credentials, or project secret keys in the app.
