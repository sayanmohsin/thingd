# Thingd Engine and Cloud Audit Remediation

## Status

Draft — implementation-ready structure, with the explicit design decisions in the “Risks, assumptions, and open questions” section resolved before the affected phase starts.

## Summary

Make the Thingd public engine and Thingd Cloud behave consistently across MCP, REST, Node SDKs, browser/edge clients, command CLI, interactive CLI/TUI, and NLQ. The first priority is security and tenant correctness: instance routing, route-aware scopes, and REST/MCP write-approval parity. The next priorities are client/CLI correctness, NLQ credential and quota handling, runtime/backup reliability, billing enforcement, and contract documentation.

This plan is cross-repository. It does not include unrelated dashboard frontend changes currently present in the working tree.

## Problem and goals

### Problems

- Cloud REST authorization is based primarily on HTTP method, so read-only `POST` operations can require write scope.
- REST proxying and MCP proxying can prefer a global runtime URL over the selected instance runtime.
- REST mutations do not currently apply the same write-approval flow as MCP.
- Cloud scope classification covers an older MCP tool set while the engine currently exposes more tools.
- The zero-dependency REST client lacks instance routing and returns hardcoded queue counts.
- The command and interactive CLIs inconsistently recognize `userToken` versus legacy `token` configuration.
- Interactive CLI Cloud discovery can fail after current login and can overwrite newer credentials.
- NLQ exposes provider credentials to the browser and does not enforce usage limits server-side.
- Provisioning, backup, billing, and documentation contain stale or incomplete behavior.

### Goals

1. Preserve tenant and instance isolation across every gateway and client.
2. Make MCP and REST authorization and approval outcomes equivalent.
3. Make every supported SDK and CLI path target the selected instance reliably.
4. Make NLQ server-controlled, read-only, metered, and plan-aware.
5. Make runtime provisioning and backups safe for multiple instances.
6. Make tool counts, route contracts, CLI behavior, and Cloud docs agree with code.
7. Leave a regression-test matrix that prevents the same drift returning.

## Scope and non-goals

### In scope

- Public engine API and tool inventory verification.
- Cloud MCP and REST gateway routing, auth, scopes, approvals, rate limits, and audit events.
- `@thingd/sdk` and `@thingd/client` Cloud behavior.
- Command CLI and interactive CLI/TUI Cloud flows.
- NLQ/TLQ execution, credential handling, usage, and entitlements.
- Runtime provisioning, backup source/consistency, billing webhook validation, and docs.
- Cross-repository tests and handoff documentation.

### Non-goals

- Redesigning the ThingD data model.
- Replacing MCP or REST protocols.
- Broad UI redesign unrelated to NLQ, instance selection, usage, or error states.
- Dependency upgrades or benchmarking unless separately requested.
- Filing external GitHub issues or making production changes as part of implementation without explicit authorization.

## Repository ownership

| Change | Repository |
|---|---|
| Engine/tool/API contracts, SDKs, REST client, CLI, public docs | `thingd` |
| Cloud auth, tenants, gateways, scopes, approvals, provisioning, backups, billing, usage, Cloud docs | `thingd-cloud` |
| Cross-repository feature | Public contract first in `thingd`; integration second in `thingd-cloud` |

Planning status remains authoritative in `thingd-cloud/docs/thingd/`.

## Repository evidence

| Area | File | Relevant behavior |
|---|---|---|
| Engine tool count | `thingd/packages/thingd/src/constants.ts` | Current constant reports 46 tools; several public docs still report 36. |
| MCP registry | `thingd/packages/thingd/src/mcp/tools.ts` | Includes indexes, vector search, schema, NLQ, aggregation, and scheduler tools. |
| REST client | `thingd/packages/thingd-client/src/client.ts` | Has no instance slug option; active/dead job counts return `0`. |
| Cloud REST gateway | `thingd-cloud/apps/control-plane/src/v1/v1.controller.ts` | Uses method-level scopes and global runtime URL precedence. |
| Cloud MCP gateway | `thingd-cloud/apps/control-plane/src/mcp/mcp.controller.ts` | Has write approval, but also global runtime URL precedence. |
| Cloud MCP scopes | `thingd-cloud/apps/control-plane/src/http/mcp-scopes.ts` | Explicit map omits newer tools; unknown tools use `admin:keys`. |
| Interactive CLI | `thingd/packages/thingd-cli/src/interactive.ts` | Cloud discovery checks legacy `token`; newer login stores `userToken`. |
| Command CLI | `thingd/packages/thingd-cli/src/commands/mcp-connect.ts` | Connection guard checks `config.token` instead of current credential precedence. |
| NLQ UI | `thingd-cloud/apps/control-plane-web/src/pages/project/ProjectNlq.tsx` | Sends configured LLM key directly from browser to provider. |
| Usage | `thingd-cloud/apps/control-plane/src/usage/usage.service.ts` | Increment methods exist, but no execution path currently calls them. |
| Provisioning | `thingd-cloud/apps/control-plane/src/runtime/provisioner.ts` | Container identity, copy pagination, and health-port behavior need correction. |
| Backups | `thingd-cloud/apps/control-plane/src/backup/backup.service.ts` | Uses legacy workspace source and requires consistency review. |
| Billing | `thingd-cloud/apps/control-plane/src/billing/billing.service.ts` | Billing behavior remains incomplete; webhook signature verification is missing. |

## Requirements

### Functional requirements

1. Resolve a target instance before proxying and use that instance’s runtime URL.
2. Honor `X-Instance-Slug` only after validating session access.
3. Classify REST operations semantically, not only by HTTP method.
4. Maintain an explicit scope classification for every MCP tool.
5. Apply write approval consistently to REST and MCP mutations.
6. Support `userToken`, explicit token, environment token, and legacy credentials through one resolver.
7. Preserve project and instance context in CLI config, SDK requests, and dashboard launches.
8. Implement real queue count behavior or expose an explicit unsupported capability.
9. Keep provider credentials server-side for Cloud NLQ.
10. Enforce NLQ/agent usage limits atomically and server-side.
11. Restrict NLQ-generated operations to validated read-only intents.
12. Provision and health-check the exact instance runtime that was created.
13. Back up the active instance database with a documented consistency and restore procedure.
14. Verify billing webhooks and make entitlement checks authoritative.
15. Update contracts, public docs, Cloud docs, and tests after behavior stabilizes.

### Non-functional requirements

- Preserve tenant isolation under concurrent requests.
- Do not log bearer tokens, LLM keys, or raw secrets.
- Keep MCP independent from REST in the public engine.
- Avoid duplicate planning status between repositories.
- Keep changes narrow and compatible with existing legacy Cloud config where practical.
- Every security or routing decision must have an automated regression test.

## Proposed design

### 1. Shared target-instance resolution

Add one Cloud-side resolver used by both controllers:

1. Resolve the authenticated project.
2. Resolve the requested instance slug, if present.
3. Validate requested instance access.
4. Otherwise resolve the session-authorized/default instance.
5. Require a runtime URL belonging to that resolved instance.
6. Reject ambiguous or unavailable targets instead of falling back to a global runtime.

The global runtime URL may remain as an explicit local-development fallback only when no tenant/instance runtime is being selected.

### 2. Operation-aware authorization

Create a single operation classification table shared by REST and MCP policy code.

Read operations include search, vector search, schema, aggregate, timeseries, NLQ, list/get/count, and queue inspection. Write operations include object writes/deletes, event append, queue mutation, link mutation, index creation, and scheduler mutation. Destructive operations must be marked separately for audit and approval policy.

Unknown MCP tools must fail as unsupported and fail CI until classified; they must not silently require `admin:keys`.

### 3. Unified approval flow

Extract approval creation into a shared Cloud service boundary. REST and MCP both pass the same approval input: project, instance, API key, request ID, action/tool name, request body, and actor metadata.

When approval is enabled for a write:

- return `202` with `pending_approval` and an approval ID;
- do not proxy to the runtime;
- record an audit event;
- make approval/replay idempotent by request ID.

### 4. Client and CLI connection model

Use one credential resolver with this precedence:

1. explicit flag/API option;
2. environment token;
3. current `userToken`;
4. legacy JWT `token`;
5. legacy project `apiKey`.

Add `instanceSlug` support and `X-Instance-Slug` emission to the zero-dependency client. Ensure Node SDK, command CLI, interactive CLI, and dashboard use the same URL and instance context.

### 5. Server-side NLQ

Move Cloud NLQ execution behind a Cloud endpoint/service:

1. authenticate the user/project;
2. authorize the plan and instance;
3. check usage limit;
4. load provider credentials server-side;
5. retrieve schema;
6. ask the provider for a constrained read-only intent;
7. validate the intent against an allowlist;
8. execute the read operation;
9. increment usage exactly once;
10. return the result without provider credentials.

The interactive CLI may continue using the public SDK NLQ path for local engines. Cloud CLI NLQ must use the Cloud-authenticated path and must not receive provider keys.

### 6. Runtime, backup, and billing hardening

- Name containers by instance ID/slug plus a collision-safe suffix.
- Persist assigned runtime ports and probe the persisted port.
- Make copy/export pagination advance offset/cursor and filter the intended collection.
- Back up active `thingd_instances` data using a consistency-safe snapshot/checkpoint process.
- Add restore verification.
- Verify webhook signatures before processing and make events idempotent.
- Use one entitlement source for UI display and API enforcement.

## Contracts and examples

### REST authorization examples

| Request | Required scope | Approval |
|---|---|---|
| `POST /v1/search` | `memory:read` | No |
| `POST /v1/search/vector` | `memory:read` | No |
| `POST /v1/aggregate` | `memory:read` | No |
| `POST /v1/nlq` | `memory:read` + NLQ entitlement | No |
| `PUT /v1/objects/:collection/:id` | `memory:write` | Instance policy |
| `POST /v1/events/:stream` | `events:write` | Instance policy |
| `POST /v1/queues/:queue/push` | `queue:write` | Instance policy |

### Pending approval response

```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "pending_approval",
    "approvalId": "approval_..."
  },
  "id": null
}
```

REST should expose an equivalent structured JSON error/result contract rather than silently proxying the mutation.

### Client instance selection

```ts
new ThingdClient({
  url: "https://api.thingd.cloud",
  authToken: "md_user_...",
  instanceSlug: "production",
});
```

The request must include `X-Instance-Slug: production`.

## Implementation impact

| Phase | Files/modules | Change | Done when |
|---|---|---|---|
| 1. Contract inventory | `thingd` MCP/REST specs, constants, tool registry; Cloud scope map | Generate/verify one 46-tool and REST operation matrix; classify read/write/destructive | Every tool/route has one classification and drift test design. |
| 2. Instance routing | Cloud MCP/V1 controllers, proxy pipeline, instance DTO/service | Centralize target resolution; remove accidental global URL precedence | Two-instance isolation tests pass through MCP and REST. |
| 3. Auth/scopes/approval | Cloud `mcp-scopes.ts`, V1 controller, approval service/tests | Route-aware REST scopes; complete MCP map; shared approval flow | Read-only POST operations work; REST writes cannot bypass approval. |
| 4. SDK/client parity | `packages/thingd-client`, Node HTTP store/types/tests | Add instance header support; fix queue counts/errors and method parity | Client tests pass against two instance contexts and error envelopes. |
| 5. Command CLI | `cloud-api.ts`, `cloud-config.ts`, `cloud.ts`, `mcp-connect.ts`, CLI tests | Unify credential resolver and generated MCP configuration | Login, logout, status, and `mcp connect` work with current and legacy configs. |
| 6. Interactive CLI/TUI | `interactive.ts`, dashboard launch/server, CLI tests | Fix discovery, persistence, switching, NLQ/aggregate/TS, maintenance counts | TUI can connect/select/switch instances and operate all supported paths. |
| 7. NLQ/usage | Cloud NLQ config/controller/service, Studio NLQ page, usage service/tests | Server-side provider calls, intent validation, quota enforcement, no browser key | Provider key never reaches browser; usage and limits are enforced. |
| 8. Runtime/backups | `provisioner.ts`, `backup.service.ts`, related tests | Correct instance identity, health port, pagination, snapshots, restore checks | Provision/restart/backup/restore tests pass for multiple instances. |
| 9. Billing/entitlements | Billing service/controller, webhook tests, shared plans | Signature validation, idempotency, authoritative feature gates | Forged/duplicate webhooks and unauthorized paid features are rejected. |
| 10. Docs and release validation | Public and Cloud docs, roadmap/handoff | Reconcile tool counts/routes/status and record final handoff | Docs match implementation and all required checks pass. |

## Test and verification matrix

| Behavior | Test location | Verification |
|---|---|---|
| REST read-only POST scope | Cloud V1 controller tests | Read key succeeds on search/aggregate/schema/NLQ; write scope is not required. |
| REST write approval | Cloud V1/approval tests | Mutation returns pending approval and runtime receives no request. |
| MCP scope completeness | Cloud scope tests + generated inventory | All engine tools have explicit scope and classification. |
| Instance isolation | Cloud MCP/V1 controller tests | Instance A cannot reach instance B; selected URL is used. |
| Client instance header | `packages/thingd-client` tests | Header is emitted and error responses remain structured. |
| Queue counts | Client/SDK tests | Counts reflect runtime data and are not hardcoded. |
| Current Cloud token | CLI cloud tests | `userToken` login supports status, connect, and interactive discovery. |
| Legacy Cloud token | CLI cloud tests | Legacy configs continue to work or emit a clear migration error. |
| Interactive switching | CLI integration/TUI harness | Switching changes instance context for all reads/writes. |
| Interactive analytics | CLI tests | Schema, aggregate, timeseries, and NLQ return correct results/errors. |
| NLQ secret handling | Cloud controller/frontend tests | Provider key is never in API/browser payloads. |
| NLQ quota | Usage/service tests | Concurrent requests cannot exceed the plan limit. |
| Provisioning | Runtime provisioner tests | Name, port, readiness, and restart metadata remain instance-specific. |
| Backup/restore | Backup integration tests | Active instance data restores without WAL-related loss. |
| Billing webhooks | Billing tests | Invalid signatures reject; duplicate valid events are idempotent. |
| Documentation parity | Script/CI check | Counts, routes, and setup examples agree with source inventory. |

Run the smallest affected checks first, then:

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

Do not claim a check passed unless it was run. Separate sandbox/listener/dependency failures from product failures.

## Risks, assumptions, and open questions

### Assumptions

- The current engine tool registry is authoritative; the implementation must verify whether the 46-tool count includes all transport-visible tools before changing docs.
- `X-Instance-Slug` remains the intended REST routing mechanism.
- Local SDK NLQ behavior may remain separate from Cloud-hosted NLQ behavior.
- Existing legacy Cloud credentials should remain readable during migration.

### Decisions required before implementation

1. Should REST write approval return the same JSON-RPC-shaped response as MCP, or a REST-native `202` envelope?
2. Should Cloud NLQ be a new endpoint, an authenticated MCP-only flow, or both?
3. Which plans may use NLQ, and are limits daily or monthly? Current UI text and usage field names appear inconsistent.
4. Is a global runtime URL permitted in production, or should it be rejected whenever instance routing is enabled?
5. Should engine feedback be filed automatically through GitHub, or only prepared and reviewed first?
6. What backup consistency guarantee is required: WAL checkpoint, filesystem snapshot, or runtime export?

Until these are answered, treat the affected phase as blocked rather than guessing.

## Handoff instructions

The implementing agent must:

1. Read this plan and both repositories’ `AGENTS.md` files completely.
2. Verify each repository-evidence item before editing.
3. Implement in phase order; do not skip contract or authorization work.
4. Preserve unrelated dashboard changes in the current `thingd` worktree.
5. Update public contracts before Cloud integration when an API change is required.
6. Add regression tests with each behavior change.
7. Audit all listed docs after implementation.
8. Run the required validation and report exact failures.
9. Stop and report any unresolved decision above instead of inventing behavior.
10. Do not commit, push, file issues, or modify production systems unless separately authorized.

### Handoff prompt

Implement `/Users/sayanmohsin/Space/Programming/ancatag/thingd/.opencode/plans/thingd-cloud-audit-remediation-plan.md` in dependency order. Read the complete plan and applicable `AGENTS.md` files first. Preserve unrelated changes, update contracts/tests/docs with implementations, run the specified validation, and stop with an evidence-based blocker report if any open decision is unresolved.
