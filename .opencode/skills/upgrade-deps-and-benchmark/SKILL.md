---
name: upgrade-deps-and-benchmark
description: |
  Use this skill when the user asks to upgrade all dependencies, bump versions, audit outdated packages,
  or benchmark performance. Covers both pnpm (Node.js) and Cargo (Rust) dependencies. Audits every
  dependency against latest, bumps version pins, runs Rust and Node benchmarks, and reports
  performance gain/loss. Use ONLY for dependency upgrades + benchmarking workflow.
---

# Upgrade All Dependencies and Benchmark

## Workflow

### 1. Audit pnpm dependencies

```bash
# Turbo-check all pnpm deps against lockfile
for pkg in @biomejs/biome @modelcontextprotocol/sdk @semantic-release/commit-analyzer @semantic-release/exec @semantic-release/github @semantic-release/npm @semantic-release/release-notes-generator @types/node conventional-changelog-conventionalcommits cross-spawn lefthook semantic-release typescript cli-table3 picocolors zod @sveltejs/vite-plugin-svelte svelte vite @nestjs/common @nestjs/core @nestjs/platform-express @nestjs/cli reflect-metadata rxjs tsx; do
  current=$(rg "name: $pkg$" -A5 pnpm-lock.yaml 2>/dev/null | rg "version:" | head -1 | awk '{print $2}')
  latest=$(npm view "$pkg" version 2>/dev/null)
  if [ "$current" != "$latest" ] && [ -n "$current" ]; then
    echo "➜ $pkg: $current → $latest"
  fi
done
```

### 2. Audit Rust dependencies

```bash
for crate in rusqlite chrono serde serde_json tempfile napi napi-derive napi-build; do
  echo -n "$crate: "
  curl -sL "https://crates.io/api/v1/crates/$crate" -H "User-Agent: thingd/0.1" |
    python3 -c "import sys,json; d=json.load(sys.stdin); print(d['crate']['max_version'])"
done
```

### 3. Bump versions in Cargo.toml and root package.json

Edit the version numbers, then:

```bash
pnpm install
cargo update
```

### 4. Benchmark before/after

```bash
# Rust (5000 iterations — the full benchmark)
pnpm bench:rust

# Rust smoke test (100 iterations — quick check)
pnpm bench:rust:smoke

# Node.js benchmark (5000 iterations)
pnpm bench:node
```

### 5. Build & test

```bash
pnpm build
pnpm test:node
pnpm test:cli
pnpm check
```

### 6. Commit

```bash
git add -A && git commit -m "chore: bump deps — <summary>"
```

## Expected output format

Report the comparison in ops/s as a table:

| Backend | Operation | Before (ops/s) | After (ops/s) | Δ |
|---|---|---|---|---|
| in-memory | object_put | ... | ... | ±x% |
| in-memory | object_get | ... | ... | ±x% |
| ... | ... | ... | ... | ... |
| sqlite-memory | object_put | ... | ... | ±x% |
| ... | ... | ... | ... | ... |
| sqlite-file | object_put | ... | ... | ±x% |
| ... | ... | ... | ... | ... |

Flag any breaking changes or notable changelog entries.
