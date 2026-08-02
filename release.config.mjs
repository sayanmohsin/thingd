export const releaseRules = [
  { breaking: true, release: "major" },
  { type: "feat", release: "minor" },
  { type: "fix", release: "patch" },
  { type: "perf", release: "patch" },
];

export const releaseNotesConfig = {
  preset: "conventionalcommits",
};

export default {
  branches: ["main"],
  tagFormat: "v${version}",
  plugins: [
    ["@semantic-release/commit-analyzer", { preset: "conventionalcommits", releaseRules }],
    ["@semantic-release/release-notes-generator", releaseNotesConfig],
    ["@semantic-release/changelog", { changelogFile: "CHANGELOG.md" }],
    ["@semantic-release/exec", { prepareCmd: "node scripts/sync-release-files.mjs ${nextRelease.version}" }],
    ["@semantic-release/npm", { pkgRoot: "packages/thingd" }],
    ["@semantic-release/npm", { pkgRoot: "packages/thingd-cli" }],
    ["@semantic-release/npm", { pkgRoot: "packages/thingd-native" }],
    ["@semantic-release/npm", { pkgRoot: "packages/thingd-client" }],
    [
      "@semantic-release/git",
      {
        assets: [
          "CHANGELOG.md",
          "Cargo.toml",
          "README.md",
          "crates/thingd/README.md",
          "crates/thingd-server/Cargo.toml",
          "packages/thingd/package.json",
          "packages/thingd-cli/package.json",
          "packages/thingd-native/package.json",
          "packages/thingd-native/Cargo.toml",
          "packages/thingd-client/package.json",
          "packages/thingd/src/version.ts",
        ],
        message: "chore(release): v${nextRelease.version} [skip ci]\n\n${nextRelease.notes}",
      },
    ],
    "@semantic-release/github",
  ],
};
