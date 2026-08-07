# Operations

thingd's current native persistent backend stores a database as a directory.
The procedures below distinguish opaque filesystem backups from logical exports;
deprecated SQLite compatibility commands are retained only for older installs.

Backup, recovery, database health, and maintenance procedures for thingd.

## Encrypted persistent storage

Set `THINGD_ENCRYPTION_KEY` to a 64-character hexadecimal key before opening a
native persistent database. Missing or incorrect keys fail safely. Changing
the variable does not rotate an existing database; use the explicit offline
re-encryption API to migrate or rotate into a new destination.

Filesystem backups remain encrypted and require the same key to restore. JSON
snapshots and logical exports contain decrypted data and must be protected as
plaintext artifacts. Encrypted search does not persist a Tantivy directory;
search is rebuilt from records at startup and uses process memory. Stop the
engine, or use the documented durability checkpoint, before copying a live
database directory. Changing `THINGD_ENCRYPTION_KEY` does not rotate a key.
Use the explicit offline re-encryption workflow instead.

### Offline migration and key rotation

```bash
thingd db reencrypt --source ./old-db --destination ./new-db
```

The source key is read from `THINGD_ENCRYPTION_SOURCE_KEY` or, for the common
case, `THINGD_ENCRYPTION_KEY`. The destination key is read from
`THINGD_ENCRYPTION_DESTINATION_KEY`. Source and destination must be different,
and an existing destination is never overwritten implicitly. Converting an
encrypted source to plaintext requires `--allow-plaintext-output`. The source
remains unchanged if the copy fails.

## Backup

### Creating a Backup

```bash
thingd backup --out /path/to/backup.db
```

The deprecated SQLite backup is created using `VACUUM INTO`. It does not apply
to current persistent database directories.

**Options:**
- `--out <path>` — Destination path for a legacy SQLite backup
- `--path <path>` — Source database path (overrides `THINGD_PATH`)

**Output:**
```
Backup created: /path/to/backup.db (1.25 MB)
```

### Restoring from Backup

For a current native persistent database, restore the directory while the
engine is stopped:

```bash
# Stop thingd-server
cp -R /path/to/backup-directory /path/to/thingd.db
# Start thingd-server
```

Or use the CLI with a file copy:

```bash
thingd db restore --in /path/to/backup-directory
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

Checks that the configured persistent directory can be opened and reports
storage errors without replacing it with memory storage. For an encrypted
database, the correct key must be present.

**Output:**
```json
{ "ok": true, "message": "Database is accessible" }
```

### Durability checkpoint

```bash
thingd db checkpoint
```

Flushes pending native persistent writes through the engine's durability
boundary before an operator copies the database directory. It does not rotate
an encryption key or decrypt a backup. In-memory databases have no filesystem
durability boundary.

## Schema and format migrations

The current native backend does not expose manual SQLite schema migrations.
Persistent format changes are versioned by the engine and encrypted databases
also validate their storage manifest and envelope version during open. Key
rotation is never automatic; use `db reencrypt`.

Migrations are applied automatically when a database is opened with an older schema version. Before each migration, an automatic backup is created at `{database_path}.pre-v{version}`.

### Legacy SQLite migration history

> Note: These SQLite schema versions apply to the deprecated SQLite backend.
> The current persistent backend has no manual schema management — schema is defined
> by the Rust struct layout and evolved through code changes.

| Version | Name | Changes |
|---------|------|---------|
| 1 | `initial_objects_events_queues` | Creates `objects`, `events`, `queue_jobs` tables |
| 2 | `fts5_search_index` | Adds FTS5 full-text search virtual table (replaced by Tantivy) |
| 3 | `queue_jobs_last_error` | Adds `last_error` column to `queue_jobs` |
| 4 | `graph_links` | Creates `links` table with graph relationship support |

### Legacy SQLite safety

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
- Database accessibility and native storage configuration

## Health Check Endpoints

- `GET /healthz` — Returns `{ "data": { "status": "ok" } }`
- `GET /v1/health` — Same as `/healthz`, REST API compliant

These endpoints are always unauthenticated and can be used by load balancers and orchestration systems.
