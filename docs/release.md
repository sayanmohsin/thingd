# Release Process

`thingd` uses Release Please to calculate versions and create release PRs. Merging
the release PR tags and publishes four npm packages (`@thingd/sdk`, `@thingd/cli`,
`@thingd/native`, and `@thingd/client`) plus the Rust crate.

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
feature/* → squash merge → main → Release Please PR → merge → publish
```

Feature branches should be squash-merged into `main` using a conventional
commit title such as `feat:` or `fix:`. Release Please groups releasable changes
and opens a release PR with synchronized npm, Cargo, and changelog versions.
After that PR merges, the workflow creates the `thingd-vX.Y.Z` tag and publishes
the release.

## GitHub Actions

CI runs on pull requests targeting `main` and pushes to `main`.

Build-time troubleshooting and scoped local commands are documented in
[Build performance](./build-performance.md).

The release workflow runs on pushes to `main`. Release Please creates or updates
the release PR; native artifacts and publishing run only after a release is
created or a manual retry is requested with `publish_version`.

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

Run the publish metadata regression tests without publishing:

```bash
pnpm test:publish-manifests
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

On every release, the workflow publishes `thingd` to [crates.io](https://crates.io/crates/thingd). The Rust crate version is kept in sync with the npm packages by Release Please.

```toml
[dependencies]
thingd = { version = "0.71", features = ["persistent", "search"] }
```

The publish runs in parallel with npm and Docker publishing.

## Docker Image Publishing

On every release, the workflow builds and pushes a Docker image to [Docker Hub](https://hub.docker.com/r/sayanmohsin/thingd). The `amd64` and `arm64` static server binaries build in parallel after the release tag is created; the image assembly runs after both binaries are uploaded and does not wait for npm publication.

- `sayanmohsin/thingd:<version>` — tagged with the exact SemVer (e.g., `v0.71.0`)
- `sayanmohsin/thingd:latest` — always points to the latest release

Pull the image:

```bash
docker pull sayanmohsin/thingd
```

The Docker image includes the native persistent driver pre-built for supported Linux targets.
See [docker-context/Dockerfile](../docker-context/Dockerfile) and [deploy/docker-compose.yml](../deploy/docker-compose.yml) for the runtime shape.

The workspace uses `workspace:^` dependency specs during development so pnpm links
the local SDK and native packages. These specs must never reach npm. The release
workflow runs `scripts/prepare-publish-manifests.mjs` on its ephemeral checkout,
converts internal ranges to `^${VERSION}`, validates all four packed manifests,
and installs the packed CLI in a clean temporary application before publishing.

After publishing, the workflow installs `@thingd/cli@VERSION` from the public npm
registry as a second smoke test. This catches registry metadata problems that a
local tarball test cannot detect.

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
4. The workflow builds native artifacts, prepares publish manifests, validates package tarballs, installs the packed CLI, publishes packages in dependency order, and verifies the CLI from npm.

If Release Please does not create a release PR, the commits on `main` did not include a releasable conventional commit. Add a `feat:`, `fix:`, `perf:`, or breaking-change commit and push again.

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

The release workflow merges through the Release Please PR and creates the tag and
GitHub release. Configure branch protection to require the release PR checks and
allow the workflow to publish tags and releases.

## Recovering from a bad npm publication

npm versions are immutable. If a package is published with invalid metadata, do
not try to overwrite that version. Create a patch release, validate it through the
full workflow, and deprecate the broken version:

```bash
npm deprecate @thingd/cli@0.77.0 "Install @thingd/cli@0.77.1; this version contains invalid workspace dependency metadata."
```

Then verify a clean application can run `npm install @thingd/cli@0.77.1` and
import the package successfully.

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
