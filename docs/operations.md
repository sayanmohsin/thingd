# Operations

Backup, recovery, database health, and maintenance procedures for thingd.

## Backup

### Creating a Backup

```bash
thingd backup --out /path/to/backup.db
```

The backup is created using SQLite's `VACUUM INTO` command, which produces a fully consistent snapshot of the database at the point in time the command runs. The backup file is a standard SQLite database that can be opened with any SQLite client.

**Options:**
- `--out <path>` — Destination path for the backup file
- `--path <path>` — Source database path (overrides `THINGD_PATH`)

**Output:**
```
Backup created: /path/to/backup.db (1.25 MB)
```

### Restoring from Backup

There is no dedicated restore command. To restore:

```bash
# Stop thingd-server
cp /path/to/backup.db /path/to/thingd.db
# Start thingd-server
```

Or use the CLI with a file copy:

```bash
thingd db restore --in /path/to/backup.db
```

## Snapshots

Snapshots export data as JSON for portability between storage drivers.

### Creating a Snapshot

```bash
thingd snapshot create --out /path/to/snapshot.json
```

Captures all collections, events, and queues into a single JSON file with version `1.0.0`.

### Restoring a Snapshot

```bash
thingd snapshot restore --in /path/to/snapshot.json
```

> **Warning:** The restore is not atomic. If it fails mid-way, data may be partially restored.
> Create a backup first: `thingd backup --out pre-restore.db`

## Export / Import

### Export

```bash
# Export a collection as JSONL
thingd export --collection my-collection --out data.jsonl

# Export events
thingd export --events --stream my-stream --out events.jsonl

# Export with redaction (scrubs passwords, tokens, keys)
thingd export --collection users --out users.jsonl --redact

# Custom redaction keys
thingd export --collection users --out users.jsonl --redact password,ssn,credit_card
```

### Import

```bash
# Import JSONL
thingd import --collection my-collection --in data.jsonl

# Import CSV (first line = headers, subsequent lines = rows)
thingd import --collection my-collection --in data.csv
```

### Redaction Rules

When `--redact` is used during export, the following are automatically redacted:
- Values under keys containing: `password`, `secret`, `token`, `key`, `auth`, `credential`, `email`, `phone`, `session`, `cookie`, `signature`, `private`, `cert`, `api`
- Email addresses (pattern-based)
- API keys matching `sk-...` pattern
- Bearer tokens

## Database Health

### Integrity Check

```bash
thingd db integrity
```

Runs `PRAGMA quick_check` against the database and reports whether it passes. The check runs automatically on startup.

**Output:**
```json
{ "ok": true, "message": "Database is accessible" }
```

### WAL Checkpoint

```bash
thingd db checkpoint
```

Triggers `PRAGMA wal_checkpoint(TRUNCATE)` to flush the Write-Ahead Log into the main database file. This reduces WAL file size and improves read performance.

**Output:**
```json
{ "framesBefore": 42, "framesAfter": 0 }
```

- WAL checkpoint also runs automatically when the database connection is closed
- In-memory databases do not support WAL mode

## Schema Migrations

thingd uses a forward-only, versioned schema migration system. Current schema version: `4`.

Migrations are applied automatically when a database is opened with an older schema version. Before each migration, an automatic backup is created at `{database_path}.pre-v{version}`.

### Migration History

| Version | Name | Changes |
|---------|------|---------|
| 1 | `initial_objects_events_queues` | Creates `objects`, `events`, `queue_jobs` tables |
| 2 | `fts5_search_index` | Adds FTS5 full-text search virtual table |
| 3 | `queue_jobs_last_error` | Adds `last_error` column to `queue_jobs` |
| 4 | `graph_links` | Creates `links` table with graph relationship support |

### Safety

- All migrations run inside transactions
- A backup is automatically created before any migration (file-based databases only)
- `PRAGMA integrity_check` runs after all migrations complete
- If a migration fails, the backup at `{path}.pre-v{version}` can be used for recovery

## Doctor Command

```bash
thingd doctor
```

The doctor command checks:
- Node.js version (requires 20+)
- Native addon availability
- Auth token configuration (for cloud driver)
- Connectivity to remote endpoints (for cloud driver)
- Database integrity (via `PRAGMA quick_check`)

## Health Check Endpoints

- `GET /healthz` — Returns `{ "data": { "status": "ok" } }`
- `GET /v1/health` — Same as `/healthz`, REST API compliant

These endpoints are always unauthenticated and can be used by load balancers and orchestration systems.
