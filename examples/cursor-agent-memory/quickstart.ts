import { ThingD } from "thingd";

// Formatted logger helper
function log(step: string, message: string, data?: unknown) {
  const blue = "\x1b[34m";
  const green = "\x1b[32m";
  const reset = "\x1b[0m";

  console.log(`\n${blue}=== [Step: ${step}] ===${reset}`);
  console.log(`${green}${message}${reset}`);
  if (data !== undefined) {
    console.log(JSON.stringify(data, null, 2));
  }
}

async function main() {
  console.log("\n🚀 Starting thingd Agent long-term memory Quickstart...");

  // 1. Open database (driver auto-promotes to "native" since it's a persistent file)
  const db = await ThingD.open({
    path: "./agent_memory.db",
    driver: "native",
  });
  log(
    "1. Database Open",
    "Opened persistent thingd instance using automatic native driver promotion.",
  );

  // 2. Put memory records
  const mem1 = await db.put("memories", {
    id: "learn-rust",
    text: "Rust is a systems programming language that provides memory safety and high performance.",
    category: "programming",
    status: "completed",
    priority: 1,
  });

  const mem2 = await db.put("memories", {
    id: "learn-typescript",
    text: "TypeScript adds optional static typing to JavaScript to help build larger apps.",
    category: "programming",
    status: "active",
    priority: 2,
  });

  log("2. Store Memories", "Stored two structured memory objects with custom metadata fields:", {
    mem1,
    mem2,
  });

  // 3. Perform Full-Text Stemming Search
  // "learning" will automatically stem to the same root word as "learn" (linguistic stemming)
  const query = "learning";
  const resultsStemming = await db.search(query);
  log(
    "3. Full-Text Stemming Search",
    `Searched for the word "${query}". Notice how the FTS5 Porter Stemmer successfully matched both "learn-rust" and "learn-typescript":`,
    resultsStemming.map((r) => ({
      id: r.id,
      score: r.score,
      text: (r.value as Record<string, unknown>).text,
    })),
  );

  // 4. Perform Search with custom Metadata Filters
  // Search for "learning" but filter only where status is "active"
  const resultsFiltered = await db.search(query, {
    filter: {
      status: "active",
    },
  });
  log(
    "4. Search with Metadata Filters",
    `Searched for "${query}" with metadata filter { status: "active" }. Notice only "learn-typescript" is returned:`,
    resultsFiltered.map((r) => ({
      id: r.id,
      score: r.score,
      status: (r.value as Record<string, unknown>).status,
      text: (r.value as Record<string, unknown>).text,
    })),
  );

  // 5. Close database safely
  await db.close();
  log("5. Database Closed", "Closed the persistent database safely.");

  console.log("\n🎉 Agent long-term memory Quickstart completed successfully!\n");
}

main().catch((error) => {
  console.error("❌ Quickstart failed with error:", error);
  process.exit(1);
});
