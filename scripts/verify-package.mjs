import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chdir, cwd, env as processEnv } from "node:process";
import { parseArgs } from "node:util";
import { execFileSync } from "node:child_process";

const rootDir = resolve(new URL("..", import.meta.url).pathname);
const packageDir = join(rootDir, "packages", "thingd");

const { values } = parseArgs({
  options: {
    keep: {
      type: "boolean",
      default: false,
    },
  },
});

const run = (command, args, options = {}) => {
  execFileSync(command, args, {
    encoding: "utf8",
    stdio: "inherit",
    ...options,
  });
};

const runJson = (command, args, options = {}) => {
  const output = execFileSync(command, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
    ...options,
  });

  return JSON.parse(output);
};

const tempDir = await mkdtemp(join(tmpdir(), "thingd-package-smoke-"));
const originalCwd = cwd();
const packageManagerEnv = {
  ...processEnv,
  npm_config_cache: join(tempDir, ".npm-cache"),
  pnpm_config_store_dir: join(tempDir, ".pnpm-store"),
};

try {
  const packOutput = runJson("pnpm", ["pack", "--json", "--pack-destination", tempDir], {
    cwd: packageDir,
    env: packageManagerEnv,
  });
  const packedPackage = Array.isArray(packOutput) ? packOutput[0] : packOutput;
  const tarball = packedPackage.filename.startsWith("/")
    ? packedPackage.filename
    : join(tempDir, packedPackage.filename);

  await writeFile(
    join(tempDir, "package.json"),
    JSON.stringify(
      {
        private: true,
        type: "module",
      },
      null,
      2,
    ),
  );

  run("pnpm", ["add", tarball, "--ignore-scripts"], {
    cwd: tempDir,
    env: packageManagerEnv,
  });

  await writeFile(
    join(tempDir, "smoke.mjs"),
    `import assert from "node:assert/strict";
import { ThingD } from "thingd";

const db = await ThingD.open(":memory:");
await db.put("decisions", {
  id: "package-smoke",
  text: "The packed package can be installed and imported locally.",
});

await db.events.append("project:thingd", {
  type: "package.verified",
  text: "Local package smoke test passed.",
});

await db.queue("verify").push({
  object: "decisions/package-smoke",
});

const object = await db.get("decisions", "package-smoke");
const eventHits = await db.search("package smoke");
const job = await db.queue("verify").claim();
const acked = job ? await db.queue("verify").ack(job.id) : null;

assert.equal(object?.id, "package-smoke");
assert.equal(eventHits[0]?.kind, "event");
assert.equal(job?.payload.object, "decisions/package-smoke");
assert.equal(acked?.ok, true);
`,
  );

  chdir(tempDir);
  run("node", ["smoke.mjs"]);
  chdir(originalCwd);

  console.log(`Verified package tarball: ${tarball}`);
} finally {
  chdir(originalCwd);

  if (!values.keep) {
    await rm(tempDir, {
      recursive: true,
      force: true,
    });
  } else {
    console.log(`Kept smoke-test directory: ${tempDir}`);
  }
}
