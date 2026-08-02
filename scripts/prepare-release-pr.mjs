import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { analyzeCommits } from "@semantic-release/commit-analyzer";
import { releaseRules } from "../release.config.mjs";

const root = process.cwd();
const args = new Set(process.argv.slice(2));
function argumentValue(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
}

const planFile = argumentValue("--plan-file");
const outputFile = argumentValue("--json");

function git(...gitArgs) {
  return execFileSync("git", gitArgs, { cwd: root, encoding: "utf8" }).trim();
}

function readVersion() {
  const cargo = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8");
  return cargo.match(/^version = "([0-9]+\.[0-9]+\.[0-9]+)"/m)?.[1];
}

function parseVersion(version) {
  const match = version.match(/^(\d+)\.(\d+)\.(\d+)$/);
  if (!match) {
    throw new Error(`Invalid version: ${version}`);
  }
  return match.slice(1).map(Number);
}

function bumpVersion(version, releaseType) {
  const [major, minor, patch] = parseVersion(version);
  if (releaseType === "major") {
    return `${major + 1}.0.0`;
  }
  if (releaseType === "minor") {
    return `${major}.${minor + 1}.0`;
  }
  return `${major}.${minor}.${patch + 1}`;
}

function generateReleaseNotes(commits, lastTag, version) {
  const groups = new Map([
    ["breaking", []],
    ["feat", []],
    ["fix", []],
  ]);
  for (const commit of commits) {
    const match = commit.message.match(/^(\w+)(?:\(([^)]+)\))?(!)?:\s+(.+)/);
    if (!match || !groups.has(match[1])) {
      continue;
    }
    const [, type, scope, bang, subject] = match;
    const breaking = bang || /BREAKING CHANGE:/m.test(commit.message);
    const label = breaking ? "breaking" : type;
    groups.get(label).push(`${scope ? `${scope}: ` : ""}${subject} (${commit.hash.slice(0, 7)})`);
  }
  const sections = [];
  if (groups.get("breaking").length) {
    sections.push(`### BREAKING CHANGES\n\n${groups.get("breaking").map((item) => `- ${item}`).join("\n")}`);
  }
  if (groups.get("feat").length) {
    sections.push(`### Features\n\n${groups.get("feat").map((item) => `- ${item}`).join("\n")}`);
  }
  if (groups.get("fix").length) {
    sections.push(`### Bug Fixes\n\n${groups.get("fix").map((item) => `- ${item}`).join("\n")}`);
  }
  const compareUrl = `https://github.com/${process.env.GITHUB_REPOSITORY || "sayanmohsin/thingd"}/compare/${lastTag}...v${version}`;
  return `## [${version}](${compareUrl}) (${new Date().toISOString().slice(0, 10)})\n\n${sections.join("\n\n")}`;
}

function commitsSince(tag) {
  const range = tag ? `${tag}..HEAD` : "HEAD";
  const raw = git("log", range, "--format=%H%x1f%s%x1f%b%x1e");
  return raw
    .split("\x1e")
    .filter(Boolean)
    .map((entry) => {
      const [rawHash, rawSubject, rawBody = ""] = entry.split("\x1f");
      const hash = rawHash.trim();
      const subject = rawSubject.trim();
      const body = rawBody.trim();
      return { hash, message: body ? `${subject}\n\n${body}` : subject };
    });
}

async function createPlan() {
  const currentVersion = readVersion();
  if (!currentVersion) {
    throw new Error("Could not read the workspace version from Cargo.toml");
  }
  let lastTag = `v${currentVersion}`;
  try {
    lastTag = git("describe", "--tags", "--abbrev=0");
  } catch {
    // The first release has no prior tag.
  }
  const commits = commitsSince(lastTag);

  if (commits.some(({ message }) => /^chore\(release\): v\d+\.\d+\.\d+/m.test(message))) {
    return { needed: false, reason: "release PR already merged and awaiting publication" };
  }

  const context = {
    cwd: root,
    commits,
    lastRelease: {
      version: currentVersion,
      gitTag: lastTag,
    },
    options: {
      repositoryUrl: process.env.GITHUB_REPOSITORY
        ? `https://github.com/${process.env.GITHUB_REPOSITORY}`
        : "https://github.com/sayanmohsin/thingd",
    },
    logger: { log() {}, error() {}, warn() {}, success() {} },
  };
  const releaseType = await analyzeCommits({ preset: "conventionalcommits", releaseRules }, context);
  if (!releaseType) {
    return { needed: false, reason: "no releasable conventional commits" };
  }

  const version = bumpVersion(currentVersion, releaseType);
  const notes = generateReleaseNotes(commits, lastTag, version);
  return { needed: true, version, releaseType, lastTag, notes };
}

function replaceFirst(file, pattern, replacement) {
  const absolute = path.join(root, file);
  const source = fs.readFileSync(absolute, "utf8");
  const updated = source.replace(pattern, replacement);
  if (source === updated) {
    throw new Error(`Expected version pattern was not found in ${file}`);
  }
  fs.writeFileSync(absolute, updated);
}

function replaceIfPresent(file, pattern, replacement) {
  const absolute = path.join(root, file);
  const source = fs.readFileSync(absolute, "utf8");
  const updated = source.replace(pattern, replacement);
  if (source !== updated) {
    fs.writeFileSync(absolute, updated);
  }
}

function updateThingdDependencyVersion(file, majorMinor) {
  const absolute = path.join(root, file);
  const source = fs.readFileSync(absolute, "utf8");
  const lines = source.split("\n");
  const start = lines.findIndex((line) => /^\s*thingd\s*=/.test(line));
  if (start === -1) {
    return;
  }

  let end = start;
  while (end < lines.length && !lines[end].includes("}")) {
    end += 1;
  }

  const block = lines.slice(start, end + 1).join("\n");
  const updatedBlock = block.replace(/(\bversion\s*=\s*)"[^"]+"/, `$1"${majorMinor}"`);
  if (block !== updatedBlock) {
    lines.splice(start, end - start + 1, ...updatedBlock.split("\n"));
    fs.writeFileSync(absolute, lines.join("\n"));
    return;
  }
}

function applyPlan(plan) {
  const { version } = plan;
  const majorMinor = version.split(".").slice(0, 2).join(".");
  for (const file of [
    "packages/thingd/package.json",
    "packages/thingd-cli/package.json",
    "packages/thingd-native/package.json",
    "packages/thingd-client/package.json",
  ]) {
    const absolute = path.join(root, file);
    const packageJson = JSON.parse(fs.readFileSync(absolute, "utf8"));
    packageJson.version = version;
    fs.writeFileSync(absolute, `${JSON.stringify(packageJson, null, 2)}\n`);
  }

  replaceFirst("Cargo.toml", /^version = ".*"$/m, `version = "${version}"`);
  replaceFirst("packages/thingd/src/version.ts", /SDK_VERSION = ".*"/, `SDK_VERSION = "${version}"`);
  for (const file of ["README.md", "crates/thingd/README.md"]) {
    replaceIfPresent(file, /version = "\d+\.\d+"/g, `version = "${majorMinor}"`);
  }
  for (const file of ["crates/thingd-server/Cargo.toml", "packages/thingd-native/Cargo.toml"]) {
    updateThingdDependencyVersion(file, majorMinor);
  }

  const changelog = path.join(root, "CHANGELOG.md");
  const existing = fs.readFileSync(changelog, "utf8").trimStart();
  fs.writeFileSync(changelog, `${plan.notes.trim()}\n\n${existing}`);
}

const plan = planFile ? JSON.parse(fs.readFileSync(path.resolve(planFile), "utf8")) : await createPlan();
if (args.has("--apply") && plan.needed) {
  applyPlan(plan);
}
const serialized = `${JSON.stringify(plan, null, 2)}\n`;
if (outputFile) {
  fs.writeFileSync(path.resolve(outputFile), serialized);
}
process.stdout.write(serialized);
