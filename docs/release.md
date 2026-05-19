# Release Process

`memoryd` uses semantic-release to publish the npm package from `packages/memoryd`.

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

## GitHub Actions

CI runs on:

- pull requests targeting `main`
- pushes to `main`

The release workflow runs on:

- pushes to `main`
- manual runs from GitHub Actions through `workflow_dispatch`

It validates the same checks, then publishes to npm when the `NPM_TOKEN` repository secret exists.

Before configuring `NPM_TOKEN`, use the local package smoke test:

```bash
pnpm test:package
```

This command builds `@sayanmohsin/memoryd`, packs it into a package tarball, installs that tarball into a temporary app, and imports the installed package.

You can also verify the release plan without publishing:

```bash
pnpm release:dry-run
```

## Required Secrets

Add this repository secret before enabling npm publishing:

```txt
NPM_TOKEN
```

The package is configured with npm provenance enabled through `publishConfig.provenance`.

Release packaging intentionally avoids `workspace:*` dependency specs in `package.json` files. The repo uses pnpm for development, but `@semantic-release/npm` calls the npm CLI internally during publish, and npm rejects pnpm-only workspace protocol dependencies.

## First npm Publish From CI

Use this path when the package does not exist on npm yet.

1. Add `NPM_TOKEN` in GitHub: repository Settings -> Secrets and variables -> Actions -> Repository secrets.
2. Push the repo to GitHub with a conventional commit that creates a release. For the first release, use something like:

```txt
feat: initial memoryd release
```

3. Open GitHub -> Actions -> Release -> Run workflow -> branch `main`.
4. The workflow runs checks, builds the package from `packages/memoryd`, publishes `@sayanmohsin/memoryd`, creates the Git tag, and creates the GitHub release.

If semantic-release says there is no release, the commits on `main` did not include a releasable conventional commit. Add a `feat:`, `fix:`, `perf:`, or breaking-change commit and run the workflow again.

After the first publish, configure npm Trusted Publishing for tokenless releases:

```txt
Organization or user: sayanmohsin
Repository: memoryd
Workflow filename: release.yml
Environment name: leave blank unless GitHub environments are enabled
```

Once Trusted Publishing is verified, remove the `NPM_TOKEN` secret.

## Branch Protection

Before opening the repository, protect `main` in GitHub:

- require pull requests before merging
- require the CI workflow to pass
- block force pushes
- require linear history if desired
- allow the release workflow to create tags and GitHub releases

The release workflow does not push version commits back to `main`; semantic-release computes the next version, creates a Git tag/GitHub release, and publishes the npm package.
