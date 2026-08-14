# Operations

thingd's current native persistent backend stores a database as a directory.
The procedures below distinguish opaque filesystem backups from logical exports;
deprecated SQLite compatibility commands are retained only for older installs.

Each current native directory contains a Thingd-owned `.thingd-storage.json`
manifest. It records the storage contract and required keyspaces. Directories
created by older releases are structurally checked on first open and receive a
manifest after a successful open; unsupported manifests fail closed. Always
copy the complete directory, including `lock`, and stop the writer or run a
durability checkpoint before copying.

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

For the current native backend, a filesystem backup is an opaque database
directory. It remains encrypted when the source is encrypted and must be made
only after the engine is stopped or after `thingd db checkpoint` completes.
JSON snapshots are logical, decrypted exports and are not equivalent to an
opaque filesystem backup.

For a local native archive, use:

```bash
thingd db backup --path /data/thingd.db --out /backups/thingd.tar
thingd db restore --in /backups/thingd.tar --destination /data/thingd-restored.db
```

The backup command requires exclusive access while it checkpoints and closes
the store. Restore validates archive paths and native layout before atomic
promotion, preserves encrypted bytes, and refuses to overwrite an existing
destination unless `--replace` is supplied. Embedded numbered-keyspace stores
and standalone named-keyspace stores are not interchangeable; use logical
JSONL snapshots for that migration boundary.

**Options:**
- `--out <path>` — Destination path for the filesystem backup
- `--path <path>` — Source database path (overrides `THINGD_PATH`)

**Output:**
```
Backup created: /path/to/backup.db (1.25 MB)
```

### Restoring from Backup

For a current native persistent database, restore the validated archive while
the engine is stopped:

```bash
thingd db restore --in /backups/thingd.tar --destination /data/thingd.db
```

The destination must not already exist unless `--replace` is supplied.

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
thingd db compact --path /data/thingd.db
```

Flushes pending native persistent writes through the engine's durability
boundary before an operator copies the database directory. It does not rotate
an encryption key or decrypt a backup. In-memory databases have no filesystem
durability boundary.

### Standalone compatibility check

The standalone server can validate an existing directory without binding an
HTTP port:

```bash
thingd-server --check /data/thingd.db
```

The check verifies the native manifest, lock file, keyspace directory, and
whether an existing Tantivy index has the current schema. An incompatible
Tantivy index is rebuildable derived state; an incompatible primary storage
manifest or missing lock file is an error and must be repaired or restored from
backup.

### Low-memory search mode

For embedded deployments that do not need full-text search, set:

```bash
THINGD_SEARCH_MODE=disabled
```

This avoids opening or rebuilding the Tantivy directory. Search uses the
engine's slower fallback scan, so callers must use small limits and filters.
For a persistent existing index without automatic repair, use
`THINGD_SEARCH_MODE=persistent-no-rebuild`; a missing or incompatible index is
then treated as unavailable rather than rebuilt during startup.
Standalone HTTP mode is preferred on hosts with less than 2 GB RAM because it
keeps the database process separate from application and catalog memory.

## Diagnostics and retention

Use the additive diagnostics endpoint to inspect bounded record counts without
loading records into the response:

```bash
curl http://127.0.0.1:8757/v1/diagnostics
```

Retention is never automatic. Preview eligible old events and completed or
dead queue jobs first:

```bash
curl -X POST http://127.0.0.1:8757/admin/retention \
  -H 'content-type: application/json' \
  -d '{"beforeUnixMs":1700000000000,"dryRun":true}'
```

Deletion requires `dryRun:false` and an explicit `confirm:true`. Protected
Thingd streams are skipped. Replication records additionally require
`includeReplication:true` and are pruned only through the minimum active
replica checkpoint; with no checkpoint they are reported as skipped.
`compact:true` requests a separate major storage compaction after successful
deletion; compaction is not run during startup.

Keep object, event, and queue payloads small. Store large blobs outside Thingd,
avoid duplicating catalog/provider data, keep only queryable fields, and remove
indexes that are not used by filters. Apply bounded `limit` values to list and
search calls. For deployments below 2 GB RAM, standalone HTTP mode with
disabled or no-rebuild search is preferred over embedding Thingd beside a
catalog enrichment process.

## Schema and format migrations

The current native backend does not expose manual SQL schema migrations.
Persistent format changes are versioned by the engine and encrypted databases
also validate their storage manifest and envelope version during open. Key
rotation is never automatic; use `db reencrypt`.

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
