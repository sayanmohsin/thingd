# Phase management

Thingd work is tracked by customer outcome and release evidence, not by a
single list of unfinished ideas.

## Repository ownership

- `thingd` owns the public engine, SDKs, CLI, MCP/REST contracts, storage
  behavior, and self-hosting documentation.
- `thingd-cloud` owns hosted auth, tenancy, provisioning, Studio, Publish,
  billing, and operational controls.
- A cross-repository feature must define its public contract here before Cloud
  integration work is considered ready.

## Phase states

- **Planned:** direction agreed; implementation has not started.
- **Active:** implementation or verification is underway.
- **Implemented:** code exists locally; release evidence is incomplete.
- **Verified:** acceptance checks pass for the stated scope.
- **Released:** packaged and published through the normal release workflow.
- **Blocked:** a named dependency or failed check prevents progress.

Do not call a phase complete from a typecheck alone. Record implementation,
tests, package/release state, deployment state, and live verification separately.

## Working order

Keep two or three active outcomes at a time. Finish reliability and recovery
evidence before adding new surfaces. Defer scale, billing, and advanced
Enterprise work until the core customer journey and production trust gates pass.

Detailed Cloud phase status is maintained privately in its phase tracker. Public
changes should reference only the released contract and user-facing behavior.
