# Nice Code review

Thingd uses [Nice Code](https://github.com/sayanmohsin/nice-code) as an
advisory engineering-review layer. It reviews source patterns that ordinary
formatters, compilers, and linters cannot fully judge, including persistence,
concurrency, errors, logging, security, testing, and performance measurement.

Nice Code does not replace Thingd's authoritative checks. Cargo, Clippy,
Biome, CodeQL, cargo-deny, Rust and Node tests, documentation checks, and
storage benchmarks continue to determine whether a change is ready to merge.

## Run it locally

Install the published Nice Code package at the version declared by Thingd's
root `package.json`:

```bash
pnpm install --frozen-lockfile
```

From the Thingd repository, run the configured checker:

```bash
pnpm check:nice-code
```

For machine-readable output:

```bash
NICE_CODE_FORMAT=json pnpm check:nice-code
NICE_CODE_FORMAT=sarif pnpm check:nice-code
```

The runner uses the exact Nice Code npm version in the workspace lockfile and
scans the full source tree. Nice Code is a development-only dependency; it is
not part of the Thingd runtime or published packages and does not modify
Thingd source files.

## CI behavior

The Nice Code workflow runs on pull requests, pushes to `main`, and manual
dispatches. It uploads SARIF findings to GitHub Code Scanning and preserves a
JSON report as a workflow artifact.

The initial policy is advisory: findings are visible but do not block a pull
request. A missing checker or malformed report does fail the review job so
that the signal cannot silently disappear. Findings are classified as
`FAIL`, `WARN`, `REVIEW`, or `PASS`; `REVIEW` means human context is required,
not that the code is incorrect.

The repository-specific [`.nice-code.json`](../.nice-code.json) excludes only
generated, vendored, and build-output paths. It contains no broad category
disables or unexplained exceptions.

## Thingd review focus

When reviewing storage or runtime changes, use Nice Code findings to prompt
checks for WAL and recovery integrity, queue concurrency, error propagation,
sensitive logging, benchmark methodology, test strength, and public API
boundaries. Treat those findings as review prompts; correctness still comes
from the Thingd contract, fault-injection, differential, and benchmark suites.
