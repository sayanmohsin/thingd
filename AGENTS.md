# Agent Notes

## Product

**thingd** is a fast object-first data engine for applications and AI agents.
It provides object storage, durable queues, event streams, full-text search,
graph links, and 46 Node MCP tools — all in one binary. Runs embedded (Rust/Node),
as a sidecar MCP server, in Docker, or in Kubernetes.

**thingd Cloud** (at [thingd.cloud](https://thingd.cloud), private repo
`sayanmohsin/thingd-cloud`) is the managed hosted version — same engine, zero
infrastructure. See `AGENTS.md` in that repo for cloud-specific docs.

## Repo boundaries

thingd (this repo, public): Rust engine + Node.js SDK + CLI + Rust sidecar + public docs.
thingd-cloud (private): hosted SaaS (auth, billing, tenants, MCP gateway, planning docs).

| Change goes in | Repo |
|----------------|------|
| New engine feature, SDK method, MCP tool, REST endpoint, API spec, public docs | thingd |
| Auth, billing, tenants, rate limiting, roadmap/phase tracking, planning docs | thingd-cloud |
| Feature spanning both | API surface in thingd, integration in thingd-cloud |

Never duplicate planning status between repos. Never commit secrets to either.

> **Warning:** `docs/sidecar-cluster.md` in this repo duplicates content in
> `thingd-cloud/docs/thingd/sidecar-cluster.md`. The thingd-cloud version is the
> authoritative planning doc — remove the public copy once it's confirmed clean.

## Architecture

```
crates/
  thingd/            ← Rust engine (zero HTTP/MCP knowledge)
  thingd-server/     ← Rust sidecar binary (axum: MCP + REST + cluster, Docker ~15MB)
packages/
  thingd/            ← @thingd/sdk (Node.js SDK: MCP + REST + three stores)
  thingd-client/     ← @thingd/client (zero-dep REST client for browsers/edge)
  thingd-cli/        ← @thingd/cli (CLI + TUI + transports)
  thingd-native/     ← @thingd/native (napi-rs binding to thingd crate)
```

## Spec-first development

`docs/api-spec/` is the single source of truth. Every feature starts there before any code.

**Required order:** data-model.md → rest-api.md → mcp-tools.md → errors.md → search.md

**Implement in every layer** (never ship one layer only):

| Layer | Location |
|-------|----------|
| Engine (Rust) | `crates/thingd/src/` (store.rs, model.rs, in_memory.rs, fjall.rs) |
| Native binding (Rust napi) | `packages/thingd-native/native/src/lib.rs` |
| Sidecar REST (Rust axum) | `crates/thingd-server/src/rest.rs` |
| Sidecar MCP (Rust) | `crates/thingd-server/src/mcp.rs` |
| Node.js SDK (TypeScript) | `packages/thingd/src/thingd.ts` |
| Browser/Edge client | `packages/thingd-client/src/client.ts` |
| Node.js REST | `packages/thingd/src/rest/server.ts` |
| Node.js MCP | `packages/thingd/src/mcp/tools.ts` |
| Node.js stores | `packages/thingd/src/stores/*.ts` |
| CLI | `packages/thingd-cli/src/index.ts` |
| Tests | `packages/thingd/test/`, `packages/thingd-cli/test/`, `crates/thingd/` |

**MCP surfaces** — the Node.js SDK MCP (`packages/thingd/src/mcp/tools.ts`) exposes 46 tools, including 10 SDK-level scheduler tools. The Rust sidecar (`crates/thingd-server/src/mcp.rs`) exposes 36 engine tools; scheduler tools remain Node SDK-only.

**Sidecar cluster** returns real config (mode, peers, discovery). Real cluster forwarding/leader election logic is in `packages/thingd-cli/src/mcp/cluster.ts`.

**Scheduler** — SDK-level module (`packages/thingd/src/scheduler.ts`). Uses existing ObjectStore + QueueStore primitives. No engine changes. Ships in SDK + MCP tools only.

**MCP layer is independent** — no imports from REST. The stdio MCP server (`thingd mcp`) runs standalone without the REST layer.

## Commands

```bash
pnpm build                    # build all packages (TypeScript + Rust native)
pnpm check                    # biome lint
pnpm check:write              # biome auto-fix
pnpm test:node                # 91 Node SDK tests
pnpm test:cli                 # 44 CLI tests
pnpm test:rust                # cargo test --workspace --all-features (226 tests — 43 fjall unit tests*)
pnpm test:local               # check → build → node+cli+package tests
pnpm bench:rust               # full Rust benchmark (in-memory + fjall)
pnpm bench:rust:smoke         # quick Rust benchmark (100 iters)
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

## Pre-push hook (lefthook, runs in parallel)

1. `pnpm check` (biome)
2. `pnpm build` (recursive — TypeScript + Rust native)
3. `pnpm test:node` (Node SDK unit tests — ~2s)
4. `cargo fmt --all --check`
5. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
6. `cargo deny check`

All six must pass before push completes. Heavy tests (`pnpm test:rust`, `pnpm test:cli`) run in CI only.
If clippy or fmt fails, fix and amend. Never use `--no-verify` to bypass pre-push hooks.

## Key conventions

- **TypeScript**: ESM only (`"type": "module"`), no CJS (biome `noCommonJs: error`)
- **Formatting**: double quotes, semicolons always, trailing commas es5, line width 100
- **Braces**: all if/for/while must use `{}` (even single-body lines, biome `useBlockStatements: error`)
- **Imports**: no unused imports (error)
- **Rust**: edition 2024, `cargo fmt` must pass
- **Commits**: conventionalcommits (`fix:`, `feat:`, `refactor:`, `BREAKING CHANGE:`) for semantic-release

## Branch workflow

Use `development` as the integration branch. Create feature branches from
`development`, then squash-merge each completed feature into `development` with
a conventional commit title. Open a release PR from `development` to `main` and
use a regular merge commit rather than a squash merge. CI runs on both branches;
release automation runs only after `main` changes.

## Keeping AGENTS.md healthy

This file should stay useful but not become a dump. Rules:

1. **Update alongside code** — add learnings in the same commit as the change, not later.
2. **Prune aggressively** — when a section drifts from "hard-won lesson" to "encyclopedia reference", extract it into its own doc and link to it.
3. **Use small sections** — if a section exceeds 15-20 lines, it needs its own doc file.
4. **No binary files** — paths to screenshots/diagrams go elsewhere (issue comments, design docs).
5. **Delete stale entries** — when you upgrade deps or fix a workaround, remove the old guidance.

## Branch and pull request workflow

Use `development` as the integration branch in this repo. Create feature
branches from `development` and open every feature, fix, documentation, or
maintenance pull request against `development` — never directly against
`main`. After the work is integrated and ready for release, manually merge
`development` into `main`; `main` remains the production and release branch.

Use semantic branch prefixes such as `feature/<name>`, `fix/<name>`,
`docs/<name>`, `refactor/<name>`, `test/<name>`, or `chore/<name>`.

## Common miss patterns

- **AGENTS.md test counts** — `pnpm test:node`/`:cli`/`:rust` counts in the Commands section must match `package.json` test scripts. Run each to verify before committing.
- **AGENTS.md version examples** — `Version specifier propagation` examples must match the current workspace `[workspace.package].version` in `Cargo.toml`.
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

### Cross-repo sync checklist

After completing thingd work, always check thingd-cloud planning docs:
- `thingd-cloud/docs/thingd/roadmap.md` — phase completion status, deliverables checkboxes
- `thingd-cloud/docs/thingd/sidecar-cluster.md` — phase checkboxes, route lists, feature status
- `thingd-cloud/docs/thingd/handoff.md` — update recommended next phase if changed

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

Releases via `semantic-release` on main. All three npm packages (`@thingd/sdk`, `@thingd/cli`, `@thingd/native`) and the Rust crate (`thingd`) publish in lockstep. Version tag format: `v${version}`. A regular `development` → `main` merge batches the conventional commits accumulated since the previous tag into one release.

> **Save GitHub Actions credits:** Release only through the `development` → `main` merge. Each push to `main` with releasable commits (`feat:`, `fix:`) triggers a release workflow. Squash feature branches into `development`, batch related work, and use a regular merge into `main` so one release covers the full batch.

Manual first publish (for new scoped packages):
```bash
pnpm --filter @thingd/sdk publish --access public --no-git-checks
pnpm --filter @thingd/cli publish --access public --no-git-checks
pnpm --filter @thingd/native publish --access public --no-git-checks
cargo publish -p thingd --features fjall,search
```

## Skills

- `/skill upgrade-deps-and-benchmark` — audit all deps, bump to latest, run benchmarks

> Audit-after-change is not a skill — use the checklist under "Doc audit after every change" above.
