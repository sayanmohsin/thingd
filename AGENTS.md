# Agent Notes

This is the public engine/SDK repo for `thingd`. The cloud provider
(`thingd-cloud`) is maintained in a separate private repo.

## Two-repo structure

```txt
thingd (public)
  - Rust engine, Node.js SDK, MCP server, CLI, Docker image
  - public docs (quickstart, MCP reference, agent setup, FAQ, etc.)
  - open source tests and benchmarks

thingd-cloud (private)
  - hosted SaaS backend (auth, billing, tenants, dashboards)
  - managed MCP gateway
  - engine planning docs (docs/thingd/)
  - cloud-specific planning docs (docs/)
```

## What lives where

### thingd (this repo) — public docs
- `QUICKSTART.md`, `agent-setup.md`, `agent-patterns.md`, `why-agents.md`
- `mcp-server.md`, `faq.md`, `runtime-env.md`, `docker-runtime.md`
- `docker-hub.md`, `architecture.md`, `benchmarks.md`, `release.md`
- `cli-reference.md`, `agent-implementation-guide.md`
- Landing page: `docs/index.html`

### thingd-cloud — planning docs (see that repo's AGENTS.md for the full list)
- `docs/thingd/roadmap.md` — full phase plan with checkboxes
- `docs/thingd/handoff.md` — contributor restart guide
- `docs/thingd/ai-primitives.md` — future AI-native primitive plans
- `docs/thingd/persistence-and-native-bindings.md` — storage implementation plans
- `docs/thingd/sidecar-cluster.md` — cluster bridge plans
- `docs/thingd/coding-standards.md` — contributor workflow rules
- `docs/thingd/doc-maintenance.md` — doc hygiene checklist
- `docs/thingd/vision.md` — product vision and design philosophy
- `docs/thingd/cli.md` — CLI phase planning

## Cross-repo rules

- **Planning changes** → update in `thingd-cloud/docs/thingd/` only.
- **Public doc changes** → update in `thingd/docs/` only.
- When a planning phase is completed, update checkboxes in
  `thingd-cloud/docs/thingd/roadmap.md` and update the public docs in `thingd/docs/`.
- If a feature spans both repos (e.g. MCP gateway), document the public SDK
  surface in `thingd` and the cloud-specific integration in `thingd-cloud`.
- Never duplicate planning status between repos — `thingd-cloud` is the
  single source of truth for roadmap/phase tracking.
- Never commit secrets, API keys, or production credentials to either repo.

## Repository boundaries

### thingd (public) — everything external users need
- **Core engine:** `thingd` Rust crate (zero HTTP/MCP knowledge)
- **Node.js SDK:** `@thingd/sdk` npm package (MCP + REST handlers, three stores)
- **CLI:** `@thingd/cli` npm package (TUI, transports, cluster)
- **Rust sidecar:** `thingd-server` crate (Rust binary, MCP + REST + cluster, ~15MB Docker)
- **API spec:** `docs/api-spec/` — language-agnostic contract for future SDKs
- **Docs:** quickstart, MCP reference, CLI reference, FAQ, benchmarks, architecture
- **Docker:** Rust sidecar (`thingd-server`) with MCP + REST + cluster, ~15MB

### thingd-cloud (private) — hosted instance only
- **Auth, billing, tenants** — user management on top of thingd
- **Rate limiting, quotas** — per-tenant resource controls
- **Managed MCP gateway** — cloud-hosted MCP endpoint
- **Web UI** — connects to local + cloud instances
- **No user-defined functions** — thingd-cloud just hosts thingd instances

### What goes where

| Change | Repo |
|--------|------|
| New engine feature (Rust) | thingd |
| New SDK method (TypeScript) | thingd |
| New MCP tool | thingd |
| New REST endpoint | thingd |
| API spec update | thingd (`docs/api-spec/`) |
| Auth/billing/tenants | thingd-cloud |
| Rate limiting | thingd-cloud |
| Roadmap/phase tracking | thingd-cloud |
| Planning docs | thingd-cloud |
| Public docs | thingd |

### Language-specific SDKs (future)

Each language SDK wraps `thingd` via FFI and implements the API spec:

| Language | FFI | Package |
|----------|-----|---------|
| Node.js | napi-rs | `@thingd/sdk` (this repo) |
| Go | cgo | `thingd-go` (separate repo) |
| Rust | direct crate | `thingd-rust` (separate repo) |
| Flutter | dart:ffi | `thingd-flutter` (separate repo) |

Each SDK implements its own MCP/REST handlers following `docs/api-spec/`.
The API contract lives in thingd public repo as language-agnostic docs.

---

## Project status — early-to-mid stage prototype (0.x track)

### Shipped

| Area | Details |
|------|---------|
| Rust engine | `thingd` — memory + SQLite adapters, FTS5 search, queue lifecycle (lease/ack/nack/dead-letter/delayed/retry), schema migrations v1-v4, graph links, ~74 tests |
| Node.js SDK | `@thingd/sdk` — three drivers: memory (default in-memory TS store), native (napi-rs Rust SQLite), remote/cloud (Streamable HTTP MCP); batch ops, sort/filter/offset, graph links |
| CLI | `@thingd/cli` — TUI dashboard, 30+ subcommands (search, objects, events, queues, links, export/import/snapshot, doctor, bench, install for Cursor/Claude Desktop) |
| MCP server | 27 tools, stdio + Streamable HTTP, audit events to `__thingd:mcp:audit` stream, collection allowlists, read-only mode |
| Docker | Rust sidecar (`thingd-server`) with MCP + REST + cluster, ~15MB |
| Logo/branding | `{thing:d}` monospace SVG, truecolor ANSI CLI logo (orange #e05316 + cyan #00c4d4) |
| SEO | JSON-LD structured data, Twitter Cards, canonical URL, robots.txt, sitemap.xml |
| CI/tooling | semantic-release, biome, lefthook (lint+build on pre-push) |

### Published npm packages
- `@thingd/sdk` — public SDK
- `@thingd/cli` — public CLI
- `@thingd/native` — private (no prebuilts), requires local Rust build

### Published Rust crate
- `thingd` — [crates.io](https://crates.io/crates/thingd) — Rust engine primitives with optional SQLite adapter

All three publish in lockstep via semantic-release.

---

## Tech stack

| Tool | Version (see config files for exact pinned versions) |
|------|---------|
| Node.js | >= 24.0.0 (check `package.json` `engines`) |
| pnpm | See `package.json` `packageManager` |
| Rust | edition 2024 (see `rustfmt.toml`) |
| Biome | See `biome.json` devDependencies |
| TypeScript | See root `package.json` devDependencies |
| semantic-release | See root `package.json` devDependencies |
| Lefthook | See root `package.json` devDependencies |

---

## Key config

### Biome (`biome.json`)
- `recommended: true` + `useBlockStatements: "error"` (all if/for/while must have braces)
- `noUnusedImports`, `noUnusedVariables`: `"error"`
- `noUnusedFunctionParameters`: `"warn"`
- `noCommonJs`: `"error"` (no CJS allowed)
- `noDoubleEquals`: `"error"`
- `noNonNullAssertion`: `"warn"`
- Trailing commas: `"es5"` (objects, arrays — NOT function params/type decls)
- Semicolons: always, quotes: double, line width: 100
- Ignores: `target`, `node_modules`, `dist`, `packages/thingd-cli/src/dashboard/public/assets`

### Rustfmt (`rustfmt.toml`)
- Edition 2024, max_width 100, stable-only options
- `match_block_trailing_comma = true`, `remove_nested_parens = true`
- Use field init shorthand, use try shorthand

### Lefthook (`lefthook.yml`)
- Pre-push hook runs: `pnpm check` (biome) + `pnpm build` (parallel)

---

## Build & test

```bash
pnpm build                    # build all packages
pnpm check                    # biome check
pnpm check:write              # biome auto-fix
pnpm test                     # all tests (node + cli + package + rust)
pnpm test:node                # thingd SDK tests
pnpm test:cli                 # thingd-cli tests
pnpm test:rust                # cargo test --workspace
pnpm test:local               # check → build → node+cli+package tests
pnpm bench:rust               # cargo bench (in-memory + sqlite)
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

---

## Publishing process

Releases are done via `semantic-release` (GitHub Actions workflow in
`.github/workflows/release.yml`). The workflow:
1. Builds native addon on matrix (macOS, ubuntu, windows)
2. Runs `pnpm release` which triggers semantic-release
3. Semantic-release uses `release.config.mjs`:
   - `tagFormat: "v${version}"`, branch: `main`
   - Analyzes commits via conventionalcommits
   - Publishes all 3 packages to npm via `@semantic-release/exec`
   - Creates GitHub release

Pre-release manual publish (fallback):
- Anchor version via git tag (e.g., `v0.20.0`), update all 3 package.jsons
- Run: `pnpm --filter @thingd/sdk publish --access public --no-git-checks`
- Deprecate old versions on npm if needed

Semantic-release bumps all 3 packages in sync via `prepareCmd`.

---

## Logo & branding

### SVG logo
`docs/logo.svg` — `{thing:d}` monospace, orange braces `#e05316`, cyan letters
`#00c4d4`. Also used as favicon (`docs/favicon.svg`).

### CLI logo
`packages/thingd-cli/src/logo.ts` — `logoText()` / `logoLine()` with truecolor
ANSI sequences for terminal display (same colors). Shown in `--help`, TUI
startup, and TUI header.

---

## Code conventions

- **TypeScript** — ESM only (`"type": "module"`), no CJS (`noCommonJs: error`)
- **Formatting** — double quotes, semicolons always, trailing commas es5
- **Braces** — all if/for/while must use `{}` (even single-line bodies)
- **Imports** — no unused imports (error)
- **Rust** — edition 2024, `cargo fmt` must pass
- **Commits** — conventionalcommits format (`fix:`, `feat:`, `refactor:`, etc.)
  for semantic-release to detect version bumps
- **Commit automation** — do NOT commit unless explicitly asked

---

## Spec-first development

`docs/api-spec/` is the single source of truth for the API contract.
Every feature starts there before any code is written.

### Required order (every feature)

1. [ ] `docs/api-spec/data-model.md` — types/interfaces
2. [ ] `docs/api-spec/rest-api.md` — REST endpoint(s)
3. [ ] `docs/api-spec/mcp-tools.md` — MCP tool(s)
4. [ ] `docs/api-spec/errors.md` — error codes
5. [ ] `docs/api-spec/search.md` — search query syntax (if applicable)

If you can't find where in the spec to add it, you haven't designed it yet.

### All layers must implement the spec

After the spec is updated, implement in **every** layer:

| Layer | Location | Language |
|-------|----------|----------|
| Engine | `crates/thingd/src/` (store.rs, model.rs, in_memory.rs, sqlite.rs) | Rust |
| Native binding | `packages/thingd-native/native/src/lib.rs` | Rust (napi) |
| Sidecar REST | `crates/thingd-server/src/rest.rs` | Rust (axum) |
| Sidecar MCP | `crates/thingd-server/src/mcp.rs` | Rust |
| Node.js SDK | `packages/thingd/src/thingd.ts` | TypeScript |
| Node.js REST | `packages/thingd/src/rest/server.ts` | TypeScript |
| Node.js MCP | `packages/thingd/src/mcp/tools.ts` | TypeScript |
| Node.js stores | `packages/thingd/src/stores/*.ts` | TypeScript |
| CLI | `packages/thingd-cli/src/index.ts` | TypeScript |
| Tests | `packages/thingd/test/`, `packages/thingd-cli/test/`, `crates/thingd/` | TS + Rust |

**Never ship a feature in only one layer.** If the Node.js SDK gets a new
endpoint, the Rust sidecar must too — and vice versa. The spec ensures both
implement the same contract.

### Common miss patterns

- **MCP tool count** — update `README.md` badge, `docs/mcp-server.md` Current Status, `docs/faq.md`
- **Sidecar REST gap** — every REST endpoint in `docs/api-spec/rest-api.md` must exist in `crates/thingd-server/src/rest.rs`
- **Sidecar MCP gap** — every MCP tool in `docs/api-spec/mcp-tools.md` must exist in `crates/thingd-server/src/mcp.rs`
- **Native binding type** — when adding napi methods, update the `NativeThingStoreBinding` type in `native-thing-store.ts`
- **Sort/filter params** — if the Rust `ListObjectsOptions` gets new fields, the native binding `list_objects_json` and TypeScript `listObjects` must pass them through

### Publishing process notes

- All versions stay below 1.0 (0.x series) indefinitely during early stage
- GitHub release titles must match the semver tag
- Semantic-release bumps all packages in sync via `prepareCmd` in `release.config.mjs`

---

## Repository skills

Skills are reusable agent instructions for common workflows. Load one with
`/skill <name>` in the conversation.

- **`upgrade-deps-and-benchmark`** (`.opencode/skills/upgrade-deps-and-benchmark.md`)
  — Audit all pnpm + Rust deps, bump to latest, run benchmarks, report
  performance change. Run this every few weeks or before a release.

---

## Next steps

1. GitHub Pages will auto-update with new logo, SEO tags, sitemap, robots.txt
   on next deploy
2. (Optional) Post Reddit r/node draft from `docs/reddit-drafts.md`

Roadmap and feature planning are tracked in the private `thingd-cloud` repo.
