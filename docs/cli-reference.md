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
thingd collections list     list collections
thingd streams list          list event streams
thingd completions [shell]   print bash, zsh, or fish completions
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
thingd queues list-all
```

### Links

```txt
thingd links create <fromRef> <linkType> <toRef> [--weight <n>] [--metadata <json>]
thingd links get <id>
thingd links delete <id>
thingd links neighbors <reference> [--direction Outgoing|Incoming|Both] [--type <linkType>] [--limit <n>]
thingd links count
```

### Schema files

```txt
thingd schema [--collection <name>]       inspect inferred runtime schema
thingd schema check [file]                parse and validate schema.thingd
```

`schema check` uses the Rust parser and returns the canonical schema plus a
stable SHA-256 hash. The file defaults to `schema.thingd` in the current
directory. Schema files are optional; this command does not change runtime
object validation or apply migrations.

### Migrations

```txt
thingd migrate create <name> [--schema <file>]
thingd migrate plan [file]
thingd migrate apply [file]
thingd migrate status
```

`migrate create` copies a validated schema into
`thingd/migrations/NNNN_<name>.thingd`. `migrate plan` compares the explicit
schema with inferred runtime collections and fields, marks destructive changes,
and performs no writes. `migrate apply` persists the canonical schema hash and
migration record in the database and creates indexes for `@index` and `@unique`
fields. It does not delete or rewrite existing objects. Rollback and destructive
schema changes remain disabled.

### Export / Import

```txt
thingd export --collection <name> --out objects.jsonl [--redact [keys]]
thingd export --events [--stream <name>] --out events.jsonl [--redact [keys]]
thingd import --collection <name> --in objects.jsonl
thingd import <connection-string> --collection <name> [--tables <names>|--query <sql>] [--sidecar <url>] [--dry-run] [--list-tables] [--batch-size <n>]
```

### Snapshots

```txt
thingd snapshot create --out snapshot.thingd.jsonl
thingd snapshot restore --in snapshot.thingd.jsonl
thingd snapshot create --url http://localhost:8757 --out snapshot.thingd.jsonl
thingd snapshot restore --url http://localhost:8757 --in snapshot.thingd.jsonl
```

### Thingd-to-Thingd synchronization

Configure one source/replica relationship between any two Thingd HTTP endpoints.
The configured source pushes changes; the configured replica pulls and applies them.

```txt
thingd sync configure --local-url <url> --remote-url <url> --role source|replica [--provider self-hosted|thingd.cloud] [--project-id <id>] [--instance-slug <slug>] [--source-id <id>] [--allow-cloud-target --confirm-target] [--local-token <token>] [--remote-token <token>]
thingd sync status
thingd sync push
thingd sync pull
thingd sync bootstrap [--replace]
thingd sync pause
thingd sync resume
thingd sync reset
```

The sync protocol is provider-neutral. `thingd.cloud` is the protected/default
provider preset, not an engine dependency. A sync role is mandatory: Thingd never
guesses whether the local or remote endpoint is authoritative. Cloud targets also
require an explicit allow flag and confirmation, plus an instance slug so the
control plane cannot fall back to another accessible instance.

Natural-language queries are exposed as `thingd nlq`. There is no separate TLQ
implementation in the current release.

### Backup

```txt
thingd backup --out backup.db
thingd backup --in backup.db
```

The file backup form is for the deprecated SQLite compatibility backend. For
current native runtimes, stop or checkpoint the engine and back up the whole
database directory. Encrypted directory backups remain opaque and require the
same key to restore. JSON snapshots and logical exports are decrypted data.

### Database Maintenance

```txt
thingd db checkpoint
thingd db compact [--path /path/to/thingd.db]
thingd db repack --path /path/to/source --destination /path/to/destination
thingd db integrity
thingd db backup --out /path/to/backup.tar [--path /path/to/thingd.db]
thingd db restore --in /path/to/backup.tar --destination /path/to/thingd.db [--replace]
thingd db reencrypt --source <path> --destination <path> [--allow-plaintext-output]
```

- `checkpoint` — Flushes pending native persistent writes before a directory backup
- `compact` — Flushes and compacts the primary native keyspaces; stop application writers first
- `repack` — Exports a bounded logical snapshot into a new validated native store; the source is retained and the destination must not exist
- `integrity` — Checks database accessibility and reports status
- `backup` — Checkpoints and closes a local native database, then creates a tar archive of the complete directory. The source must not be open by another process.
- `restore` — Validates a Thingd tar archive and atomically restores it into a new destination. Existing destinations require `--replace`.
- `reencrypt` — Copies logical records into a new destination for migration or
  key rotation without modifying the source

The standalone server exposes equivalent offline operations as
`thingd-server --compact <path>` and
`thingd-server --repack <source> --destination <path>`. Both forms require
exclusive access. Compaction reduces physical journal pressure; repacking is a
separate logical migration for stores whose journal is already too large for a
safe in-place recovery.

The default native format is embedded RocksDB. The experimental Rust-native
ThingDB backend can be selected with `THINGD_STORAGE_BACKEND=thingdb`; it uses
a separate format and is not opened by changing the variable on an existing
RocksDB directory. Existing legacy directories must first be migrated with the
repository-built `thingd-migrate` utility; the published `thingd` CLI does not
silently convert native stores. See [Storage backends](./storage-backends.md).

`THINGD_ENCRYPTION_KEY` supplies the normal native database key.
`THINGD_ENCRYPTION_SOURCE_KEY` and `THINGD_ENCRYPTION_DESTINATION_KEY` supply
the two keys for `db reencrypt`. Source and destination must differ, existing
destinations are not overwritten, and encrypted-to-plaintext output requires
`--allow-plaintext-output`.

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
  use local native Rust persistent driver

no URL and no native driver:
  use in-memory store
```
## Cloud

`thingd cloud login [--code <code> --token <token>]`
  Authenticate with thingd Cloud (opens browser for device-code flow).
  Automatically creates a persistent CLI token (md_user_*) on success.

`thingd cloud logout`
  Revokes the current CLI token and clears saved credentials.

`thingd cloud status`
  Show logged-in user, active CLI token info, and current instance.

`thingd cloud token list`
  List all CLI tokens with their prefix, last used time, and project access.

`thingd cloud token create <name>`
  Create a new CLI token (md_user_*) for use with the CLI or TUI.
  Shows the full token once — copy it immediately.

`thingd cloud token revoke <id>`
  Revoke a CLI token — it will no longer authenticate.

`thingd cloud token restrict <id> <project-slug>`
  Limit a CLI token to a specific project.

`thingd cloud token unrestricted <id>`
  Allow a CLI token to access all projects.

`thingd cloud token cleanup`
  Remove expired login sessions from your account.

`thingd cloud api-key create <project> <name>`
  [DEPRECATED] Use `thingd cloud token create` for CLI tokens.
  Project API keys are managed in the web dashboard.

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
