# Agent Notes

This is the public engine/SDK repo for `thingd`. The cloud provider
(`thingd-cloud`) is maintained in a separate private repo.

## Two-repo structure

```txt
thingd (public)
  - Rust engine, Node.js SDK, MCP server, CLI, Docker image
  - public docs (quickstart, MCP reference, agent setup, FAQ, etc.)
  - open source tests and benchmarks

thingd-cloud (private)
  - hosted SaaS backend (auth, billing, tenants, dashboards)
  - managed MCP gateway
  - ALL planning/roadmap docs for thingd engine development
```

## What lives where

### thingd (this repo) — public docs
- `QUICKSTART.md`, `agent-setup.md`, `agent-patterns.md`, `why-agents.md`
- `mcp-server.md`, `faq.md`, `runtime-env.md`, `docker-runtime.md`
- `docker-hub.md`, `architecture.md`, `benchmarks.md`, `release.md`
- `cli-reference.md`, `agent-implementation-guide.md`
- Landing page: `docs/index.html`

### thingd-cloud — planning docs
- `roadmap.md` — full phase plan with checkboxes
- `handoff.md` — contributor restart guide
- `ai-primitives.md` — future primitive plans (graph links, hybrid search, etc.)
- `persistence-and-native-bindings.md` — storage implementation plans
- `sidecar-cluster.md` — cluster bridge plans
- `coding-standards.md` — contributor workflow rules
- `doc-maintenance.md` — doc hygiene checklist
- `vision.md` — product vision and design philosophy
- `cli.md` — CLI phase planning (original, before extraction)

## Cross-repo rules

- **Planning changes** → update in `thingd-cloud/docs/` only.
- **Public doc changes** → update in `thingd/docs/` only.
- When a planning phase is completed, update checkboxes in
  `thingd-cloud/docs/roadmap.md` and update the public docs in `thingd/docs/`.
- If a feature spans both repos (e.g. MCP gateway), document the public SDK
  surface in `thingd` and the cloud-specific integration in `thingd-cloud`.
- Never duplicate planning status between repos — `thingd-cloud` is the
  single source of truth for roadmap/phase tracking.
- Never commit secrets, API keys, or production credentials to either repo.
