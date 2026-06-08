# CLI Reference

The `thingd` command-line tool for inspecting and operating thingd stores.

## Installation

```bash
npm install -g thingd-cli
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
thingd install              zero-config setup for Cursor / Claude Desktop
thingd doctor               diagnose installation and connectivity
thingd search <query>       full-text search across collections
```

### Objects

```txt
thingd objects get <collection> <id>
thingd objects put <collection> <id> --text <text>
thingd objects put <collection> <id> --data '{"field":"value"}'
thingd objects delete <collection> <id>
thingd objects list <collection>
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

### Export / Import

```txt
thingd export --collection <name> --out objects.jsonl [--redact [keys]]
thingd export --events [--stream <name>] --out events.jsonl [--redact [keys]]
thingd import --collection <name> --in objects.jsonl
```

### Snapshots

```txt
thingd snapshot create --out snapshot.thingd.json
thingd snapshot restore --in snapshot.thingd.json
```

### MCP Server

```txt
thingd mcp [--path <path>] [--driver <driver>]
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

## Common Options

```txt
--url <url>          remote thingd URL. Defaults to THINGD_URL
--auth-token <tok>   remote bearer token. Defaults to THINGD_AUTH_TOKEN
--path <path>        local database path. Defaults to THINGD_PATH or ~/.thingd/data.db
--driver <driver>    memory, native, or remote
--pretty             pretty-print JSON output
--limit <n>          result limit for search and list commands
```

## Connection Rules

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

## Environment

```txt
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=change-me
THINGD_PATH=/data/thingd.db
THINGD_DRIVER=native|memory|remote
```

Full env var reference: [runtime-env.md](./runtime-env.md)
