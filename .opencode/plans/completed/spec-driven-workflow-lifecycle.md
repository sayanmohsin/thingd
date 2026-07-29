# Shared Spec-Driven Workflow Lifecycle

## Status

Complete

## Summary

Ensure the `spec-driven-planning` and `spec-driven-coding` skills are discoverable from both the `thingd` and `thingd-cloud` repositories, use a predictable plan lifecycle, and hand work cleanly from planning to implementation. New plans belong in `active/`; completed plans belong in `completed/`; blocked plans belong in `blocked/`.

## Problem and goals

The workflow needs to be unambiguous across ChatGPT/Codex and OpenCode. An agent starting in either repository must know where to find the current plan, which plans are historical, which skill to load, what validation to run, and where to move the plan after implementation.

Goals:

- expose both skills from each repository's `.opencode/skills/` directory;
- make `.opencode/plans/active/` the default location for implementation-ready plans;
- define explicit completed and blocked destinations;
- preserve the exact spec path and handoff contract in every generated plan;
- prevent an implementing agent from silently selecting legacy or completed plans.

## Scope and non-goals

In scope:

- shared skill discovery links and skill instructions in `thingd` and `thingd-cloud`;
- plan directory conventions and handoff instructions;
- filesystem and documentation validation for the workflow.

Out of scope:

- implementing product features described by unrelated plans;
- automatically migrating existing root-level plans whose status is unclear;
- committing, pushing, deploying, or changing production systems.

## Repository evidence

| Area | File | Relevant behavior |
|---|---|---|
| Canonical skills | `/Users/sayanmohsin/Space/Programming/ancatag/thingd/skills/spec-driven-planning/SKILL.md` | Defines planning output and lifecycle |
| Canonical skills | `/Users/sayanmohsin/Space/Programming/ancatag/thingd/skills/spec-driven-coding/SKILL.md` | Defines implementation, validation, and plan movement |
| thingd OpenCode discovery | `/Users/sayanmohsin/Space/Programming/ancatag/thingd/.opencode/skills/` | Project-local skill links |
| Cloud OpenCode discovery | `/Users/sayanmohsin/Space/Programming/ancatag/thingd-cloud/.opencode/skills/` | Separate Git worktree needs its own discovery links |
| Plan lifecycle | `/Users/sayanmohsin/Space/Programming/ancatag/thingd/.opencode/plans/` and `/Users/sayanmohsin/Space/Programming/ancatag/thingd-cloud/.opencode/plans/` | Active, completed, blocked directories |

## Requirements

### Functional requirements

1. Both repositories must expose `spec-driven-planning` and `spec-driven-coding` at `.opencode/skills/<name>/SKILL.md`.
2. Planning must write new durable plans to `.opencode/plans/active/<name>.md`.
3. Coding must inspect `active/` first and treat root-level plans as legacy unless explicitly named.
4. Coding must update status and validation evidence before moving a finished plan.
5. Successful implementation must move the plan to `.opencode/plans/completed/<name>.md`.
6. A genuine blocker must move the plan to `.opencode/plans/blocked/<name>.md` with a resume condition.
7. Every active plan must contain `## Handoff instructions` with the exact spec path, validation commands, and the completed-folder move instruction.

### Non-functional requirements

- Keep one canonical skill implementation where possible; avoid divergent skill text.
- Preserve unrelated working-tree changes.
- Keep the workflow tool-neutral enough for ChatGPT/Codex and OpenCode.
- Do not claim validation passed unless the command was run.

## Proposed design

Use the canonical skill files under `thingd/skills/`. Expose them to OpenCode from both Git worktrees through their project-local `.opencode/skills/` paths. Use this plan state machine:

```text
active/  --validated success-->  completed/
   |
   +-- genuine blocker ------->  blocked/
```

The plan remains in `active/` while implementation is in progress. The implementing agent must preserve the filename and move only after updating status and evidence.

## Contracts and examples

Planning handoff:

```text
Use $spec-driven-coding to implement the approved spec at:
/Users/sayanmohsin/Space/Programming/ancatag/thingd/.opencode/plans/active/spec-driven-workflow-lifecycle.md

From the Cloud repository, use:
/Users/sayanmohsin/Space/Programming/ancatag/thingd-cloud/.opencode/plans/active/spec-driven-workflow-lifecycle.md
```

Completion move:

```text
mv .opencode/plans/active/spec-driven-workflow-lifecycle.md \
   .opencode/plans/completed/spec-driven-workflow-lifecycle.md
```

Blocked move:

```text
mv .opencode/plans/active/spec-driven-workflow-lifecycle.md \
   .opencode/plans/blocked/spec-driven-workflow-lifecycle.md
```

## Implementation impact

| Phase | Files/modules | Change | Done when |
|---|---|---|---|
| 1. Skill contracts | `thingd/skills/spec-driven-planning/SKILL.md`, `thingd/skills/spec-driven-coding/SKILL.md` | Define discovery, active-plan selection, validation, and movement rules | Both skills state the same lifecycle |
| 2. OpenCode discovery | `thingd/.opencode/skills/`, `thingd-cloud/.opencode/skills/` | Expose both skills from both worktrees | All four `SKILL.md` paths resolve |
| 3. Plan directories | Both repositories' `.opencode/plans/` | Ensure `active/`, `completed/`, and `blocked/` exist | Directory checks pass |
| 4. Verification | Both repositories | Check links, required instructions, and whitespace | All validation commands pass |
| 5. Handoff | This plan | Update status and move this file after verification | File is in `completed/` or `blocked/` |

## Test and verification matrix

| Behavior | Test location | Verification |
|---|---|---|
| thingd skill discovery | `thingd/.opencode/skills/` | `test -f .opencode/skills/spec-driven-planning/SKILL.md && test -f .opencode/skills/spec-driven-coding/SKILL.md` |
| Cloud skill discovery | `thingd-cloud/.opencode/skills/` | Equivalent `test -f` checks from the Cloud root |
| Plan directories | Both `.opencode/plans/` trees | `test -d active && test -d completed && test -d blocked` |
| Handoff contract | Both canonical skill files and this plan | `rg -n "Handoff instructions|plans/completed|plans/blocked|plans/active"` |
| File quality | Both repositories | `git diff --check` |

## Risks, assumptions, and open questions

- The two repositories are currently sibling directories, so project-local skill links resolve in this workspace. If they are distributed independently, replace the links with a packaged/shared installation or synchronized copies.
- Existing files directly under `.opencode/plans/` have mixed or unknown status and should not be moved automatically.
- The official Python skill validator may be unavailable if the `yaml` module is not installed; record that as an environment limitation rather than a product failure.

## Handoff instructions

1. Load this spec using the spec-driven-coding skill (`/skill spec-driven-coding`).
2. Read the complete plan from the repository you are implementing: `/Users/sayanmohsin/Space/Programming/ancatag/thingd/.opencode/plans/active/spec-driven-workflow-lifecycle.md` for thingd or `/Users/sayanmohsin/Space/Programming/ancatag/thingd-cloud/.opencode/plans/active/spec-driven-workflow-lifecycle.md` for thingd-cloud.
3. Verify repository assumptions before editing, including both `thingd` and `thingd-cloud`.
4. Implement phase by phase, validating after each phase.
5. Run validation: `test -f .opencode/skills/spec-driven-planning/SKILL.md && test -f .opencode/skills/spec-driven-coding/SKILL.md`, `test -d .opencode/plans/active && test -d .opencode/plans/completed && test -d .opencode/plans/blocked`, equivalent checks from `/Users/sayanmohsin/Space/Programming/ancatag/thingd-cloud`, and `git diff --check` in both repositories.
6. On success: update status to `Complete` and move this file to `.opencode/plans/completed/spec-driven-workflow-lifecycle.md`.
   Because this plan is exposed in both worktrees, also move the corresponding Cloud path to `/Users/sayanmohsin/Space/Programming/ancatag/thingd-cloud/.opencode/plans/completed/spec-driven-workflow-lifecycle.md`.
7. On blocker: update status to `Blocked`, record the blocker and resume condition, and move this file to `.opencode/plans/blocked/spec-driven-workflow-lifecycle.md`. Also move the corresponding Cloud path to `/Users/sayanmohsin/Space/Programming/ancatag/thingd-cloud/.opencode/plans/blocked/spec-driven-workflow-lifecycle.md`.
8. Report the result with changed files, validation outcomes, and remaining risks.
9. Do not commit, push, or deploy unless explicitly asked.
