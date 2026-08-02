# Release Process

`thingd` uses semantic-release to calculate, version, tag, and publish four npm packages (`@thingd/sdk`, `@thingd/cli`, `@thingd/native`, and `@thingd/client`) plus the Rust crate.

The hosted app-backend client is released as part of the public client package.
Deploy its compatible Cloud API only after the public contract release and the
Cloud compatibility matrix has been updated.

## Versioning

Versions follow standard SemVer through conventional commits:

- `fix:` creates a patch release, such as `1.0.1`
- `feat:` creates a minor release, such as `1.1.0`
- `BREAKING CHANGE:` or `!` creates a major release, such as `2.0.0`

Examples:

```txt
fix(queue): preserve idempotency keys
feat(search): add metadata filters
feat(storage)!: replace the storage adapter interface
```

## Branch and release workflow

Use the following branch flow for open-source development:

```txt
feature/* → squash merge → main → semantic-release → publish
```

Feature branches should be squash-merged into `main` using a conventional
commit title such as `feat:` or `fix:`. After a releasable change lands on
`main`, semantic-release calculates the next SemVer, updates the synchronized
npm/Cargo/version files, commits the release with `[skip ci]`, creates the
`vX.Y.Z` tag, and publishes the release. There is no intermediate release PR.

## GitHub Actions

CI runs on pull requests targeting `main` and pushes to `main`.

The release workflow runs on pushes to `main`. A semantic-release dry run first
determines whether the commit range requires a release. Native artifacts and
publishing are skipped when no release is needed. A manual `workflow_dispatch`
with `publish_version` retries publication for an existing tagged version without
calculating a new version.

It validates the same checks, then publishes to npm when the `NPM_TOKEN` repository secret exists.

## Branch and pull request workflow

Create feature branches from `main` and target all pull requests at `main`.
Squash-merge completed feature branches into `main`; production deployment and
publishing remain restricted to `main`.

Before configuring `NPM_TOKEN`, use the local package smoke test:

```bash
pnpm test:package
```

This command builds `@thingd/sdk`, packs it into a package tarball, installs that tarball into a temporary app, and imports the installed package.

You can also verify the release plan without publishing:

```bash
pnpm release:dry-run
```

## Required Secrets

Add these repository secrets before enabling publishing:

```txt
NPM_TOKEN               # npm publish (required)
CARGO_REGISTRY_TOKEN     # crates.io publish (required)
DOCKER_USERNAME          # Docker Hub username (required for Docker image)
DOCKER_PASSWORD          # Docker Hub password or access token (required for Docker image)
```

The npm package is configured with npm provenance enabled through `publishConfig.provenance`.

## crates.io Publishing

On every release, the workflow publishes `thingd` to [crates.io](https://crates.io/crates/thingd). The Rust crate version is kept in sync with the npm packages via `semantic-release`.

```toml
[dependencies]
thingd = { version = "0.71", features = ["persistent", "search"] }
```

The publish runs in parallel with npm and Docker publishing.

## Docker Image Publishing

On every release, the workflow builds and pushes a Docker image to [Docker Hub](https://hub.docker.com/r/sayanmohsin/thingd):

- `sayanmohsin/thingd:<version>` — tagged with the exact SemVer (e.g., `v0.71.0`)
- `sayanmohsin/thingd:latest` — always points to the latest release

Pull the image:

```bash
docker pull sayanmohsin/thingd
```

The Docker image includes the native persistent driver pre-built for supported Linux targets.
See [docker-context/Dockerfile](../docker-context/Dockerfile) and [deploy/docker-compose.yml](../deploy/docker-compose.yml) for the runtime shape.

The workspace uses `workspace:^` dependency specs during development so pnpm links the local SDK and native packages. The semantic-release prepare hook converts those internal ranges to `^${nextRelease.version}` before npm publishes and records the publishable ranges in the release commit.

The release workflow pins Node.js 24. Each release automatically publishes all
four npm packages, updates `CHANGELOG.md` from conventional commits, creates a
GitHub Release with release notes, and commits the synchronized version bump to
`main`.

## First npm Publish From CI

Use this path when the package does not exist on npm yet.

1. Add `NPM_TOKEN` in GitHub: repository Settings -> Secrets and variables -> Actions -> Repository secrets.
2. Push the repo to GitHub with a conventional commit that creates a release. For the first release, use something like:

```txt
feat: initial thingd release
```

3. Open GitHub -> Actions -> Release -> Run workflow -> branch `main`.
4. The workflow runs the semantic-release dry run, builds native artifacts, validates package tarballs, publishes all packages, creates the Git tag, and creates the GitHub release.

If semantic-release says there is no release, the commits on `main` did not include a releasable conventional commit. Add a `feat:`, `fix:`, `perf:`, or breaking-change commit and run the workflow again.

After the first publish, configure npm Trusted Publishing for tokenless releases:

```txt
Organization or user: sayanmohsin
Repository: thingd
Workflow filename: release.yml
Environment name: leave blank unless GitHub environments are enabled
```

Once Trusted Publishing is verified, remove the `NPM_TOKEN` secret.

## Branch Protection

Protect `main` in GitHub:

- require pull requests before merging
- require the CI workflow to pass
- block force pushes
- require linear history if desired
- allow the release workflow to create tags and GitHub releases
- allow repository administrators to bypass protection while the project is small
- require outside contributors to use forks and pull requests

The release workflow commits the semantic-release version commit directly to
`main`. Configure branch protection to allow only the release workflow to bypass
the protected-branch requirement for that commit and tag.

---

## Native Prebuilds Workflow

Prebuild binaries (`.node` files) are compiled for common architectures and platforms, then bundled inside the `@thingd/native` package.

### Target Matrix
- `darwin-arm64` (macOS Apple Silicon)
- `darwin-x64` (macOS Intel)
- `linux-arm64` (Linux ARM)
- `linux-x64` (Linux Intel)

### Local Prebuild Compilation
To compile and stage a prebuild for your current platform and architecture:
1. Build the Rust target in release mode:
   ```bash
   cargo build -p thingd-native --release
   ```
2. Run the staging script to copy the binary to the `prebuilds/` distribution folder:
   ```bash
   pnpm --filter thingd-native build:prebuild
   ```
   *(Or run `node packages/thingd-native/scripts/copy-prebuild.mjs` directly.)*

### Staging All Targets for Release
During the release workflow, the runner cross-compiles the Rust addon for all supported target platforms and places them in `packages/thingd-native/prebuilds/<platform>-<arch>/thingd_native.node` before building and publishing the NPM package.

Because `prebuilds` is listed in the `files` array inside `packages/thingd-native/package.json`, they are automatically bundled in the published package, allowing consumer environments to resolve and dynamically load them.
