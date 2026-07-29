---
name: spec-driven-planning
description: Turn product ideas, bug reports, and feature requests into repository-aware, implementation-ready specifications and handoff plans. Use when the user wants to think through a change before coding, create a detailed plan for another agent, define contracts and acceptance criteria, or save a spec for later implementation by Codex, ChatGPT, OpenCode, or another coding agent.
---

# Spec-Driven Planning

Create a precise, reviewable contract for implementation. Stay in planning mode unless the user explicitly asks for code changes. The final artifact must allow a different agent to implement the work without reconstructing decisions from the conversation.

## Workflow

### 1. Frame the request

Extract:

- problem and user value;
- desired behavior and observable outcomes;
- scope, non-goals, and constraints;
- affected repository or repositories;
- known assumptions and unresolved decisions.

Ask focused questions only when the answer would materially change the design. Otherwise make a clearly labeled assumption and continue.

### 2. Inspect the repository

Before proposing implementation details, read the applicable `AGENTS.md`, `RTK.md`, README, package manifests, relevant API or design docs, and current implementation/tests. Use `rg` and `rg --files` for discovery. Check repository status and preserve unrelated changes.

Trace the current behavior through all relevant layers. Do not invent filenames, APIs, conventions, test commands, or system boundaries when the repository can establish them. Cite repository-relative file paths and line numbers where useful.

For cross-repository work, state the ownership boundary and identify which changes belong in each repository. Keep planning status in its authoritative repository.

### 3. Design the change

Describe the smallest coherent design that satisfies the request. Include:

- current behavior and gap;
- proposed behavior and invariants;
- data model, API, CLI, MCP, or event contract changes;
- compatibility and migration implications;
- authorization, validation, failure, concurrency, and recovery behavior;
- affected files/modules and why;
- alternatives considered and the reason for the chosen approach.

Separate confirmed facts, decisions, assumptions, and open questions. Mark behavior that still requires runtime verification.

### 4. Build the execution plan

Order work by dependencies, not by presentation. Each phase must name its deliverables, affected files, and verification. Prefer phases such as:

1. contract/specification updates;
2. core implementation;
3. adapters and public interfaces;
4. tests and fixtures;
5. documentation and rollout;
6. full validation.

For every phase, define a completion condition. Include the smallest relevant commands first and broader checks afterward. Never claim a command passed unless it was actually run.

### 5. Review the spec

Challenge the result before handing it off:

- Is every requirement testable?
- Are edge cases and failure modes covered?
- Are all affected layers and contracts listed?
- Could an implementer make two incompatible choices from this document?
- Does the scope contain accidental refactors or unrelated cleanup?
- Are rollout, migration, backward compatibility, and security implications addressed?

Resolve issues in the spec or list them explicitly as blockers. Do not hide uncertainty behind confident wording.

## Output format

When the user wants a durable artifact, save it as `.opencode/plans/active/<short-kebab-case-name>.md` unless they specify another path. The same Markdown file must be readable by Codex and OpenCode. Do not create a second divergent copy merely for another agent.

After saving, report the repository-relative path prominently. The implementing agent must be able to load the plan with that exact path; do not refer only to a descriptive plan title.

Use this lifecycle so the next agent can identify the current work:

- `.opencode/plans/active/` — draft or approved plans awaiting implementation;
- `.opencode/plans/completed/` — plans whose implementation and final validation are complete;
- `.opencode/plans/blocked/` — plans paused because a decision, dependency, or external change is required.

Create or update the plan in `active/`. Do not treat Markdown files directly under `.opencode/plans/` as current by default; they are legacy plans and must be explicitly selected or migrated.

Set `## Status` to `Draft`, `Ready for implementation`, `Blocked`, or `Complete`. When implementation finishes, the coding skill moves the file from `active/` to `completed/`; when blocked, it moves the file to `blocked/` and records the blocker.

Use this structure:

```markdown
# <Feature or change>

## Status
Draft | Ready for implementation | Blocked

## Summary
<one-paragraph description>

## Problem and goals
...

## Scope and non-goals
...

## Repository evidence
| Area | File | Relevant behavior |
|---|---|---|

## Requirements
### Functional requirements
...
### Non-functional requirements
...

## Proposed design
...

## Contracts and examples
<schemas, request/response examples, state transitions, or pseudocode>

## Implementation impact
| Phase | Files/modules | Change | Done when |
|---|---|---|---|

## Test and verification matrix
| Behavior | Test location | Verification |
|---|---|---|

## Risks, assumptions, and open questions
...

## Handoff instructions
<direct instructions for the implementing agent>
```

For a chat-only response, use the same sections but omit repository writes. End with a concise handoff prompt that names the spec path, implementation constraints, required validation, and the rule to stop and report blockers instead of guessing.

## Handoff rules

The implementing agent should:

- read the complete spec and applicable repository instructions first;
- verify the repository evidence before editing;
- implement in the listed dependency order;
- update contracts, implementations, adapters, tests, and docs together when required;
- run the specified validation and report failures with evidence;
- avoid expanding scope or changing unrelated files;
- stop for decisions explicitly marked as blockers.

Do not commit, push, open issues, or modify production systems as part of planning unless the user separately requests those actions.
