# Agent instructions for public thingd

thingd is the public, open-source Rust data engine, Node.js SDK, browser/edge
client, sidecar MCP/REST server, CLI, native binding, and self-hosting docs.
thingd Cloud is a separate private repository built on the released public API.

## Repository boundary

- Put engine, SDK, CLI, MCP, REST, API-specification, self-hosting, and public
  contributor changes in this repository.
- Put auth, billing, tenants, hosted provisioning, Cloud operations, internal
  product planning, engine roadmap, private audits, and cross-repository
  handoff documents in `thingd-cloud`.
- A feature spanning both repositories gets its public contract here and its
  hosted integration in `thingd-cloud`.
- Never add private planning, customer data, credentials, or Cloud-only
  operational details to this public repository.

## Source of truth

- Public API contracts: `docs/api-spec/`.
- Public contributor and integration guidance: `docs/agent-implementation-guide.md`.
- Public runtime behavior: source, tests, and user-facing docs in this repo.
- Private engine planning and cross-repository handoff: `thingd-cloud/docs/thingd/`.

## Implementation rules

For a public feature, update every affected layer: Rust engine, native binding,
sidecar REST/MCP, Node SDK, browser client, CLI, tests, and public docs. Keep
MCP independent from REST and keep Cloud concerns out of the engine crate.

Required workflow:

1. Inspect `git status` and preserve unrelated changes.
2. Read the applicable API spec and trace existing behavior before editing.
3. Update contracts, implementation, adapters, tests, and docs together.
4. Run focused checks, then the proportionate repository checks.
5. Review the diff and public-document boundary before handoff.

## Commands

```bash
pnpm check
pnpm build
pnpm test:node
pnpm test:cli
pnpm test:rust
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Use `pnpm check:docs` for documentation and public-boundary validation.

## External actions

Do not commit, push, open GitHub issues, deploy, or contact external systems
unless the user explicitly requests it. Record an engine friction point in the
private Cloud handoff/audit log when working from Cloud, then ask before filing
an external issue.

## Documentation boundary

- **TypeScript**: ESM only (`"type": "module"`), no CJS (biome `noCommonJs: error`)
- **Formatting**: double quotes, semicolons always, trailing commas es5, line width 100
- **Braces**: all if/for/while must use `{}` (even single-body lines, biome `useBlockStatements: error`)
- **Imports**: no unused imports (error)
- **Rust**: edition 2024, `cargo fmt` must pass
- **Commits**: conventionalcommits (`fix:`, `feat:`, `refactor:`, `BREAKING CHANGE:`) for Release Please

## Branch workflow

Use `main` as the integration, production, and release branch. Create feature
branches from `main` and open every feature, fix, documentation, or maintenance
pull request against `main`. Squash-merge completed work into `main` with a
conventional commit title.

## Keeping AGENTS.md healthy

This file should stay useful but not become a dump. Rules:

1. **Update alongside code** — add learnings in the same commit as the change, not later.
2. **Prune aggressively** — when a section drifts from "hard-won lesson" to "encyclopedia reference", extract it into its own doc and link to it.
3. **Use small sections** — if a section exceeds 15-20 lines, it needs its own doc file.
4. **No binary files** — paths to screenshots/diagrams go elsewhere (issue comments, design docs).
5. **Delete stale entries** — when you upgrade deps or fix a workaround, remove the old guidance.

## Branch and pull request workflow

Use `main` as the integration branch in this repo. Create feature branches from
`main` and open every feature, fix, documentation, or maintenance pull request
against `main`. Squash-merge completed work into `main`; `main` remains the
production and release branch.

Use semantic branch prefixes such as `feature/<name>`, `fix/<name>`,
`docs/<name>`, `refactor/<name>`, `test/<name>`, or `chore/<name>`.

## Common miss patterns

- **AGENTS.md test counts** — `pnpm test:node`/`:cli`/`:rust` counts in the Commands section must match `package.json` test scripts. Run each to verify before committing.
- **AGENTS.md version examples** — `Version specifier propagation` examples must match the current workspace `[workspace.package].version` in `Cargo.toml`.
- **Release dependency sync** — run `pnpm check:cargo-versions`; Release Please uses `scripts/sync-cargo-local-dependency-versions.mjs` to update every local Cargo path dependency, including `thingd-schema`.
- **MCP tool count** — update in `packages/thingd/src/constants.ts` only; VitePress docs use `{{ $themeConfig.mcpToolCount }}` from `docs/.vitepress/config.ts`
- **Sidecar REST gap** — every REST endpoint in `docs/api-spec/rest-api.md` must exist in `crates/thingd-server/src/rest.rs`
- **CLI import for DBs** — `thingd import <connection-string>` calls sidecar `POST /v1/connectors/{type}/pull`. Document flags in `docs/cli-reference.md`.

- **Sidecar MCP sync** — every MCP tool added to `packages/thingd/src/mcp/tools.ts` must also exist in `crates/thingd-server/src/mcp.rs`
- **Native binding type** — update `NativeThingStoreBinding` in `native-thing-store.ts` when adding napi methods
- **Sort/filter params** — Rust `ListObjectsOptions` changes must propagate to native binding `list_objects_json` and TypeScript `listObjects`

## Lessons from sidecar hardening session (June 2026)

### Doc audit after every change

After any implementation, audit ALL public docs for staleness. This session found:
- **AGENTS.md** claimed sidecar MCP was a "stub with 5 tools" (now 32 tools)
- **rest-api.md** had wrong error format, missing `body` field in GET response, wrong links endpoint
- **mcp-tools.md** had incorrect tool breakdown (claimed 12/12/3, actual 16 read-only / 11 write of which 3 destructive)
- **errors.md** referenced non-existent `ThingDError` class (SDK throws plain `Error`)

Checklist of files to audit:
- `AGENTS.md` — remove stale claims about stubs or incomplete features
- `docs/api-spec/rest-api.md` — response shapes, error format, endpoint paths
- `docs/api-spec/mcp-tools.md` — tool count, breakdown, schemas
- `docs/api-spec/errors.md` — error codes, SDK error types
- `docs/api-spec/search.md` — search behavior and syntax
- `docs/api-spec/data-model.md` — entity shapes
- `README.md` — tool count badge, feature descriptions
- `docs/mcp-server.md` — status, tool descriptions
- `docs/faq.md` — tool count, feature questions
- `docs/cli-reference.md` — CLI commands and flags (`thingd mcp connect`, `install --antigravity`)
- `docs/agent-setup.md` — cloud MCP setup path
- `docs/quickstart.md` — cloud setup path

### Version specifier propagation

When `[workspace.package].version` bumps in `Cargo.toml`, path deps with exact version specs must follow:
```
crates/thingd-server/Cargo.toml: thingd = { path = "../thingd", version = "0.48" }
packages/thingd-native/Cargo.toml: thingd = { path = "../../crates/thingd", version = "0.48" }
```
Search with `rg 'version = "0\.xx"' crates/ packages/` after every version bump.

### Rust patterns learned

| Pattern | Do this | Not this |
|---------|---------|----------|
| Static data with `json!()` | `static FOO: LazyLock<Vec<T>> = LazyLock::new(\|\| vec![...])` | `const FOO: &[T] = &[...]` — `json!()` is not const-compatible |
| Auth middleware | Read token from `AppState` (set once at startup from config) | `std::env::var("THINGD_AUTH_TOKEN")` per request (TOCTOU race) |
| Middleware using `State` | `middleware::from_fn_with_state(Arc::clone(&state), handler)` | `middleware::from_fn(handler)` — won't compile with State extractor |
| Request timeout in axum 0.8 | `ServiceBuilder::new().layer(HandleErrorLayer::new(...)).layer(TimeoutLayer::new(...)).into_inner()` | Bare `TimeoutLayer` — error type is incompatible with axum's Router |
| CI pre-push fix | `cargo fmt --all && git add -A && git commit --amend --no-edit && git push --force-with-lease` | Using `--no-verify` skips important checks |

### CLI testability

- All commands must use `writeJson(context.stdout, data, context.pretty)` not `console.log()`
- TUI should call `db.listQueues()` instead of accessing `store.queues` via `as unknown as` casts
- The `call_mcp_with()` test helper pattern in `mcp.rs` is reusable for new MCP tool tests

## Docker No pnpm, no Node.js in the image — just a static Rust binary on `scratch`. The binary is compiled on the CI runner and copied into the build context before `docker buildx`.

## Publishing

Releases via Release Please on main. All four npm packages (`@thingd/sdk`, `@thingd/cli`, `@thingd/native`, and `@thingd/client`) and the Rust crate (`thingd`) publish in lockstep. Version tag format: `thingd-v${version}`. Release Please opens a release PR; merging it triggers the publication workflow.

> **Save GitHub Actions credits:** Release only through the `development` → `main` merge. Each push to `main` with releasable commits (`feat:`, `fix:`) triggers a release workflow. Squash feature branches into `development`, batch related work, and use a regular merge into `main` so one release covers the full batch.

Manual first publish (for new scoped packages):
```bash
pnpm --filter @thingd/sdk publish --access public --no-git-checks
pnpm --filter @thingd/cli publish --access public --no-git-checks
pnpm --filter @thingd/native publish --access public --no-git-checks
cargo publish -p thingd --features persistent,search
```

## Skills

- `/skill upgrade-deps-and-benchmark` — audit all deps, bump to latest, run benchmarks

> Audit-after-change is not a skill — use the checklist under "Doc audit after every change" above.
Public docs may explain Cloud as a product and document public Cloud endpoints,
but must not expose private roadmap status, internal audits, customer/tenant
operations, or private implementation plans. Run the boundary checker before
handoff. Do not add a second private planning copy here.
