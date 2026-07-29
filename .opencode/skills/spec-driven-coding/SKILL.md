---
name: spec-driven-coding
description: Implement an approved repository-aware specification safely and completely. Use when the user provides a feature spec, implementation plan, or handoff document and wants Codex or OpenCode to execute it with incremental validation, tests, documentation updates, and explicit blocker reporting.
---

# Spec-Driven Coding

Treat the approved specification as the implementation contract. Implement the requested change rather than redesigning it, while correcting factual inconsistencies discovered in the repository and reporting any decision that requires user approval.

## Workflow

### 1. Load the contract

Find and read the complete specification before editing. Prefer the path named by the user; otherwise list Markdown files directly under `.opencode/plans/active/` with `find .opencode/plans/active -maxdepth 1 -type f -name '*.md' | sort`. If there is exactly one candidate, load it. If there are multiple candidates, show the list and ask the user which plan to implement; never guess. Then inspect `docs/specs/` if no active plan exists. Do not select files directly under `.opencode/plans/` unless the user explicitly identifies one; those are legacy plans. Ignore `.opencode/plans/completed/` unless the user asks for historical context, and inspect `.opencode/plans/blocked/` only when resuming blocked work. Also read applicable `AGENTS.md`, `RTK.md`, README files, package manifests, and local contributor instructions.

Confirm the spec has a clear status such as `Ready for implementation`. If it is a draft or blocked, report that and ask for approval before making source changes.

Extract requirements, non-goals, affected files, dependencies, acceptance criteria, test matrix, migration steps, and validation commands. Convert the implementation phases into a checklist and keep it updated as work progresses.

### 2. Verify assumptions

Inspect the repository before writing code. Confirm that referenced files, APIs, types, commands, and architectural boundaries exist. Check `git status` and preserve unrelated user changes.

If the repository contradicts the specification:

- fix harmless stale file references or line-number drift using current evidence;
- stop and report contradictions that change behavior, scope, public contracts, security, data migration, or architecture;
- do not silently invent a replacement design.

### 3. Implement in dependency order

Work phase by phase, normally in this order:

1. contracts, schemas, or API specifications;
2. core implementation and invariants;
3. adapters, bindings, clients, and public interfaces;
4. focused tests and fixtures;
5. documentation and migration or rollout notes;
6. broader validation.

Keep each change focused. Do not perform unrelated refactors, dependency upgrades, formatting sweeps, or cleanup unless the spec requires them. Follow repository conventions for naming, formatting, errors, logging, security, and commits.

### 4. Validate continuously

After each meaningful phase:

- run the smallest relevant formatter, linter, type check, or focused test;
- inspect the diff for accidental changes;
- verify the implementation against the corresponding acceptance criteria;
- record failures and their cause before continuing.

At the end, run the complete validation set named in the spec or repository instructions, proportionate to the change. Distinguish product failures from sandbox, dependency, network, or environment failures. Never claim a check passed unless it was actually run.

### 5. Complete the handoff

Before finishing, verify:

- every requirement is implemented or explicitly marked incomplete;
- every affected layer and public contract is synchronized;
- tests cover happy paths, edge cases, errors, and compatibility behavior;
- documentation and plans are updated where required;
- no unrelated files were changed;
- the final diff is reviewable.

If all acceptance criteria pass, update the plan status to `Complete` and move it from `.opencode/plans/active/<name>.md` to `.opencode/plans/completed/<name>.md`. Preserve the filename. If work cannot continue because of a genuine blocker, update the status to `Blocked`, record the blocker and resume condition, and move the plan to `.opencode/plans/blocked/<name>.md`. Do not move a plan until its status and validation evidence are updated.

Report the result with changed files, tests run and outcomes, remaining risks, and any follow-up work. If the repository uses a progress section in the spec, update it with completed phases and verification evidence.

## Blockers and scope control

Stop and ask for direction when implementation requires a new product decision, changes a public contract not covered by the spec, introduces a destructive migration, reveals a security or data-loss risk, or requires work outside the stated repository boundary.

Do not commit, push, open issues, deploy, or contact external systems unless the user explicitly requests that action. If the user asks for a commit or push, follow the repository's commit and validation rules and report the exact result.

## Handoff prompt (for Codex/planning skill)

At the end of every plan saved to `.opencode/plans/active/`, include a `## Handoff instructions` section with this structure:

```markdown
## Handoff instructions

1. Load this spec using the spec-driven-coding skill (`/skill spec-driven-coding`)
2. Read the complete plan from `.opencode/plans/active/<filename>.md`
3. Verify repository assumptions before editing
4. Implement phase by phase, validating after each phase
5. Run validation: `<command 1>`, `<command 2>`
6. On success: update status to `Complete` and move to `.opencode/plans/completed/<filename>.md`
7. On blocker: update status to `Blocked`, record the blocker and resume condition, and move to `.opencode/plans/blocked/<filename>.md`
8. Report result with changed files, validation outcomes, and remaining risks
9. Do not commit, push, or deploy unless explicitly asked
```

## Final report

Use this compact structure:

```text
Status: complete | partial | blocked
Spec: <repository-relative path>

Implemented:
- <phase or requirement> — <summary>

Files changed:
- <absolute path> — <purpose>

Validation:
- `<command>` — passed/failed/not run

Remaining:
- <risk, blocker, or follow-up; say “none” when empty>
```
