# Coding Standards

This repo uses checked-in project files as the source of truth for future human and AI work.

## TypeScript and JSON

Use Biome for formatting, linting, and safe source actions.

```bash
npm run check
npm run check:write
```

Standards:

- 2-space indentation
- 100-column line width
- double quotes
- semicolons
- trailing commas in JavaScript and TypeScript where Biome supports them
- recommended Biome lint rules
- recommended Biome source actions

## Rust

Use `rustfmt` and Clippy for Rust code.

```bash
npm run rust:check
npm run rust:fmt:check
npm run rust:clippy
npm test
```

Standards:

- Rust 2021 edition
- 100-column line width
- no unsafe code
- public API docs for exported engine types
- workspace-level Rust and Clippy lints
- Rust check/test scripts run with all features enabled so storage adapters are covered
- no panic-heavy or allocation-heavy API design without a clear reason
- prefer explicit result types once storage, IO, or network behavior is introduced

## Documentation

Because this repository is private, keep important design context in normal project files:

- [README.md](../README.md) for the product shape and roadmap
- [docs/vision.md](./vision.md) for project direction
- [docs/architecture.md](./architecture.md) for implementation direction
- [docs/agent-implementation-guide.md](./agent-implementation-guide.md) for app integration guidance for AI agents and contributors
- [docs/persistence-and-native-bindings.md](./persistence-and-native-bindings.md) for Rust persistence and N-API direction
- [docs/benchmarks.md](./benchmarks.md) for storage benchmark commands and interpretation
- [docs/release.md](./release.md) for package publishing and versioning
- this file for coding standards

When implementation behavior changes, update the closest doc in the same change.
