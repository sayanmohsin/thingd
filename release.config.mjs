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
      "@semantic-release/npm",
      {
        npmPublish: false,
        pkgRoot: "packages/thingd",
      },
    ],
    [
      "@semantic-release/exec",
      {
        prepareCmd: "npm --no-git-tag-version --prefix packages/thingd-cli version ${nextRelease.version}",
        publishCmd: "pnpm --filter thingd publish --access public --no-git-checks && pnpm --filter thingd-cli publish --access public --no-git-checks"
      }
    ],
    "@semantic-release/github",
  ],
};

export default config;
