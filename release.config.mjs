const config = {
  branches: ["main"],
  tagFormat: "v${version}",
  plugins: [
    [
      "@semantic-release/commit-analyzer",
      {
        preset: "conventionalcommits",
        releaseRules: [
          { breaking: true, release: "major" },
          { type: "feat", release: "minor" },
          { type: "fix", release: "patch" },
          { type: "perf", release: "patch" },
        ],
      },
    ],
    [
      "@semantic-release/release-notes-generator",
      {
        preset: "conventionalcommits",
      },
    ],
    [
      "@semantic-release/changelog",
      {
        changelogFile: "CHANGELOG.md",
      },
    ],
    [
      "@semantic-release/exec",
      {
        prepareCmd: "npm --no-git-tag-version --prefix packages/thingd version ${nextRelease.version} && npm --no-git-tag-version --prefix packages/thingd-cli version ${nextRelease.version} && npm --no-git-tag-version --prefix packages/thingd-native version ${nextRelease.version} && sed -i.bak 's/^version = \".*\"/version = \"${nextRelease.version}\"/' crates/thingd-core/Cargo.toml && rm -f crates/thingd-core/Cargo.toml.bak",
        publishCmd: "pnpm --filter thingd publish --access public --no-git-checks && pnpm --filter thingd-cli publish --access public --no-git-checks && pnpm --filter thingd-native publish --access public --no-git-checks"
      }
    ],
    [
      "@semantic-release/git",
      {
        assets: [
          "CHANGELOG.md",
          "packages/thingd/package.json",
          "packages/thingd-cli/package.json",
          "packages/thingd-native/package.json",
          "crates/thingd-core/Cargo.toml",
        ],
      },
    ],
    "@semantic-release/github",
  ],
};

export default config;
