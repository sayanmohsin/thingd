---
name: phase-tracker
description: Audit and update Thingd and Thingd Cloud phase tracking when roadmap status, handoffs, or release evidence changes.
---

# Phase Tracker

Use this skill when the user asks to update phases, roadmaps, handoffs, readiness,
or project status across the public `thingd` and private `thingd-cloud` repos.

## Workflow

1. Inspect both repositories' status before editing and preserve unrelated work.
2. Read the Cloud phase registry at `thingd-cloud/docs/agent-work/phase-tracker.md`
   and the relevant detailed phase documents.
3. Keep status values consistent: `planned`, `active`, `implemented`, `verified`,
   `released`, or `blocked`.
4. Update the registry and detailed handoff together. Move completed or blocked
   plans out of `agent-work/active/` when appropriate.
5. Keep public contracts and engine work in `thingd`; keep hosted operations,
   Studio, Publish, billing, and private planning in `thingd-cloud`.
6. Never infer release or production readiness from a typecheck alone. Record
   tests, package/release, deployment, and live verification separately.
7. Run the checker in `scripts/check-phase-tracker.mjs`, then the smallest
   repository documentation checks affected by the change.

Do not commit, push, deploy, or contact external systems unless the user
explicitly requests it. Do not copy Cloud-private operational details into
public documentation.
