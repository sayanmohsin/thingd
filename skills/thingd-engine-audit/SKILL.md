---
name: thingd-engine-audit
description: Audit thingd and thingd Cloud end to end for missing features, parity gaps, security issues, routing bugs, stale contracts, storage reliability, and operational risks. Use from Codex or OpenCode for audits of the Rust engine, sidecar, MCP gateway, REST gateway, Node SDKs, browser client, command CLI, interactive CLI/TUI, NLQ, provisioning, backups, billing, dependency/benchmark concerns, or Cloud handoff readiness.
---

# Thingd Engine and Cloud Audit

Perform a read-only, evidence-based audit across the public engine repository and private Cloud repository. Report prioritized findings with exact file and line references, affected paths, reproduction conditions, recommended fixes, and regression tests. Do not implement fixes unless explicitly asked. This workflow is tool-neutral and should work when invoked from either Codex or OpenCode.

## Repository scope

- Public engine: `/Users/sayanmohsin/Space/Programming/ancatag/thingd`
- Cloud application: `/Users/sayanmohsin/Space/Programming/ancatag/thingd-cloud`

Respect the boundary: engine, SDK, MCP, REST, CLI, and public docs belong in `thingd`; auth, tenants, billing, rate limits, provisioning, backups, Cloud gateways, and planning docs belong in `thingd-cloud`. Never duplicate planning status or expose secrets.

This is the canonical shared skill for Codex and OpenCode. Keep tool-specific adapters thin and point them here instead of maintaining a second audit checklist.

## Workflow

### 1. Establish context

Read applicable `AGENTS.md`, `RTK.md`, API specs, package scripts, and repository status. Preserve unrelated working-tree changes. Use `rg` and `rg --files` for discovery. State whether the audit is engine-only, Cloud-only, cross-repository, or handoff-focused. Default to read-only inspection.

### 2. Build the capability matrix

Compare implementation against the contracts in this order:

1. `docs/api-spec/data-model.md`
2. `docs/api-spec/rest-api.md`
3. `docs/api-spec/mcp-tools.md`
4. `docs/api-spec/errors.md`
5. `docs/api-spec/search.md`

Trace every capability through:

```text
engine -> native binding -> sidecar REST -> sidecar MCP
       -> Node SDK -> browser/edge client -> command CLI
       -> interactive CLI/TUI -> Cloud REST/MCP gateway -> docs/tests
```

Record implementation, typing, authorization, tests, and documentation for each layer. Derive MCP counts from the actual registry; do not trust stale constants or docs.

### 3. Audit MCP and REST separately, then compare

MCP checks: tool registration and schemas, read/write/destructive classification, batches, unknown tools, approval, rate limiting, audit records, instance selection, and runtime forwarding.

REST checks: every documented route, payload/query encoding, response/error envelopes, route-specific authorization, read-only POST routes, write-approval parity, `X-Instance-Slug`, readiness, and proxy failures.

Flag method-only authorization such as `POST => write` when the endpoint is semantically read-only. Verify REST and MCP produce equivalent security decisions.

### 4. Audit SDK and client parity

Compare `@thingd/sdk`, `@thingd/client`, and REST implementations for method coverage, payload names, optional fields, pagination, filters, sorting, batches, queue counts, aggregate/timeseries/vector/NLQ behavior, instance headers, errors, 404 handling, and Cloud URL derivation. Treat hardcoded values such as unconditional zero counts as correctness bugs unless explicitly documented.

### 5. Audit both CLI modes

Audit command CLI and interactive CLI/TUI separately.

Command CLI: login/logout, token revocation, legacy config fallback, project/instance commands, `mcp connect`, URL selection, token precedence, dashboard flags, and generated editor configs.

Interactive CLI/TUI: automatic Cloud connection, `userToken` versus legacy `token`, multi-project/instance selection, persistence, switching/logout, objects/events/queues/links, schema/search/aggregate/timeseries/NLQ, import/export, maintenance, dashboard launch, polling, reconnect, errors, and cleanup.

Require both modes to use one shared credential resolver. Confirm selected instance context survives every connection and dashboard transition.

### 6. Audit NLQ/TLQ as a security surface

Treat “TLQ” as NLQ unless defined otherwise. Verify LLM credential storage, browser exposure, read-only intent validation, schema retrieval, provider failures, usage counting, plan limits, and parity between Studio, REST, MCP, SDK, and CLI. Report browser-exposed provider keys and unenforced limits as high priority.

### 7. Audit Cloud operations

Inspect tenant and instance isolation, runtime URL precedence, provisioning names/ports/readiness/reconciliation, backup sources and WAL consistency, restore behavior, billing completeness, webhook signatures/idempotency, rate limiting, audit logging, and usage resets. Check that active instance storage is used instead of legacy workspace data.

Use the existing OpenCode role boundaries when proposing fixes:

- Backend: `thingd-cloud/apps/control-plane/src/**`, SDK/database usage, auth, billing, and proxy services.
- Frontend: `thingd-cloud/apps/control-plane-web/src/**`, API clients, navigation, and UI state.
- Infrastructure: Docker, scripts, CI, environment templates, hooks, and workspace configuration.

Follow established conventions: typed `ThingDConnection`, batch operations for bulk work, existing test mocks, shared UI primitives, `useAsyncData` patterns, minimal multi-stage images, musl engine targets, and multi-tenant Docker configuration.

### 8. Audit docs and handoff state

Check tool counts, route lists, phase/status claims, package names, setup commands, CLI flags, Cloud setup paths, feature availability, and error formats in both repositories. For cross-repo work inspect `thingd-cloud/docs/thingd/roadmap.md`, `sidecar-cluster.md`, and `handoff.md`; do not update planning docs unless asked.

## OpenCode compatibility

OpenCode may invoke this skill from `.opencode/skills/thingd-engine-audit.md`; that adapter must defer to this file. OpenCode plans and role agents remain useful task context, but this file owns the cross-layer audit method, feedback workflow, and finding format.

### 9. Capture engine feedback

When an audit confirms a public-engine bug, missing API, performance issue, docs
gap, or integration friction:

1. Prepare a focused GitHub issue for `sayanmohsin/thingd` with reproduction,
   expected behavior, actual behavior, severity, and affected layers.
2. If `gh` is authenticated and issue filing is within the user's request,
   file it immediately; otherwise provide the issue-ready body without claiming
   it was filed.
3. Record the finding and issue number in
   `thingd-cloud/docs/thingd/engine-feedback-log.md` when cloud planning is in
   scope. Keep roadmap and handoff status only in `thingd-cloud`.
4. If a workaround is implemented in Cloud, link it to the public issue and
   document the workaround's removal condition.

## Finding format

Report in priority order:

```text
[P0/P1/P2/P3] Title
Files: absolute paths with line numbers
Paths: MCP, REST, SDK, CLI, TUI, or NLQ paths affected
Evidence: what the code does
Impact: what breaks or becomes unsafe
Fix direction: concise remediation
Test: regression test proving the fix
```

- P0: security boundary bypass, tenant isolation failure, data loss, or broad outage.
- P1: major broken path, incorrect authorization/routing, or silently wrong data.
- P2: important parity, reliability, test, or documentation gap.
- P3: cleanup, ergonomics, or low-risk consistency issue.

Separate confirmed findings from hypotheses and mark unverified runtime behavior explicitly.

## Validation

Run the smallest relevant checks first, then broader checks proportionate to risk:

```bash
pnpm check
pnpm build
pnpm test:node
pnpm test:cli
pnpm test:rust
pnpm typecheck
pnpm lint
pnpm test
```

Record sandbox/environment failures separately from product failures. Do not modify code merely to make checks pass.

For dependency upgrades or performance work, use the repository's separate
OpenCode `upgrade-deps-and-benchmark` workflow as the detailed benchmark
procedure. Do not silently upgrade dependencies during an audit.

For one-by-one GitHub issue implementation, use the repository's separate
OpenCode `fix-github-issues` workflow after the audit has produced and triaged
the issue. Do not combine diagnosis and implementation unless the user asks.

## Handoff output

End with an executive summary, prioritized findings, confirmed working areas, implementation phases and dependencies, regression-test matrix, documentation/planning updates, and an explicit statement of whether files changed.

For implementation handoffs, sequence work as:

1. contract/tool inventory;
2. instance routing;
3. auth/scopes/approval parity;
4. SDK/client parity;
5. command and interactive CLI fixes;
6. NLQ security and usage enforcement;
7. provisioning/backups;
8. billing/entitlements;
9. docs and full validation.
