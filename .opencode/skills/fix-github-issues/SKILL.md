---
name: fix-github-issues
description: |
  Use when the user says to fix GitHub issues one by one. Covers the full
  workflow: list open issues, pick one, investigate, implement, test, commit,
  and close. Supports both public and private repos.
---

# Fix GitHub Issues (One by One)

## Workflow

### 1. List all open issues across repos

```bash
gh issue list --repo sayanmohsin/thingd --state open --json number,title,createdAt,labels --limit 30
gh issue list --repo sayanmohsin/thingd-cloud --state open --json number,title,createdAt,labels --limit 30
```

If no issues exist, report to the user.

### 2. Pick the next issue

Sort by `createdAt` ascending (oldest first). Start with the oldest open issue
to work through the backlog systematically.

### 3. Investigate

Read the issue body fully:

```bash
gh issue view <number> --json title,body,labels,comments
```

Explore the codebase to understand the root cause. Look at relevant source
files, tests, and docs. Form a plan before writing code.

### 4. Implement

Apply the fix. Follow the repo conventions:
- Rust (edition 2024, cargo fmt, clippy -D warnings)
- TypeScript (ESM, double quotes, semicolons, trailing commas es5, line width 100)
- Conventional commits (fix:, feat:, refactor:, chore:)

### 5. Verify

```bash
cargo test --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check
pnpm check
pnpm test:node
pnpm test:cli
```

### 6. Commit and push

```bash
git add -A
git commit -m "type(scope): short description

Closes #<number>"
git pull --rebase
git push
```

The `Closes #<number>` in the commit message auto-closes the issue on push.

### 7. Verify issue is closed

```bash
gh issue view <number> --json number,state,title
```

Proceed to the next open issue.

## Relevant commands

| Action | Command |
|--------|---------|
| List open issues | `gh issue list --state open --json number,title,createdAt` |
| View issue | `gh issue view <number> --json title,body,labels,comments` |
| Close issue manually | `gh issue close <number> -c "reason"` |
| Git commit to auto-close | Include `Closes #<number>` in commit message |
| List repos | `gh repo list sayanmohsin --limit 30 --json name,isPrivate` |
