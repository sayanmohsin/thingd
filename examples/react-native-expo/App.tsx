import { createThingdAppClient, ThingdAppError } from "@thingd/client";
import * as SecureStore from "expo-secure-store";
import { useEffect, useState } from "react";
import { Button, SafeAreaView, Text, View } from "react-native";

const ACCESS_TOKEN_KEY = "thingd_access_token";
const REFRESH_TOKEN_KEY = "thingd_refresh_token";

function requiredEnvironmentVariable(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`Set ${name} before starting the Expo app.`);
  }
  return value;
}

const client = createThingdAppClient({
  baseUrl: process.env.EXPO_PUBLIC_THINGD_URL ?? "https://api.example.com",
  publishableKey: process.env.EXPO_PUBLIC_THINGD_PUBLISHABLE_KEY ?? "pk_replace_me",
  expectedIdentity: {
    projectId: requiredEnvironmentVariable("EXPO_PUBLIC_THINGD_PROJECT_ID"),
    appId: requiredEnvironmentVariable("EXPO_PUBLIC_THINGD_APP_ID"),
    instanceId: requiredEnvironmentVariable("EXPO_PUBLIC_THINGD_INSTANCE_ID"),
  },
  onSessionChange: (session) => {
    void Promise.all(
      session
        ? [
            SecureStore.setItemAsync(ACCESS_TOKEN_KEY, session.accessToken),
            SecureStore.setItemAsync(REFRESH_TOKEN_KEY, session.refreshToken),
          ]
        : [
            SecureStore.deleteItemAsync(ACCESS_TOKEN_KEY),
            SecureStore.deleteItemAsync(REFRESH_TOKEN_KEY),
          ]
    );
  },
});

function errorMessage(error: unknown): string {
  if (error instanceof ThingdAppError) {
    return error.requestId
      ? `${error.code} (${error.requestId}): ${error.message}`
      : `${error.code}: ${error.message}`;
  }
  return "The app request could not be completed.";
}

export default function App() {
  const [message, setMessage] = useState("Checking app configuration…");
  const [ready, setReady] = useState(false);

  useEffect(() => {
    let active = true;
    async function restoreSession() {
      try {
        const manifest = await client.manifest();
        if (!active) {
          return;
        }
        setMessage(
          `${manifest.project.slug}/${manifest.instance.slug}: ${manifest.actions.length} actions`
        );

        const [accessToken, refreshToken] = await Promise.all([
          SecureStore.getItemAsync(ACCESS_TOKEN_KEY),
          SecureStore.getItemAsync(REFRESH_TOKEN_KEY),
        ]);
        if (accessToken) {
          client.setAccessToken(accessToken);
          try {
            const user = await client.auth.getCurrentUser();
            if (active) {
              setMessage(`Signed in as ${user.name}`);
            }
          } catch {
            if (refreshToken) {
              const session = await client.auth.refresh(refreshToken);
              if (active) {
                setMessage(`Session restored for ${session.user.name}`);
              }
            } else {
              client.setAccessToken(undefined);
              await Promise.all([
                SecureStore.deleteItemAsync(ACCESS_TOKEN_KEY),
                SecureStore.deleteItemAsync(REFRESH_TOKEN_KEY),
              ]);
            }
          }
        } else if (refreshToken) {
          const session = await client.auth.refresh(refreshToken);
          if (active) {
            setMessage(`Session restored for ${session.user.name}`);
          }
        }
      } catch (error) {
        if (active) {
          setMessage(errorMessage(error));
        }
      } finally {
        if (active) {
          setReady(true);
        }
      }
    }
    void restoreSession();
    return () => {
      active = false;
    };
  }, []);

  async function signUp() {
    const password = process.env.EXPO_PUBLIC_DEMO_PASSWORD;
    if (!password) {
      setMessage("Set EXPO_PUBLIC_DEMO_PASSWORD to try the demo signup.");
      return;
    }
    setMessage("Creating account…");
    try {
      const session = await client.auth.signUp({
        email: "demo@example.com",
        password,
        name: "Demo User",
      });
      setMessage(`Signed in as ${session.user.name}`);
    } catch (error) {
      setMessage(errorMessage(error));
    }
  }

  async function signOut() {
    try {
      await client.auth.signOut();
      setMessage("Signed out");
    } catch (error) {
      setMessage(errorMessage(error));
    }
  }

  async function showUsage() {
    try {
      const usage = await client.actions.getUsage("generate_workout");
      setMessage(`${usage.remaining} of ${usage.limit} workout generations remain`);
    } catch (error) {
      setMessage(errorMessage(error));
    }
  }

  async function runNamedAction() {
    try {
      const user = await client.auth.getCurrentUser();
      const result = await client.actions.invoke(
        process.env.EXPO_PUBLIC_DEMO_ACTION ?? "generate_workout",
        { goal: "strength" },
        { idempotencyKey: `expo-demo:${user.id}:strength:${new Date().toISOString().slice(0, 10)}` }
      );
      setMessage(`Action completed: ${JSON.stringify(result)}`);
    } catch (error) {
      setMessage(errorMessage(error));
    }
  }

  return (
    <SafeAreaView>
      <View style={{ padding: 24, gap: 16 }}>
        <Text>thingd app backend</Text>
        <Text>{message}</Text>
        <Button title="Sign up" onPress={() => void signUp()} disabled={!ready} />
        <Button title="Run named action" onPress={() => void runNamedAction()} disabled={!ready} />
        <Button title="Check action usage" onPress={() => void showUsage()} disabled={!ready} />
        <Button title="Sign out" onPress={() => void signOut()} disabled={!ready} />
      </View>
    </SafeAreaView>
  );
}
