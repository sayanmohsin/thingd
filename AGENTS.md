# Agent Notes

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
  thingd-cli/        ← @thingd/cli (CLI + TUI + transports)
  thingd-native/     ← @thingd/native (napi-rs binding to thingd crate)
```

## Spec-first development

`docs/api-spec/` is the single source of truth. Every feature starts there before any code.

**Required order:** data-model.md → rest-api.md → mcp-tools.md → errors.md → search.md

**Implement in every layer** (never ship one layer only):

| Layer | Location |
|-------|----------|
| Engine (Rust) | `crates/thingd/src/` (store.rs, model.rs, in_memory.rs, sqlite.rs) |
| Native binding (Rust napi) | `packages/thingd-native/native/src/lib.rs` |
| Sidecar REST (Rust axum) | `crates/thingd-server/src/rest.rs` |
| Sidecar MCP (Rust) | `crates/thingd-server/src/mcp.rs` |
| Node.js SDK (TypeScript) | `packages/thingd/src/thingd.ts` |
| Node.js REST | `packages/thingd/src/rest/server.ts` |
| Node.js MCP | `packages/thingd/src/mcp/tools.ts` |
| Node.js stores | `packages/thingd/src/stores/*.ts` |
| CLI | `packages/thingd-cli/src/index.ts` |
| Tests | `packages/thingd/test/`, `packages/thingd-cli/test/`, `crates/thingd/` |

**Sidecar MCP** implements all 27 tools natively (`crates/thingd-server/src/mcp.rs`) via a registry-based dispatch. The Node.js SDK MCP (`packages/thingd/src/mcp/tools.ts`) remains the primary reference and adds auth gating.

**Sidecar cluster** returns real config (mode, peers, discovery). Real cluster forwarding/leader election logic is in `packages/thingd-cli/src/mcp/cluster.ts`.

**MCP layer is independent** — no imports from REST. The stdio MCP server (`thingd mcp`) runs standalone without the REST layer.

## Commands

```bash
pnpm build                    # build all packages (TypeScript + Rust native)
pnpm check                    # biome lint
pnpm check:write              # biome auto-fix
pnpm test:node                # 53 Node SDK tests
pnpm test:cli                 # 39 CLI tests
pnpm test:rust                # cargo test --workspace (75 tests)
pnpm test:local               # check → build → node+cli+package tests
pnpm bench:rust               # full Rust benchmark (in-memory + sqlite)
pnpm bench:rust:smoke         # quick Rust benchmark (100 iters)
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

## Pre-push hook (lefthook, runs in parallel)

1. `pnpm check` (biome)
2. `pnpm build` (recursive — TypeScript + Rust native)
3. `cargo fmt --all --check`
4. `cargo clippy --workspace --all-targets --all-features -- -D warnings`

All four must pass before push completes. If clippy or fmt fails, fix and amend.

## Key conventions

- **TypeScript**: ESM only (`"type": "module"`), no CJS (biome `noCommonJs: error`)
- **Formatting**: double quotes, semicolons always, trailing commas es5, line width 100
- **Braces**: all if/for/while must use `{}` (even single-line bodies, biome `useBlockStatements: error`)
- **Imports**: no unused imports (error)
- **Rust**: edition 2024, `cargo fmt` must pass
- **Commits**: conventionalcommits (`fix:`, `feat:`, `refactor:`, `BREAKING CHANGE:`) for semantic-release

## Common miss patterns

- **MCP tool count** — update in `README.md` badge, `docs/mcp-server.md` Current Status, `docs/faq.md`
- **Sidecar REST gap** — every REST endpoint in `docs/api-spec/rest-api.md` must exist in `crates/thingd-server/src/rest.rs`
- **Sidecar MCP gap** — every MCP tool in `docs/api-spec/mcp-tools.md` must exist in `crates/thingd-server/src/mcp.rs`
- **Native binding type** — update `NativeThingStoreBinding` in `native-thing-store.ts` when adding napi methods
- **Sort/filter params** — Rust `ListObjectsOptions` changes must propagate to native binding `list_objects_json` and TypeScript `listObjects`

## Docker

The Docker image is built from `docker-context/Dockerfile` using a prebuilt `thingd-server` binary. No pnpm, no Node.js in the image — just a static Rust binary on `scratch`. The binary is compiled on the CI runner and copied into the build context before `docker buildx`.

## Publishing

Releases via `semantic-release` on main. All three npm packages (`@thingd/sdk`, `@thingd/cli`, `@thingd/native`) and the Rust crate (`thingd`) publish in lockstep. Version tag format: `v${version}`.

Manual first publish (for new scoped packages):
```bash
pnpm --filter @thingd/sdk publish --access public --no-git-checks
pnpm --filter @thingd/cli publish --access public --no-git-checks
pnpm --filter @thingd/native publish --access public --no-git-checks
cargo publish -p thingd --features sqlite
```

## Skills

- `/skill upgrade-deps-and-benchmark` — audit all deps, bump to latest, run benchmarks
