# Upgrade All Dependencies and Benchmark

Upgrade all pnpm and Rust dependencies to latest, then run benchmarks and report gain/loss.

## Workflow

### 1. Audit pnpm dependencies

```bash
# Check all root devDependencies
npm view @biomejs/biome @modelcontextprotocol/sdk @semantic-release/commit-analyzer @semantic-release/exec @semantic-release/github @semantic-release/npm @semantic-release/release-notes-generator @types/node conventional-changelog-conventionalcommits cross-spawn lefthook semantic-release typescript version

# Check frontend deps
npm view @sveltejs/vite-plugin-svelte svelte vite version

# Example deps
npm view @nestjs/common @nestjs/core @nestjs/platform-express @nestjs/cli version

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
# Check resolved versions in Cargo.lock
for crate in rusqlite chrono serde serde_json tempfile napi napi-derive napi-build; do
  cargo search "$crate" 2>/dev/null | head -1
done

# Or via crates.io API:
for crate in rusqlite chrono serde serde_json tempfile napi napi-derive napi-build; do
  echo -n "$crate: "
  curl -sL "https://crates.io/api/v1/crates/$crate" -H "User-Agent: thingd/0.1" |
    python3 -c "import sys,json; d=json.load(sys.stdin); print(d['crate']['max_version'])"
done
```

### 3. Bump versions

```bash
# pnpm deps — edit root package.json, then:
pnpm install

# Rust deps — edit Cargo.toml files, then:
cargo update
```

### 4. Benchmark before/after

```bash
# Rust (5000 iterations)
pnpm bench:rust

# Rust smoke test (100 iterations)
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
git add -A
git commit -m "chore: bump deps — <summary of version changes>"
```

## Expected output format

Report the comparison in ops/s as a table:

| Operation | Before (ops/s) | After (ops/s) | Δ |
|---|---|---|---|
| in-memory object_put | ... | ... | ±x% |
| in-memory object_get | ... | ... | ±x% |
| ... | ... | ... | ... |
| sqlite-memory object_put | ... | ... | ±x% |
| ... | ... | ... | ... |
| sqlite-file object_put | ... | ... | ±x% |
| ... | ... | ... | ... |

Flag any breaking changes or notable changelog entries.
