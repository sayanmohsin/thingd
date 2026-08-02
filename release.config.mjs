export const releaseRules = [
  { breaking: true, release: "major" },
  { type: "feat", release: "minor" },
  { type: "fix", release: "patch" },
  { type: "perf", release: "patch" },
];

export const releaseNotesConfig = {
  preset: "conventionalcommits",
};

// Release publication is orchestrated by .github/workflows/release.yml.
// Keeping this config free of @semantic-release/git prevents direct pushes to
// protected main when semantic-release is run locally or by another workflow.
export default {
  branches: ["main"],
  tagFormat: "v${version}",
  plugins: [
    ["@semantic-release/commit-analyzer", { preset: "conventionalcommits", releaseRules }],
    ["@semantic-release/release-notes-generator", releaseNotesConfig],
  ],
};
