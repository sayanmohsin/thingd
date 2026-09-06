import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const publicRoot = resolve(import.meta.dirname, "../../..");
const cloudRoot = resolve(publicRoot, "../thingd-cloud");
const registry = resolve(cloudRoot, "docs/agent-work/phase-tracker.md");
const allowed = ["planned", "active", "in progress", "implemented", "verified", "released", "blocked", "ready"];

if (!existsSync(registry)) {
  console.error(`Missing Cloud phase tracker: ${registry}`);
  process.exit(1);
}

const text = readFileSync(registry, "utf8");
const rows = text.split("\n").filter((line) => line.startsWith("| ") && !line.startsWith("| ---"));
const errors = [];
for (const row of rows) {
  const cells = row.split("|").slice(1, -1).map((cell) => cell.trim());
  if (cells[0] === "Area") continue;
  if (cells.length < 4) errors.push(`Malformed registry row: ${row}`);
  if (cells[1] && !allowed.some((status) => cells[1].toLowerCase().startsWith(status))) {
    errors.push(`Unknown status in row: ${row}`);
  }
}

if (errors.length) {
  console.error(errors.join("\n"));
  process.exit(1);
}
console.log(`Phase tracker valid: ${rows.length - 1} tracked areas`);
