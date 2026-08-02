# Release Process

`thingd` uses semantic-release to publish three npm packages (`@thingd/sdk`, `@thingd/cli`, `@thingd/native`).

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
feature/* → squash merge → development → regular merge PR → main → release
```

Feature branches should start from `development`. Squash-merge completed
features into `development` using a conventional commit title such as `feat:` or
`fix:`. CI runs on `development` and `main`, but release automation runs only
after `main` changes.

When `development` is ready, open a pull request into `main` and use a regular
merge commit. Do not squash this release merge: semantic-release scans all
conventional commits since the previous tag and produces one version containing
the complete batch. After the release, sync `main` back into `development`.

## GitHub Actions

CI runs on:

- pull requests targeting `development` or `main`
- pushes to `development` or `main`

The release workflow runs on:

- pushes to `main`
- manual runs from GitHub Actions through `workflow_dispatch`

It validates the same checks, then publishes to npm when the `NPM_TOKEN` repository secret exists.

## Branch and pull request workflow

Create feature branches from `development` and target all pull requests at
`development`. Do not open pull requests directly against `main`. Once the
integrated changes are ready for release, manually merge `development` into
`main`; production deployment and publishing remain restricted to `main`.

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
thingd = { version = "0.71", features = ["fjall", "search"] }
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

The Docker image includes the native Fjall driver pre-built for supported Linux targets.
See [docker-context/Dockerfile](../docker-context/Dockerfile) and [deploy/docker-compose.yml](../deploy/docker-compose.yml) for the runtime shape.

Release packaging intentionally avoids `workspace:*` dependency specs in `package.json` files. The repo uses pnpm for development, but `@semantic-release/exec` calls the npm CLI internally during version bumps, and npm rejects pnpm-only workspace protocol dependencies.

The release workflow pins Node.js 24 so semantic-release runs with a modern npm. Each release automatically publishes all three npm packages, updates `CHANGELOG.md` in the repo from conventional commits, creates a GitHub Release with release notes, and pushes version bump commits back to `main`.

## First npm Publish From CI

Use this path when the package does not exist on npm yet.

1. Add `NPM_TOKEN` in GitHub: repository Settings -> Secrets and variables -> Actions -> Repository secrets.
2. Push the repo to GitHub with a conventional commit that creates a release. For the first release, use something like:

```txt
feat: initial thingd release
```

3. Open GitHub -> Actions -> Release -> Run workflow -> branch `main`.
4. The workflow runs checks, builds the package from `packages/thingd`, publishes `@thingd/sdk`, creates the Git tag, and creates the GitHub release.

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

Protect both `main` and `development` in GitHub:

- require pull requests before merging
- require the CI workflow to pass
- block force pushes
- require linear history if desired
- allow the release workflow to create tags and GitHub releases
- allow repository administrators to bypass protection while the project is small
- require outside contributors to use forks and pull requests

The release workflow pushes version bump commits (including `CHANGELOG.md` and updated `package.json` files) back to `main` via `@semantic-release/git`. It also creates a Git tag and GitHub Release.

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
