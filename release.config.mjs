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
        prepareCmd: [
          "npm --no-git-tag-version --prefix packages/thingd version ${nextRelease.version}",
          "npm --no-git-tag-version --prefix packages/thingd-cli version ${nextRelease.version}",
          "npm --no-git-tag-version --prefix packages/thingd-native version ${nextRelease.version}",
          "sed -i.bak 's/^version = \".*\"/version = \"${nextRelease.version}\"/' Cargo.toml && rm -f Cargo.toml.bak",
          "sed -i.bak 's/export const SDK_VERSION = \".*\"/export const SDK_VERSION = \"${nextRelease.version}\"/' packages/thingd/src/version.ts && rm -f packages/thingd/src/version.ts.bak",
          "VERSION_MM=$(echo ${nextRelease.version} | sed 's/\\([0-9]*\\.[0-9]*\\)\\..*/\\1/')",
          "sed -i.bak 's/version = \"[0-9]*\\.[0-9]*\"/version = \"'\"$VERSION_MM\"'\"/g' README.md crates/thingd-core/README.md && rm -f README.md.bak crates/thingd-core/README.md.bak",
        ].join(" && "),
        publishCmd: "pnpm --filter thingd publish --access public --no-git-checks && pnpm --filter thingd-cli publish --access public --no-git-checks && pnpm --filter thingd-native publish --access public --no-git-checks"
      }
    ],
    [
      "@semantic-release/git",
      {
        assets: [
          "CHANGELOG.md",
          "README.md",
          "Cargo.toml",
          "packages/thingd/package.json",
          "packages/thingd/src/version.ts",
          "packages/thingd-cli/package.json",
          "packages/thingd-native/package.json",
          "crates/thingd-core/README.md",
        ],
      },
    ],
    "@semantic-release/github",
  ],
};

export default config;
