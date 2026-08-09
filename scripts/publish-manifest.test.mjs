import assert from "node:assert/strict";
import test from "node:test";
import {
  normalizeManifest,
  publishPackages,
  publishRange,
  validatePublishManifest,
} from "./publish-manifest.mjs";

test("normalizes workspace dependencies for the SDK", () => {
  const manifest = {
    name: "@thingd/sdk",
    version: "0.77.1",
    optionalDependencies: { "@thingd/native": "workspace:^" },
  };
  const normalized = normalizeManifest(manifest, "0.77.1");
  assert.equal(normalized.optionalDependencies["@thingd/native"], "^0.77.1");
  assert.equal(manifest.optionalDependencies["@thingd/native"], "workspace:^");
});

test("normalizes an already-versioned CLI dependency", () => {
  const manifest = {
    name: "@thingd/cli",
    version: "0.77.1",
    dependencies: { "@thingd/sdk": "^0.77.0" },
  };
  const normalized = normalizeManifest(manifest, "0.77.1");
  assert.equal(normalized.dependencies["@thingd/sdk"], publishRange("0.77.1"));
});

test("rejects workspace protocols and mismatched internal versions", () => {
  const errors = validatePublishManifest(
    {
      name: "@thingd/cli",
      version: "0.77.0",
      dependencies: { "@thingd/sdk": "workspace:^" },
    },
    "0.77.1",
  );
  assert.match(errors.join("\n"), /expected version 0\.77\.1/);
  assert.match(errors.join("\n"), /workspace protocols remain/);
  assert.match(errors.join("\n"), /must be \^0\.77\.1/);
});

for (const pkg of publishPackages) {
  test(`validates the ${pkg.name} publish manifest`, () => {
    const manifest = { name: pkg.name, version: "0.77.1" };
    for (const section of Object.values(pkg.dependencies)) {
      manifest[section] = { ...(manifest[section] ?? {}) };
    }
    const normalized = normalizeManifest(manifest, "0.77.1");
    assert.deepEqual(validatePublishManifest(normalized, "0.77.1"), []);
  });
}
