import { createThingdAppClient } from "@thingd/client";
import { useEffect, useState } from "react";
import { Button, SafeAreaView, Text, View } from "react-native";

const client = createThingdAppClient({
  baseUrl: process.env.EXPO_PUBLIC_THINGD_URL ?? "https://api.example.com",
  publishableKey: process.env.EXPO_PUBLIC_THINGD_PUBLISHABLE_KEY ?? "pk_replace_me",
});

export default function App() {
  const [message, setMessage] = useState("Ready");

  useEffect(() => {
    void client
      .manifest()
      .then((manifest) =>
        setMessage(`${manifest.project.slug}: ${manifest.functions.length} functions available`)
      )
      .catch(() => setMessage("Unable to reach thingd Cloud"));
  }, []);

  async function signUp() {
    setMessage("Creating account...");
    try {
      const session = await client.auth.signUp({
        email: "demo@example.com",
        password: "replace-with-a-real-password",
        name: "Demo User",
      });
      setMessage(`Signed in as ${session.user.name}`);
    } catch {
      setMessage("Signup failed");
    }
  }

  return (
    <SafeAreaView>
      <View style={{ padding: 24, gap: 16 }}>
        <Text>thingd app backend</Text>
        <Text>{message}</Text>
        <Button title="Sign up" onPress={() => void signUp()} />
      </View>
    </SafeAreaView>
  );
}
