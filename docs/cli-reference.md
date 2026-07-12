# CLI Reference

The `thingd` command-line tool for inspecting and operating thingd stores.

## Installation

```bash
npm install -g @thingd/cli
```

Or run with `npx`:

```bash
npx thingd <command>
```

## Commands

### General

```txt
thingd status              show runtime status (local or remote)
thingd tools                list MCP tools when connected remotely
thingd install [--raw] [--claude] [--cursor] [--antigravity]    zero-config local setup for Cursor / Claude Desktop / Antigravity IDE
thingd doctor               diagnose installation and connectivity
thingd search <query>       full-text search across collections
```

### Objects

```txt
thingd objects get <collection> <id>
thingd objects put <collection> <id> --text <text>
thingd objects put <collection> <id> --data '{"field":"value"}'
thingd objects put-batch <collection> --file <path>
thingd objects delete <collection> <id>
thingd objects delete-batch <collection> <id1> [id2] ...
thingd objects list <collection> [--limit <n>] [--offset <n>] [--sort-by <field>] [--sort-dir <asc|desc>] [--filter <json>]
```

### Events

```txt
thingd events append <stream> <type> [--text <text>] [--data '{"field":"value"}']
thingd events list [stream]
thingd events streams
```

### Queues

```txt
thingd queues push <queue> --payload '{"key":"value"}'
thingd queues claim <queue>
thingd queues ack <queue> <jobId>
thingd queues nack <queue> <jobId>
thingd queues list <queue>
thingd queues dead <queue>
thingd queues stats <queue>
```

### Links

```txt
thingd links create <fromRef> <linkType> <toRef> [--weight <n>] [--metadata <json>]
thingd links get <id>
thingd links delete <id>
thingd links neighbors <reference> [--direction Outgoing|Incoming|Both] [--type <linkType>] [--limit <n>]
thingd links count
```

### Export / Import

```txt
thingd export --collection <name> --out objects.jsonl [--redact [keys]]
thingd export --events [--stream <name>] --out events.jsonl [--redact [keys]]
thingd import --collection <name> --in objects.jsonl
thingd import <connection-string> --collection <name> [--tables <names>|--query <sql>] [--sidecar <url>] [--dry-run] [--list-tables] [--batch-size <n>]
```

### Snapshots

```txt
thingd snapshot create --out snapshot.thingd.json
thingd snapshot restore --in snapshot.thingd.json
```

### Backup

```txt
thingd backup --out backup.db
thingd backup --in backup.db
```

Creates a consistent snapshot of the SQLite database using `VACUUM INTO`.
The backup file is a standard SQLite database file. Use `--in` to restore from a backup.

### Database Maintenance

```txt
thingd db checkpoint
thingd db integrity
```

- `checkpoint` — Runs `PRAGMA wal_checkpoint(TRUNCATE)` to flush the WAL
- `integrity` — Checks database accessibility and reports status

### MCP Server

```txt
thingd mcp [--path <path>] [--driver <driver>]
thingd mcp connect
thingd mcp-http [--path <path>] [--driver <driver>] [--host <host>] [--port <port>] [--auth-token <token>] [--allow-unauthenticated]
```

See [MCP Server](./mcp-server.md) for details.

### Benchmark

```txt
thingd bench rust --smoke
thingd bench rust --count <n>
```

### Dashboard

```txt
thingd dashboard
```

Opens a live browser dashboard at `http://localhost:8758`.

### Metrics

```txt
thingd metrics
```

Shows Prometheus-format metrics (objects_total, events_total, etc.).

## Common Options

```txt
--url <url>          remote thingd URL. Defaults to THINGD_URL
--auth-token <tok>   remote bearer token. Defaults to THINGD_AUTH_TOKEN
--path <path>        local database path. Defaults to THINGD_PATH or ~/.thingd/data.db
--driver <driver>    memory, native, or cloud
--pretty             pretty-print JSON output
--limit <n>          result limit for search and list commands
```

## Connection Rules

```txt
THINGD_URL set:
  use remote SDK driver over HTTP REST

--url set:
  use remote SDK driver over HTTP REST

--driver native --path ./thingd.db:
  use local native Rust SQLite driver

no URL and no native driver:
  use in-memory store
```
## Cloud

`thingd cloud login [--code <code> --token <token>]`
  Authenticate with thingd Cloud (opens browser for device-code flow).

`thingd cloud api-key create <project> <name>`
  Create a persistent API key for MCP or SDK access. Returns the key once.
  The key survives cloud deployments (unlike ephemeral test tokens).

`thingd cloud project list`
  List projects in your thingd Cloud account.

`thingd cloud instance list <project>`
  List instances and their MCP URLs for a project.

## Environment

```txt
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=change-me
THINGD_PATH=/data/thingd.db
THINGD_DRIVER=native|memory|cloud
```

Full env var reference: [runtime-env.md](./runtime-env.md)

Full API reference: [api-spec/](./api-spec/)
