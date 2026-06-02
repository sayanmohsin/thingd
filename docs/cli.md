# CLI Roadmap

This document describes the current command-line surface and the remaining
operator/developer CLI phases for `thingd`.

The project now has runtime CLIs through the unified `thingd` command and a
first-pass admin CLI through `thingd-cli`.

## Current CLIs

### `thingd`

Inspects and mutates local or remote `thingd` stores through the public SDK.

```bash
node packages/thingd-cli/dist/index.js status
node packages/thingd-cli/dist/index.js objects put decisions cli --text "Built the CLI."
node packages/thingd-cli/dist/index.js queues push embed --payload '{"object":"docs/readme"}'
```

Commands:

```txt
thingd status
thingd tools
thingd install
thingd search <query>
thingd objects get <collection> <id>
thingd objects put <collection> <id> --text <text>
thingd objects put <collection> <id> --data '{"field":"value"}'
thingd objects delete <collection> <id>
thingd events append <stream> <type> [--text <text>] [--data '{"field":"value"}']
thingd events list [stream]
thingd queues push <queue> --payload '{"key":"value"}'
thingd queues claim <queue>
thingd queues ack <queue> <jobId>
thingd queues nack <queue> <jobId>
thingd queues list <queue>
thingd queues dead <queue>
```

Common options:

```txt
--url <url>          remote thingd URL. Defaults to THINGD_URL
--auth-token <tok>  remote bearer token. Defaults to THINGD_AUTH_TOKEN
--path <path>       local database path. Defaults to THINGD_PATH or ~/.thingd/data.db
--driver <driver>   memory, native, or remote
--pretty            pretty-print JSON output
--limit <n>         result limit for search and list commands
```

Remote sidecar usage:

```bash
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=change-me
node packages/thingd-cli/dist/index.js status
node packages/thingd-cli/dist/index.js tools
```

### `thingd install`

Runs a zero-config setup flow that:
1. Detects Node.js, CLI entry path, database path, and native driver availability.
2. Auto-configures Claude Desktop config on macOS.
3. Prints a copy-pasteable Cursor MCP JSON config block.
4. Auto-creates the default database directory `~/.thingd`.

```bash
thingd install
```

### `thingd mcp`

Runs the MCP server over stdio for local MCP clients.

```bash
node packages/thingd-cli/dist/index.js mcp --driver native
```

Options:

```txt
--path <path>      thingd database path. Defaults to THINGD_PATH or ~/.thingd/data.db
--driver <driver> memory or native. Defaults to THINGD_DRIVER or memory
-h, --help        show help
```

Environment:

```txt
THINGD_PATH=~/.thingd/data.db
THINGD_DRIVER=native
THINGD_MCP_AUDIT=true
THINGD_MCP_ACTOR=mcp-client
THINGD_MCP_SOURCE=thingd-mcp
THINGD_MCP_AUDIT_STREAM=__thingd:mcp:audit
```

### `thingd mcp-http`

Runs the MCP server over Streamable HTTP.

```bash
node packages/thingd-cli/dist/index.js mcp-http \
  --path ./thingd.db \
  --driver native \
  --host 127.0.0.1 \
  --port 8757 \
  --auth-token change-me
```

Options:

```txt
--path <path>             thingd database path
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
THINGD_CLUSTER_MODE=single|leader|follower
THINGD_CLUSTER_LEADER_URL=http://thingd-leader:8757
THINGD_CLUSTER_PEERS=http://thingd-0:8757,http://thingd-1:8757
THINGD_CLUSTER_DISCOVERY=none|static|kubernetes
THINGD_ADVERTISE_URL=http://pod-ip:8757
THINGD_CLUSTER_FORWARD_AUTH_TOKEN=<leader-token>
```

## CLI Goals

The admin CLI should make `thingd` easy to inspect and operate from a
terminal. It should work against:

- local in-memory stores for quick tests
- native SQLite stores when `thingd-native` is built
- remote sidecars through `THINGD_URL`
- Docker or Kubernetes sidecar deployments

The first version prefers predictable JSON output over visual polish. Pretty
tables can come later.

## Package

The dedicated workspace package is:

```txt
packages/thingd-cli
```

Package shape:

```txt
packages/thingd-cli/
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
    "thingd": "./dist/index.js"
  }
}
```

The CLI uses the public `thingd` SDK. Do not import store
internals directly.

## Connection Rules

Open the store with the same rules as app code:

```txt
THINGD_URL set:
  use remote SDK driver over Streamable HTTP MCP

--url set:
  use remote SDK driver over Streamable HTTP MCP

--driver native --path ./thingd.db:
  use local native Rust SQLite driver

no URL and no native driver:
  use in-memory proof store
```

Suggested shared options:

```txt
--url <url>          remote thingd URL. Defaults to THINGD_URL
--auth-token <tok>  remote bearer token. Defaults to THINGD_AUTH_TOKEN
--path <path>       local database path. Defaults to THINGD_PATH or :memory:
--driver <driver>   memory, native, or remote
--json              emit JSON
--pretty            human-readable output
--limit <n>         result limit for list commands
```

For Phase CLI-A, default to JSON output unless `--pretty` is passed.

## Phase CLI-A - Inspect And Admin

Status: completed.

Deliverables:

- [x] create `packages/thingd-cli`
- [x] add `thingd --help`
- [x] add remote and local connection handling
- [x] add JSON output helper
- [x] add tests for command parsing and command output
- [x] update root scripts and docs

Commands:

```txt
thingd status [--url <url>]
thingd tools --url <url>
thingd mcp [--path <path>] [--driver <driver>]
thingd mcp-http [--path <path>] [--driver <driver>] [--host <host>] [--port <port>] [--auth-token <tok>] [--allow-unauthenticated]
thingd objects get <collection> <id>
thingd objects put <collection> <id> --text <text>
thingd objects delete <collection> <id>
thingd search <query>
thingd events list [stream]
thingd queues list <queue>
thingd queues dead <queue>
thingd queues claim <queue>
thingd queues ack <queue> <jobId>
thingd queues nack <queue> <jobId> --error <message>
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
- `thingd doctor`
- queue stats
- object and event list summaries
- benchmark wrapper commands
- better error messages for auth, connection refused, and missing native binding

Commands:

```txt
thingd doctor
thingd queues stats <queue>
thingd collections list
thingd objects list <collection>
thingd events streams
thingd bench rust --smoke
thingd bench rust --count <n>
```

`doctor` should check:

- Node version
- package build output exists
- native package availability when `--driver native` is selected
- remote sidecar reachability when `THINGD_URL` is set
- auth token presence for non-local HTTP URLs

## Phase CLI-C - Data Movement

Status: completed.

Deliverables:

- [x] export/import JSONL
- [x] snapshots for local development
- [x] redaction hooks for agent memory exports

Commands:

```txt
thingd export --collection <name> --out objects.jsonl [--redact [keys]]
thingd export --events [--stream <name>] --out events.jsonl [--redact [keys]]
thingd import --collection <name> --in objects.jsonl
thingd snapshot create --out snapshot.thingd.json
thingd snapshot restore --in snapshot.thingd.json
```

### Redaction Support
The `--redact` flag on `export` supports custom and default redaction for sensitive fields. If specified without arguments, it redacts common secrets (like passwords, keys, and tokens). It can also accept a comma-separated list of keys to redact (e.g. `--redact secret_key,token`). The export command will recursively search object bodies and redact any matched keys, replacing their values with `"[REDACTED]"`. Any string values in the `text` fields are scanned for email addresses and API keys and replaced automatically.

## Handoff Notes

Best next step is Phase CLI-B (Phase 1 in [roadmap.md](./roadmap.md)).

When adding or changing commands, follow [doc-maintenance.md](./doc-maintenance.md).

Keep the implementation small:

- use the SDK as the only app-facing API
- use MCP only when the CLI is connected to a remote sidecar
- keep output stable enough for scripts
- add tests before adding visual polish
- update this document whenever CLI commands change
