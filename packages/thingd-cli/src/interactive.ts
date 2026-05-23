import * as p from "@clack/prompts";
import pc from "picocolors";
import { runCli } from "./index.js";

function getCancelMessage(): never {
  p.cancel("Operation cancelled.");
  process.exit(0);
}

export async function runInteractiveCli(): Promise<void> {
  console.clear();
  p.intro(`${pc.bgCyan(pc.black(" thingd "))} Interactive CLI`);

  const driver = await p.select({
    message: "Which environment do you want to connect to?",
    options: [
      { value: "memory", label: "Memory", hint: "Ephemeral, destroyed on exit" },
      { value: "native", label: "Native", hint: "Local SQLite file" },
      { value: "cloud", label: "Cloud", hint: "Connect to a remote thingd instance" },
    ],
  });

  if (p.isCancel(driver)) getCancelMessage();

  const env: Record<string, string | undefined> = { ...process.env };
  env.THINGD_DRIVER = driver as string;

  if (driver === "cloud") {
    const url = await p.text({
      message: "Enter the Cloud URL:",
      placeholder: "http://localhost:3000",
      defaultValue: "http://localhost:3000",
    });
    if (p.isCancel(url)) getCancelMessage();
    env.THINGD_URL = url as string;

    const token = await p.password({
      message: "Enter the Bearer Token (optional):",
    });
    if (p.isCancel(token)) getCancelMessage();
    if (token) {
      env.THINGD_AUTH_TOKEN = token as string;
    }
  } else if (driver === "native") {
    const dbPath = await p.text({
      message: "Enter the local database path:",
      placeholder: "./data.db",
      defaultValue: "./data.db",
    });
    if (p.isCancel(dbPath)) getCancelMessage();
    env.THINGD_PATH = dbPath as string;
  }

  const feature = await p.select({
    message: "What do you want to explore?",
    options: [
      { value: "objects", label: "📦 Objects", hint: "Manage JSON documents" },
      { value: "events", label: "🌊 Events", hint: "Append-only streams" },
      { value: "queues", label: "🚦 Queues", hint: "Background job processing" },
      { value: "search", label: "🔍 Search", hint: "Global vector and text search" },
      { value: "status", label: "ℹ️ Status", hint: "Check connection status" },
    ],
  });

  if (p.isCancel(feature)) getCancelMessage();

  const args: string[] = [feature as string];

  if (feature === "status") {
    // No extra args needed
  } else if (feature === "search") {
    const query = await p.text({
      message: "Enter your search query:",
    });
    if (p.isCancel(query)) getCancelMessage();
    args.push(query as string);
  } else if (feature === "objects") {
    const action = await p.select({
      message: "What action?",
      options: [
        { value: "get", label: "Get Object" },
        { value: "put", label: "Put (Create/Update) Object" },
        { value: "delete", label: "Delete Object" },
      ],
    });
    if (p.isCancel(action)) getCancelMessage();
    args.push(action as string);

    const collection = await p.text({
      message: "Collection name:",
    });
    if (p.isCancel(collection)) getCancelMessage();
    args.push(collection as string);

    const id = await p.text({
      message: "Object ID:",
    });
    if (p.isCancel(id)) getCancelMessage();
    args.push(id as string);

    if (action === "put") {
      const data = await p.text({
        message: "JSON Data:",
        placeholder: '{"key":"value"}',
      });
      if (p.isCancel(data)) getCancelMessage();
      args.push("--data", data as string);
    }
  } else if (feature === "events") {
    const action = await p.select({
      message: "What action?",
      options: [
        { value: "list", label: "List Events" },
        { value: "append", label: "Append Event" },
      ],
    });
    if (p.isCancel(action)) getCancelMessage();
    args.push(action as string);

    const stream = await p.text({
      message: "Stream name:",
    });
    if (p.isCancel(stream)) getCancelMessage();
    args.push(stream as string);

    if (action === "append") {
      const type = await p.text({
        message: "Event Type:",
        placeholder: "user.created",
      });
      if (p.isCancel(type)) getCancelMessage();
      args.push(type as string);

      const data = await p.text({
        message: "JSON Data:",
        placeholder: '{"key":"value"}',
      });
      if (p.isCancel(data)) getCancelMessage();
      if (data) args.push("--data", data as string);
    }
  } else if (feature === "queues") {
    const action = await p.select({
      message: "What action?",
      options: [
        { value: "list", label: "List Active Jobs" },
        { value: "push", label: "Push Job" },
        { value: "claim", label: "Claim Job" },
        { value: "ack", label: "Ack Job" },
        { value: "nack", label: "Nack Job" },
      ],
    });
    if (p.isCancel(action)) getCancelMessage();
    args.push(action as string);

    const queue = await p.text({
      message: "Queue name:",
    });
    if (p.isCancel(queue)) getCancelMessage();
    args.push(queue as string);

    if (action === "push") {
      const payload = await p.text({
        message: "JSON Payload:",
        placeholder: '{"key":"value"}',
      });
      if (p.isCancel(payload)) getCancelMessage();
      args.push("--payload", payload as string);
    } else if (action === "ack" || action === "nack") {
      const jobId = await p.text({
        message: "Job ID:",
      });
      if (p.isCancel(jobId)) getCancelMessage();
      args.push(jobId as string);
    }
  }

  p.outro(`Running: ${pc.cyan(`thingd ${args.join(" ")}`)}`);
  
  // Forward to existing CLI handler with pretty print enabled by default for human eyes
  args.push("--pretty");

  const exitCode = await runCli(args, { env });
  process.exit(exitCode);
}
