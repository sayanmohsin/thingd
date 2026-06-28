# Skill: audit-after-change

Run after any implementation to catch stale docs, missing endpoints, test
gaps, cross-repo sync issues, and version pin drift. Designed to prevent the
gaps found in the June 2026 sidecar hardening session from recurring.

## Workflow

### 1. Cross-reference REST routes

Check every route in `docs/api-spec/rest-api.md` exists in the sidecar REST:

```bash
# Extract routes from spec vs implementation
rg '### (GET|PUT|POST|DELETE) ' docs/api-spec/rest-api.md
rg '\.route\(' crates/thingd-server/src/rest.rs
```

Flag any mismatch. The spec always wins — if the code is missing a route, add
it. If the spec claims a route but it doesn't exist in the code, the spec is
stale.

### 2. Cross-reference MCP tools

Check every tool in `docs/api-spec/mcp-tools.md` exists in the sidecar MCP:

```bash
rg '"thing_' docs/api-spec/mcp-tools.md | sort
rg '"thing_' crates/thingd-server/src/mcp.rs | sort
```

Both should have the same 27 tools. The spec has the canonical list.

### 3. Stale phrase scan

Search docs for phrases that indicate stale content after a change:

```bash
rg -i 'stub|planned|not implemented|not yet|5 tools|old version|sidecar is'
docs/ README.md AGENTS.md
```

If you find `"stub"` or `"5 tools"` or `"not implemented"` for something that
is now implemented, update the doc.

### 4. Response shape audit

Check that response examples in `docs/api-spec/rest-api.md` match what the
sidecar actually returns. Common mismatches found in June 2026:

- GET `/v1/objects/:id` response missing `body` field
- Error format (`code/message` vs `type/title/status/detail`)
- Flattened body fields vs nested `"body"` key
- Links endpoint uses path param `/{id}` not `?id=` query param

Compare the spec examples against the Rust handler outputs in `rest.rs`.

### 5. thingd-cloud sync

The planning docs in `../thingd-cloud/docs/thingd/` are the single source of
truth for phase tracking. After changes, check:

- `../thingd-cloud/docs/thingd/roadmap.md` — phase completion status,
  deliverables checkboxes. If a new feature completes a phase, check off
  deliverables.
- `../thingd-cloud/docs/thingd/sidecar-cluster.md` — phase checklist,
  implemented routes list, current status section
- `../thingd-cloud/docs/thingd/handoff.md` — if the recommended "next phase"
  has changed

Commit doc updates in the thingd-cloud repo after committing code in thingd.

### 6. Test gap check

Cross-reference tools against test coverage:

```bash
# MCP tool test coverage
rg 'name.*"' crates/thingd-server/src/mcp.rs | rg 'thing_' | sort
rg 'test_mcp_' crates/thingd-server/src/mcp.rs | sort
```

Every tool should have at least one integration test.

### 7. Version pin audit

After a workspace version bump in `Cargo.toml`, check path deps:

```bash
rg 'version = "0\.' crates/thingd-server/Cargo.toml packages/thingd-native/Cargo.toml
```

The version spec must match `[workspace.package].version` in root
`Cargo.toml`.

### 8. Common miss pattern check

Run through the checklist in `AGENTS.md`:

- MCP tool count: update `README.md`, `docs/mcp-server.md`, `docs/faq.md`
- REST gap: every endpoint in spec exists in `rest.rs`
- MCP gap: every tool in spec exists in `mcp.rs`
- Native binding type: update `NativeThingStoreBinding` in
  `native-thing-store.ts` when adding napi methods
- Sort/filter params: propagate Rust `ListObjectsOptions` changes to native
  binding and TypeScript

### 9. Build & test the full stack

```bash
pnpm check        # biome lint
pnpm build        # TypeScript + Rust native
pnpm test:rust    # cargo test --workspace
pnpm test:node    # Node SDK tests
pnpm test:cli     # CLI tests
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

## When to run

- After implementing a new MCP tool or REST endpoint
- After changing any API response shape
- After bumping workspace version
- After any sidecar/server work
- As a pre-PR checklist before pushing
