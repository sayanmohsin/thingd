# CLI Roadmap

This document describes the current command-line surface and the remaining
operator/developer CLI phases for `memoryd`.

The project now has runtime CLIs through `@sayanmohsin/memoryd-mcp` and a
first-pass admin CLI through `@sayanmohsin/memoryd-cli`.

## Current CLIs

### `memoryd`

Inspects and mutates local or remote `memoryd` stores through the public SDK.

```bash
node packages/memoryd-cli/dist/index.js status
node packages/memoryd-cli/dist/index.js objects put decisions cli --text "Built the CLI."
node packages/memoryd-cli/dist/index.js queues push embed --payload '{"object":"docs/readme"}'
```

Commands:

```txt
memoryd status
memoryd tools
memoryd search <query>
memoryd objects get <collection> <id>
memoryd objects put <collection> <id> --text <text>
memoryd objects put <collection> <id> --data '{"field":"value"}'
memoryd objects delete <collection> <id>
memoryd events append <stream> <type> [--text <text>] [--data '{"field":"value"}']
memoryd events list [stream]
memoryd queues push <queue> --payload '{"key":"value"}'
memoryd queues claim <queue>
memoryd queues ack <queue> <jobId>
memoryd queues nack <queue> <jobId>
memoryd queues list <queue>
memoryd queues dead <queue>
```

Common options:

```txt
--url <url>          remote memoryd URL. Defaults to MEMORYD_URL
--auth-token <tok>  remote bearer token. Defaults to MEMORYD_AUTH_TOKEN
--path <path>       local database path. Defaults to MEMORYD_PATH or :memory:
--driver <driver>   memory, native, or remote
--pretty            pretty-print JSON output
--limit <n>         result limit for search and list commands
```

Remote sidecar usage:

```bash
MEMORYD_URL=http://127.0.0.1:8757
MEMORYD_AUTH_TOKEN=change-me
node packages/memoryd-cli/dist/index.js status
node packages/memoryd-cli/dist/index.js tools
```

### `memoryd-mcp`

Runs the MCP server over stdio for local MCP clients.

```bash
node packages/memoryd-mcp/dist/cli.js --path :memory:
```

Options:

```txt
--path <path>      memoryd database path. Defaults to MEMORYD_PATH or :memory:
--driver <driver> memory or native. Defaults to MEMORYD_DRIVER or memory
-h, --help        show help
```

Environment:

```txt
MEMORYD_PATH=:memory:
MEMORYD_DRIVER=memory
MEMORYD_MCP_AUDIT=true
MEMORYD_MCP_ACTOR=mcp-client
MEMORYD_MCP_SOURCE=memoryd-mcp
MEMORYD_MCP_AUDIT_STREAM=__memoryd:mcp:audit
```

### `memoryd-mcp-http`

Runs the MCP server over Streamable HTTP.

```bash
node packages/memoryd-mcp/dist/http-cli.js \
  --path ./memoryd.db \
  --driver native \
  --host 127.0.0.1 \
  --port 8757 \
  --auth-token change-me
```

Options:

```txt
--path <path>             memoryd database path
--driver <driver>        memory or native
--host <host>            bind host
--port <port>            bind port
--auth-token <token>     bearer token
--allow-unauthenticated  allow tokenless non-loopback binding
-h, --help               show help
```

Endpoints:

```txt
GET  /healthz
POST /mcp
GET  /cluster/status
GET  /cluster/peers
```

Cluster environment:

```txt
MEMORYD_CLUSTER_MODE=single|leader|follower
MEMORYD_CLUSTER_LEADER_URL=http://memoryd-leader:8757
MEMORYD_CLUSTER_PEERS=http://memoryd-0:8757,http://memoryd-1:8757
MEMORYD_CLUSTER_DISCOVERY=none|static|kubernetes
MEMORYD_ADVERTISE_URL=http://pod-ip:8757
MEMORYD_CLUSTER_FORWARD_AUTH_TOKEN=<leader-token>
```

## CLI Goals

The admin CLI should make `memoryd` easy to inspect and operate from a
terminal. It should work against:

- local in-memory stores for quick tests
- native SQLite stores when `@sayanmohsin/memoryd-native` is built
- remote sidecars through `MEMORYD_URL`
- Docker or Kubernetes sidecar deployments

The first version prefers predictable JSON output over visual polish. Pretty
tables can come later.

## Package

The dedicated workspace package is:

```txt
packages/memoryd-cli
```

Package shape:

```txt
packages/memoryd-cli/
  src/
    index.ts
    commands/
      status.ts
      collections.ts
      objects.ts
      events.ts
      queues.ts
      doctor.ts
    output.ts
    open-db.ts
  test/
    cli.test.mjs
  package.json
  README.md
  tsconfig.json
```

Binary:

```json
{
  "bin": {
    "memoryd": "./dist/index.js"
  }
}
```

The CLI uses the public `@sayanmohsin/memoryd` SDK. Do not import store
internals directly.

## Connection Rules

Open the store with the same rules as app code:

```txt
MEMORYD_URL set:
  use remote SDK driver over Streamable HTTP MCP

--url set:
  use remote SDK driver over Streamable HTTP MCP

--driver native --path ./memoryd.db:
  use local native Rust SQLite driver

no URL and no native driver:
  use in-memory proof store
```

Suggested shared options:

```txt
--url <url>          remote memoryd URL. Defaults to MEMORYD_URL
--auth-token <tok>  remote bearer token. Defaults to MEMORYD_AUTH_TOKEN
--path <path>       local database path. Defaults to MEMORYD_PATH or :memory:
--driver <driver>   memory, native, or remote
--json              emit JSON
--pretty            human-readable output
--limit <n>         result limit for list commands
```

For Phase CLI-A, default to JSON output unless `--pretty` is passed.

## Phase CLI-A - Inspect And Admin

Status: completed.

Deliverables:

- [x] create `packages/memoryd-cli`
- [x] add `memoryd --help`
- [x] add remote and local connection handling
- [x] add JSON output helper
- [x] add tests for command parsing and command output
- [x] update root scripts and docs

Commands:

```txt
memoryd status
memoryd tools
memoryd objects get <collection> <id>
memoryd objects put <collection> <id> --text <text>
memoryd objects delete <collection> <id>
memoryd search <query>
memoryd events list [stream]
memoryd queues list <queue>
memoryd queues dead <queue>
memoryd queues claim <queue>
memoryd queues ack <queue> <jobId>
memoryd queues nack <queue> <jobId> --error <message>
```

Notes:

- `status` should call remote `/healthz` and `/cluster/status` when possible.
- `tools` should list MCP tools when connected remotely.
- Local `status` can report `mode=local`, driver, and path.
- Do not add SQL inspection as the public interface.

Required checks:

```bash
pnpm check
pnpm build
pnpm test:local
pnpm test:mcp
```

## Phase CLI-B - Operator Polish

Target duration: 1 to 2 focused days.

Deliverables:

- table output
- `memoryd doctor`
- queue stats
- object and event list summaries
- benchmark wrapper commands
- better error messages for auth, connection refused, and missing native binding

Commands:

```txt
memoryd doctor
memoryd queues stats <queue>
memoryd collections list
memoryd objects list <collection>
memoryd events streams
memoryd bench rust --smoke
memoryd bench rust --count <n>
```

`doctor` should check:

- Node version
- package build output exists
- native package availability when `--driver native` is selected
- remote sidecar reachability when `MEMORYD_URL` is set
- auth token presence for non-local HTTP URLs

## Phase CLI-C - Data Movement

Target duration: 1 to 2 focused days after persistence behavior is stable.

Deliverables:

- export/import JSONL
- snapshots for local development
- redaction hooks for agent memory exports

Commands:

```txt
memoryd export --collection <name> --out objects.jsonl
memoryd export --events --out events.jsonl
memoryd import --collection <name> --in objects.jsonl
memoryd snapshot create --out snapshot.memoryd.json
memoryd snapshot restore --in snapshot.memoryd.json
```

Do not add this until collection listing and pagination semantics are designed.

## Handoff Notes

Best next step is Phase CLI-B.

Keep the implementation small:

- use the SDK as the only app-facing API
- use MCP only when the CLI is connected to a remote sidecar
- keep output stable enough for scripts
- add tests before adding visual polish
- update this document whenever CLI commands change
