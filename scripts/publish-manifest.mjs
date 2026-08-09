import fs from "node:fs";
import path from "node:path";

export const publishPackages = [
  {
    name: "@thingd/sdk",
    path: "packages/thingd",
    dependencies: { "@thingd/native": "optionalDependencies" },
  },
  {
    name: "@thingd/cli",
    path: "packages/thingd-cli",
    dependencies: { "@thingd/sdk": "dependencies" },
  },
  { name: "@thingd/native", path: "packages/thingd-native", dependencies: {} },
  { name: "@thingd/client", path: "packages/thingd-client", dependencies: {} },
];

function dependencySection(manifest, section, dependency) {
  const dependencies = manifest[section];
  return dependencies && typeof dependencies === "object" ? dependencies[dependency] : undefined;
}

export function publishRange(version) {
  return `^${version}`;
}

export function normalizeManifest(manifest, version) {
  const normalized = structuredClone(manifest);
  for (const pkg of publishPackages) {
    const packageManifest = pkg.name === manifest.name ? normalized : null;
    if (!packageManifest) {
      continue;
    }
    for (const [dependency, section] of Object.entries(pkg.dependencies)) {
      if (dependencySection(packageManifest, section, dependency) !== undefined) {
        packageManifest[section][dependency] = publishRange(version);
      }
    }
  }
  return normalized;
}

export function findWorkspaceProtocols(value, location = "manifest") {
  const matches = [];
  if (typeof value === "string") {
    if (value.startsWith("workspace:")) {
      matches.push(location);
    }
    return matches;
  }
  if (Array.isArray(value)) {
    value.forEach((item, index) => matches.push(...findWorkspaceProtocols(item, `${location}[${index}]`)));
    return matches;
  }
  if (value && typeof value === "object") {
    for (const [key, item] of Object.entries(value)) {
      matches.push(...findWorkspaceProtocols(item, `${location}.${key}`));
    }
  }
  return matches;
}

export function validatePublishManifest(manifest, version) {
  const errors = [];
  if (manifest.version !== version) {
    errors.push(`expected version ${version}, received ${manifest.version}`);
  }
  const workspaceProtocols = findWorkspaceProtocols(manifest);
  if (workspaceProtocols.length > 0) {
    errors.push(`workspace protocols remain at ${workspaceProtocols.join(", ")}`);
  }
  const pkg = publishPackages.find(({ name }) => name === manifest.name);
  if (pkg) {
    for (const [dependency, section] of Object.entries(pkg.dependencies)) {
      const actual = dependencySection(manifest, section, dependency);
      if (actual !== undefined && actual !== publishRange(version)) {
        errors.push(`${section}.${dependency} must be ${publishRange(version)}, received ${actual}`);
      }
    }
  }
  return errors;
}

export function readManifest(root, packagePath) {
  return JSON.parse(fs.readFileSync(path.join(root, packagePath, "package.json"), "utf8"));
}
