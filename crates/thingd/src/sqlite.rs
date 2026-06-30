//! `SQLite`-backed storage adapter.
//!
//! This adapter implements durable object, event, and queue storage.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::model::{ListEventsOptions, ListObjectsOptions};
use crate::{
    EventLog, MemoryEvent, MemoryObject, ObjectKey, ObjectStore, QueueClaimOptions, QueueJob,
    QueueJobStatus, QueueNackOptions, QueueStore, ThingdError, ThingdResult, u64_to_i64,
    unix_timestamp_millis,
};

/// Current `SQLite` schema version.
pub const SQLITE_SCHEMA_VERSION: u32 = 5;

/// `SQLite`-backed memory store.
pub struct SqliteThingStore {
    connection: Connection,
    db_path: Option<String>,
    event_idempotency_keys: HashMap<(String, String), u64>,
}

impl SqliteThingStore {
    /// Open a `SQLite` database file and initialize the schema.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` cannot open the path or initialize schema.
    pub fn open(path: impl AsRef<Path>) -> ThingdResult<Self> {
        let path_str = path.as_ref().to_str().map(String::from);
        let connection = Connection::open(path).map_err(ThingdError::from)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(ThingdError::from)?;
        let store = Self {
            connection,
            db_path: path_str,
            event_idempotency_keys: HashMap::new(),
        };
        store.initialize()?;
        Ok(store)
    }

    /// Open an in-memory `SQLite` database and initialize the schema.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` cannot initialize schema.
    pub fn open_in_memory() -> ThingdResult<Self> {
        let connection = Connection::open_in_memory().map_err(ThingdError::from)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(ThingdError::from)?;
        let store = Self {
            connection,
            db_path: None,
            event_idempotency_keys: HashMap::new(),
        };
        store.initialize()?;
        Ok(store)
    }

    fn initialize(&self) -> ThingdResult<()> {
        let current_mode: String = self
            .connection
            .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
            .unwrap_or_else(|_| "delete".to_string());

        if current_mode.to_lowercase() != "wal" {
            self.connection
                .query_row("PRAGMA journal_mode = WAL;", [], |_| Ok(()))
                .map_err(|e| {
                    eprintln!("warning: failed to enable WAL journal mode: {e}");
                })
                .ok();
        }

        self.connection
            .execute_batch(
                r"
                PRAGMA synchronous = NORMAL;
                PRAGMA foreign_keys = ON;
                PRAGMA busy_timeout = 5000;
                ",
            )
            .map_err(ThingdError::from)?;

        self.connection
            .execute_batch(
                r"
                CREATE TABLE IF NOT EXISTS thingd_schema_migrations (
                    version INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                ",
            )
            .map_err(ThingdError::from)?;

        let current_version = self.schema_version()?;

        // Auto-backup before migration for file-based databases
        if current_version > 0 && current_version < SQLITE_SCHEMA_VERSION {
            #[allow(clippy::collapsible_if)]
            if let Some(ref path) = self.db_path {
                let backup_path = format!("{path}.pre-v{current_version}");
                let escaped = backup_path.replace('\'', "''");
                let _ = self
                    .connection
                    .execute_batch(&format!("VACUUM INTO '{escaped}'"));
            }
        }

        if current_version < 1 {
            self.apply_schema_v1()?;
        }

        let current_version = self.schema_version()?;

        if current_version < 2 {
            self.apply_schema_v2()?;
        }

        let current_version = self.schema_version()?;

        if current_version < 3 {
            self.apply_schema_v3()?;
        }

        let current_version = self.schema_version()?;

        if current_version < 4 {
            self.apply_schema_v4()?;
        }

        let current_version = self.schema_version()?;

        if current_version < 5 {
            self.apply_schema_v5()?;
        }

        if current_version > SQLITE_SCHEMA_VERSION {
            return Err(ThingdError::Storage(format!(
                "database schema version {current_version} is newer than supported version {SQLITE_SCHEMA_VERSION}"
            )));
        }

        // Run integrity check after all migrations
        let ok: String = self
            .connection
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(ThingdError::from)?;
        if ok != "ok" {
            return Err(ThingdError::Storage(format!(
                "database integrity check failed: {ok}"
            )));
        }

        Ok(())
    }

    /// Return the latest applied `SQLite` schema version.
    ///
    /// # Errors
    ///
    /// Returns an error when the migration metadata cannot be read.
    pub fn schema_version(&self) -> ThingdResult<u32> {
        let version = self
            .connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM thingd_schema_migrations",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(ThingdError::from)?;

        u32::try_from(version).map_err(|error| ThingdError::Storage(error.to_string()))
    }

    /// Run `PRAGMA wal_checkpoint(TRUNCATE)` to flush the WAL into the main database.
    ///
    /// Returns `(frames_before, frames_after)` where `frames_after` should be 0
    /// when the checkpoint fully succeeds.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` fails to run the checkpoint.
    pub fn wal_checkpoint(&self) -> ThingdResult<(i32, i32)> {
        self.connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                Ok((row.get::<_, i32>(0)?, row.get::<_, i32>(1)?))
            })
            .map_err(ThingdError::from)
    }

    /// Optimize the FTS5 search index to merge segments and reclaim space.
    ///
    /// Run periodically on long-lived databases to prevent search performance
    /// degradation from index fragmentation.
    ///
    /// # Errors
    ///
    /// Returns an error when the merge command fails.
    pub fn optimize_search_index(&self) -> ThingdResult<()> {
        self.connection
            .execute_batch("INSERT INTO search_index(search_index) VALUES('optimize')")
            .map_err(ThingdError::from)
    }

    /// Close the database connection after running a WAL checkpoint.
    ///
    /// # Errors
    ///
    /// Returns an error when the checkpoint fails.
    pub fn close(&self) -> ThingdResult<()> {
        self.wal_checkpoint()?;
        Ok(())
    }

    /// Create a consistent snapshot backup using `VACUUM INTO`.
    ///
    /// The backup file will contain all data at the current point in time.
    /// The file can be opened with any `SQLite` client.
    ///
    /// # Errors
    ///
    /// Returns an error when the destination path cannot be written or contains
    /// path traversal components (`..`).
    pub fn backup_to(&self, path: &str) -> ThingdResult<()> {
        // Reject path traversal attempts
        if Path::new(path)
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => Some(s.to_str().unwrap_or("")),
                std::path::Component::ParentDir => Some(".."),
                _ => None,
            })
            .any(|x| x == "..")
        {
            return Err(ThingdError::InvalidInput(
                "Backup path must not contain '..' (path traversal)".to_string(),
            ));
        }
        let escaped = path.replace('\'', "''");
        self.connection
            .execute_batch(&format!("VACUUM INTO '{escaped}'"))
            .map_err(ThingdError::from)
    }

    fn apply_schema_v1(&self) -> ThingdResult<()> {
        self.connection
            .execute_batch(
                r"
                BEGIN;

                CREATE TABLE IF NOT EXISTS objects (
                    collection TEXT NOT NULL,
                    id TEXT NOT NULL,
                    body TEXT NOT NULL,
                    version INTEGER NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    PRIMARY KEY (collection, id)
                );

                CREATE TABLE IF NOT EXISTS events (
                    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                    stream TEXT NOT NULL,
                    event_type TEXT NOT NULL,
                    body TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );

                CREATE INDEX IF NOT EXISTS idx_events_stream_sequence
                    ON events (stream, sequence);

                CREATE TABLE IF NOT EXISTS queue_jobs (
                    queue TEXT NOT NULL,
                    id TEXT NOT NULL,
                    body TEXT NOT NULL,
                    attempts INTEGER NOT NULL,
                    max_attempts INTEGER NOT NULL,
                    status TEXT NOT NULL,
                    available_at_ms INTEGER NOT NULL,
                    leased_at_ms INTEGER,
                    lease_expires_at_ms INTEGER,
                    completed_at_ms INTEGER,
                    dead_at_ms INTEGER,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    PRIMARY KEY (queue, id)
                );

                CREATE INDEX IF NOT EXISTS idx_queue_jobs_queue_status_created
                    ON queue_jobs (queue, status, created_at);

                CREATE INDEX IF NOT EXISTS idx_queue_jobs_status
                    ON queue_jobs (status);

                INSERT OR IGNORE INTO thingd_schema_migrations (version, name, applied_at)
                VALUES (1, 'initial_objects_events_queues', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

                COMMIT;
                ",
            )
            .map_err(ThingdError::from)?;

        Ok(())
    }

    fn apply_schema_v2(&self) -> ThingdResult<()> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(ThingdError::from)?;

        tx.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
                collection UNINDEXED,
                id UNINDEXED,
                kind UNINDEXED,
                text,
                tokenize='porter unicode61'
            );",
        )
        .map_err(ThingdError::from)?;

        // Reindex all existing objects and events inside the same transaction
        Self::reindex_all_into(&tx)?;

        tx.execute(
            "INSERT OR IGNORE INTO thingd_schema_migrations (version, name, applied_at)
             VALUES (2, 'fts5_search_index', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )
        .map_err(ThingdError::from)?;

        tx.commit().map_err(ThingdError::from)?;
        Ok(())
    }

    fn apply_schema_v3(&self) -> ThingdResult<()> {
        self.connection
            .execute(
                "ALTER TABLE queue_jobs ADD COLUMN last_error TEXT NOT NULL DEFAULT ''",
                [],
            )
            .map_err(ThingdError::from)?;
        self.connection
            .execute(
                "INSERT OR IGNORE INTO thingd_schema_migrations (version, name, applied_at)
                 VALUES (3, 'queue_jobs_last_error', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                [],
            )
            .map_err(ThingdError::from)?;
        Ok(())
    }

    fn apply_schema_v4(&self) -> ThingdResult<()> {
        self.connection
            .execute_batch(
                r"
                CREATE TABLE IF NOT EXISTS links (
                    id TEXT PRIMARY KEY,
                    from_ref TEXT NOT NULL,
                    type TEXT NOT NULL,
                    to_ref TEXT NOT NULL,
                    weight REAL,
                    metadata_json TEXT NOT NULL DEFAULT '{}',
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );

                CREATE INDEX IF NOT EXISTS idx_links_from_ref ON links (from_ref);
                CREATE INDEX IF NOT EXISTS idx_links_to_ref ON links (to_ref);
                CREATE INDEX IF NOT EXISTS idx_links_type ON links (type);
                ",
            )
            .map_err(ThingdError::from)?;
        self.connection
            .execute(
                "INSERT OR IGNORE INTO thingd_schema_migrations (version, name, applied_at)
                 VALUES (4, 'graph_links', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                [],
            )
            .map_err(ThingdError::from)?;
        Ok(())
    }

    fn apply_schema_v5(&self) -> ThingdResult<()> {
        self.connection
            .execute_batch(
                r"
                CREATE INDEX IF NOT EXISTS idx_objects_collection ON objects (collection);
                CREATE INDEX IF NOT EXISTS idx_objects_created_at ON objects (created_at);
                CREATE INDEX IF NOT EXISTS idx_events_stream ON events (stream);
                ",
            )
            .map_err(ThingdError::from)?;
        self.connection
            .execute(
                "INSERT OR IGNORE INTO thingd_schema_migrations (version, name, applied_at)
                 VALUES (5, 'performance_indexes', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                [],
            )
            .map_err(ThingdError::from)?;
        Ok(())
    }

    fn reindex_all_into(tx: &rusqlite::Transaction<'_>) -> ThingdResult<()> {
        let mut stmt_objects = tx
            .prepare("SELECT collection, id, body FROM objects")
            .map_err(ThingdError::from)?;
        let rows_objects = stmt_objects
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(ThingdError::from)?;

        let mut stmt_events = tx
            .prepare("SELECT stream, sequence, body FROM events")
            .map_err(ThingdError::from)?;
        let rows_events = stmt_events
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?.to_string(),
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(ThingdError::from)?;

        // Clear existing FTS index
        tx.execute("DELETE FROM search_index", [])
            .map_err(ThingdError::from)?;

        for row in rows_objects {
            let (collection, id, body) = row.map_err(ThingdError::from)?;
            let text = extract_text_from_json(&body);
            tx.execute(
                "INSERT INTO search_index (collection, id, kind, text) VALUES (?1, ?2, 'object', ?3)",
                params![collection, id, text],
            )
            .map_err(ThingdError::from)?;
        }

        for row in rows_events {
            let (stream, sequence, body) = row.map_err(ThingdError::from)?;
            let text = extract_text_from_json(&body);
            tx.execute(
                "INSERT INTO search_index (collection, id, kind, text) VALUES (?1, ?2, 'event', ?3)",
                params![stream, sequence, text],
            )
            .map_err(ThingdError::from)?;
        }

        Ok(())
    }
}

impl ObjectStore for SqliteThingStore {
    fn put_object(&mut self, mut object: MemoryObject) -> ThingdResult<MemoryObject> {
        let transaction = self.connection.transaction().map_err(ThingdError::from)?;

        // Use UPSERT with automatic version increment — no SELECT needed
        let row = transaction
            .query_row(
                r"
                INSERT INTO objects (collection, id, body, version, created_at, updated_at)
                VALUES (?1, ?2, ?3, 1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                ON CONFLICT(collection, id) DO UPDATE SET
                    body = excluded.body,
                    version = version + 1,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                RETURNING version, created_at, updated_at
                ",
                params![&object.key.collection, &object.key.id, &object.body],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(ThingdError::from)?;
        object.version = u64::try_from(row.0).map_err(|e| ThingdError::Storage(e.to_string()))?;
        object.created_at = row.1;
        object.updated_at = row.2;

        let text = extract_text_from_json(&object.body);
        transaction
            .execute(
                "DELETE FROM search_index WHERE collection = ?1 AND id = ?2 AND kind = 'object'",
                params![&object.key.collection, &object.key.id],
            )
            .map_err(ThingdError::from)?;
        transaction
            .execute(
                "INSERT INTO search_index (collection, id, kind, text) VALUES (?1, ?2, 'object', ?3)",
                params![&object.key.collection, &object.key.id, text],
            )
            .map_err(ThingdError::from)?;

        transaction.commit().map_err(ThingdError::from)?;

        Ok(object)
    }

    fn put_objects_batch(&mut self, objects: Vec<MemoryObject>) -> ThingdResult<Vec<MemoryObject>> {
        let transaction = self.connection.transaction().map_err(ThingdError::from)?;

        let mut fts_updates: Vec<(String, String, String)> = Vec::with_capacity(objects.len());
        let mut results = Vec::with_capacity(objects.len());
        for mut object in objects {
            // Use UPSERT with automatic version increment — no SELECT needed
            let row = transaction
                .query_row(
                    r"
                    INSERT INTO objects (collection, id, body, version, created_at, updated_at)
                    VALUES (?1, ?2, ?3, 1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                    ON CONFLICT(collection, id) DO UPDATE SET
                        body = excluded.body,
                        version = version + 1,
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    RETURNING version, created_at, updated_at
                    ",
                    params![&object.key.collection, &object.key.id, &object.body],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .map_err(ThingdError::from)?;
            object.version =
                u64::try_from(row.0).map_err(|e| ThingdError::Storage(e.to_string()))?;
            object.created_at = row.1;
            object.updated_at = row.2;

            let text = extract_text_from_json(&object.body);
            fts_updates.push((object.key.collection.clone(), object.key.id.clone(), text));

            results.push(object);
        }

        for (collection, id, text) in &fts_updates {
            transaction
                .execute(
                    "DELETE FROM search_index WHERE collection = ?1 AND id = ?2 AND kind = 'object'",
                    params![collection, id],
                )
                .map_err(ThingdError::from)?;
            transaction
                .execute(
                    "INSERT INTO search_index (collection, id, kind, text) VALUES (?1, ?2, 'object', ?3)",
                    params![collection, id, text],
                )
                .map_err(ThingdError::from)?;
        }

        transaction.commit().map_err(ThingdError::from)?;

        Ok(results)
    }

    fn put_object_with_options(
        &mut self,
        mut object: MemoryObject,
        options: crate::PutObjectOptions,
    ) -> ThingdResult<MemoryObject> {
        let transaction = self.connection.transaction().map_err(ThingdError::from)?;
        let current_version = transaction
            .query_row(
                "SELECT version FROM objects WHERE collection = ?1 AND id = ?2",
                params![&object.key.collection, &object.key.id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(ThingdError::from)?;

        // CAS check: if expected_version is Some, verify it matches
        if let Some(expected) = options.expected_version {
            match current_version {
                Some(actual) if u64::try_from(actual).unwrap_or(0) != expected => {
                    return Err(ThingdError::Conflict(format!(
                        "Version mismatch for {}/{}: expected {expected}, got {actual}",
                        object.key.collection, object.key.id,
                    )));
                },
                None => {
                    return Err(ThingdError::Conflict(format!(
                        "Version mismatch for {}/{}: expected {expected}, object does not exist",
                        object.key.collection, object.key.id,
                    )));
                },
                _ => {},
            }
        }

        let version = current_version.map_or(Ok::<u64, ThingdError>(1), |existing| {
            u64::try_from(existing)
                .map(|existing| existing + 1)
                .map_err(|error| ThingdError::Storage(error.to_string()))
        })?;

        object.version = version;
        let stored_version = i64::try_from(object.version)
            .map_err(|error| ThingdError::Storage(error.to_string()))?;

        let timestamps = transaction
            .query_row(
                r"
                INSERT INTO objects (collection, id, body, version, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                ON CONFLICT(collection, id) DO UPDATE SET
                    body = excluded.body,
                    version = excluded.version,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                RETURNING created_at, updated_at
                ",
                params![
                    &object.key.collection,
                    &object.key.id,
                    &object.body,
                    stored_version
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                    ))
                },
            )
            .map_err(ThingdError::from)?;
        object.created_at = timestamps.0;
        object.updated_at = timestamps.1;

        if options.index {
            let text = extract_text_from_json(&object.body);
            transaction
                .execute(
                    "DELETE FROM search_index WHERE collection = ?1 AND id = ?2 AND kind = 'object'",
                    params![&object.key.collection, &object.key.id],
                )
                .map_err(ThingdError::from)?;
            transaction
                .execute(
                    "INSERT INTO search_index (collection, id, kind, text) VALUES (?1, ?2, 'object', ?3)",
                    params![&object.key.collection, &object.key.id, text],
                )
                .map_err(ThingdError::from)?;
        }

        transaction.commit().map_err(ThingdError::from)?;

        Ok(object)
    }

    fn get_object(&self, collection: &str, id: &str) -> ThingdResult<Option<MemoryObject>> {
        self.connection
            .query_row(
                "SELECT collection, id, body, version, created_at, updated_at FROM objects WHERE collection = ?1 AND id = ?2",
                params![collection, id],
                |row| {
                    let version = row.get::<_, i64>(3)?;

                    Ok(MemoryObject {
                        key: ObjectKey::new(row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                        body: row.get(2)?,
                        version: u64::try_from(version).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                3,
                                rusqlite::types::Type::Integer,
                                Box::new(error),
                            )
                        })?,
                        created_at: row.get::<_, String>(4).unwrap_or_default(),
                        updated_at: row.get::<_, String>(5).unwrap_or_default(),
                    })
                },
            )
            .optional()
            .map_err(ThingdError::from)
    }

    fn list_objects(
        &self,
        collections: Option<&[String]>,
        options: &ListObjectsOptions,
    ) -> ThingdResult<Vec<MemoryObject>> {
        // Build a dynamic SQL query applying collection filter, JSON field filters,
        // LIMIT, and OFFSET entirely in SQLite — no post-processing in Rust.
        let mut conditions: Vec<String> = Vec::new();
        let mut bound_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        // Collection IN (...) clause.
        let has_collections = collections.is_some_and(|c| !c.is_empty());
        if has_collections {
            let cols = collections.unwrap_or_default();
            let placeholders = cols.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            conditions.push(format!("collection IN ({placeholders})"));
            for col in cols {
                bound_values.push(Box::new(col.clone()));
            }
        }

        // json_extract(...) = ? for each filter pair.
        for (key, value) in &options.filter {
            // Validate filter key to prevent injection into json_extract path
            if !key
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
            {
                return Err(ThingdError::InvalidInput(format!(
                    "Invalid filter key: '{key}'. Only alphanumeric, underscore, and dot characters are allowed."
                )));
            }
            conditions.push(format!("json_extract(body, '$.{key}') = ?"));
            let sql_val: Box<dyn rusqlite::types::ToSql> = match value {
                serde_json::Value::String(s) => Box::new(s.clone()),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        Box::new(i)
                    } else {
                        Box::new(n.as_f64().unwrap_or(0.0))
                    }
                },
                serde_json::Value::Bool(b) => Box::new(i64::from(*b)),
                serde_json::Value::Null => Box::new(rusqlite::types::Null),
                other => Box::new(other.to_string()),
            };
            bound_values.push(sql_val);
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // Use bound parameters for LIMIT/OFFSET
        let limit_clause = match (options.limit, options.offset) {
            (Some(l), Some(o)) => {
                bound_values.push(Box::new(i64::try_from(l).unwrap_or(i64::MAX)));
                bound_values.push(Box::new(i64::try_from(o).unwrap_or(i64::MAX)));
                " LIMIT ? OFFSET ?"
            },
            (Some(l), None) => {
                bound_values.push(Box::new(i64::try_from(l).unwrap_or(i64::MAX)));
                " LIMIT ?"
            },
            (None, Some(o)) => {
                bound_values.push(Box::new(i64::try_from(o).unwrap_or(i64::MAX)));
                " LIMIT -1 OFFSET ?"
            },
            (None, None) => "",
        };

        let order_clause = options.sort_by.as_ref().map_or_else(
            || "ORDER BY collection, id".to_string(),
            |sort_by| {
                let col = match sort_by.field.as_str() {
                    "id" => "id",
                    "collection" => "collection",
                    "created_at" => "created_at",
                    "updated_at" => "updated_at",
                    "version" => "version",
                    _ => "collection, id",
                };
                let dir = match sort_by.direction {
                    crate::model::SortDirection::Asc => "ASC",
                    crate::model::SortDirection::Desc => "DESC",
                };
                format!("ORDER BY {col} {dir}")
            },
        );

        let sql = format!(
            "SELECT collection, id, body, version, created_at, updated_at FROM objects {where_clause} {order_clause} {limit_clause}"
        );

        let mut statement = self.connection.prepare(&sql).map_err(ThingdError::from)?;
        let params: Vec<&dyn rusqlite::types::ToSql> =
            bound_values.iter().map(AsRef::as_ref).collect();
        let rows = statement
            .query_map(params.as_slice(), row_to_object)
            .map_err(ThingdError::from)?;

        let mut objects = Vec::new();
        for row in rows {
            objects.push(row.map_err(ThingdError::from)?);
        }
        Ok(objects)
    }

    fn delete_object(&mut self, collection: &str, id: &str) -> ThingdResult<bool> {
        let transaction = self.connection.transaction().map_err(ThingdError::from)?;
        let changed = transaction
            .execute(
                "DELETE FROM objects WHERE collection = ?1 AND id = ?2",
                params![collection, id],
            )
            .map_err(ThingdError::from)?;

        if changed > 0 {
            transaction
                .execute(
                    "DELETE FROM search_index WHERE collection = ?1 AND id = ?2 AND kind = 'object'",
                    params![collection, id],
                )
                .map_err(ThingdError::from)?;
        }

        transaction.commit().map_err(ThingdError::from)?;
        Ok(changed > 0)
    }

    fn delete_objects_batch(&mut self, keys: &[(String, String)]) -> ThingdResult<u64> {
        use std::fmt::Write;

        let transaction = self.connection.transaction().map_err(ThingdError::from)?;

        if keys.is_empty() {
            return Ok(0);
        }

        let mut total_deleted = 0u64;
        // Chunk to avoid SQLite expression tree depth limit (max ~500 per chunk is safe)
        for chunk in keys.chunks(500) {
            let mut sql = String::from("DELETE FROM objects WHERE ");
            let mut fts_sql = String::from("DELETE FROM search_index WHERE kind = 'object' AND (");
            let mut param_values: Vec<String> = Vec::with_capacity(chunk.len() * 2);
            for (i, (collection, id)) in chunk.iter().enumerate() {
                if i > 0 {
                    sql.push_str(" OR ");
                    fts_sql.push_str(" OR ");
                }
                let ci = i * 2 + 1;
                let ii = i * 2 + 2;
                let _ = write!(sql, "(collection = ?{ci} AND id = ?{ii})");
                let _ = write!(fts_sql, "(collection = ?{ci} AND id = ?{ii})");
                param_values.push(collection.clone());
                param_values.push(id.clone());
            }
            fts_sql.push(')');
            let param_slices: Vec<&dyn rusqlite::types::ToSql> = param_values
                .iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();

            let deleted = transaction
                .execute(&sql, param_slices.as_slice())
                .map_err(ThingdError::from)?;
            transaction
                .execute(&fts_sql, param_slices.as_slice())
                .map_err(ThingdError::from)?;
            total_deleted += deleted as u64;
        }

        transaction.commit().map_err(ThingdError::from)?;
        Ok(total_deleted)
    }

    fn count_objects(&self) -> ThingdResult<u64> {
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM objects", [], |row| row.get(0))
            .map_err(ThingdError::from)?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    fn list_collections(&self) -> ThingdResult<Vec<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT DISTINCT collection FROM objects ORDER BY collection")
            .map_err(ThingdError::from)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(ThingdError::from)?;

        let mut collections = Vec::new();
        for row in rows {
            collections.push(row.map_err(ThingdError::from)?);
        }
        Ok(collections)
    }
}

impl EventLog for SqliteThingStore {
    fn append_event(&mut self, mut event: MemoryEvent) -> ThingdResult<MemoryEvent> {
        // Idempotency check: if idempotency_key is set and known, return existing event
        if !event.idempotency_key.is_empty()
            && let Some(&existing_seq) = self
                .event_idempotency_keys
                .get(&(event.stream.clone(), event.idempotency_key.clone()))
        {
            let existing = self.connection.query_row(
                "SELECT stream, event_type, body, sequence, created_at FROM events WHERE stream = ?1 AND sequence = ?2",
                params![&event.stream, existing_seq.cast_signed()],
                |row| {
                    Ok(MemoryEvent {
                        stream: row.get(0)?,
                        event_type: row.get(1)?,
                        body: row.get(2)?,
                        sequence: row.get::<_, i64>(3)?.cast_unsigned(),
                        created_at: row.get(4)?,
                        idempotency_key: event.idempotency_key.clone(),
                    })
                },
            ).map_err(ThingdError::from)?;
            return Ok(existing);
        }

        let transaction = self.connection.transaction().map_err(ThingdError::from)?;

        let (sequence, created_at): (i64, String) = transaction
            .query_row(
                r"
                INSERT INTO events (stream, event_type, body, created_at)
                VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                RETURNING sequence, created_at
                ",
                params![&event.stream, &event.event_type, &event.body],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .map_err(ThingdError::from)?;

        event.sequence =
            u64::try_from(sequence).map_err(|error| ThingdError::Storage(error.to_string()))?;
        event.created_at = created_at;

        // Track idempotency key
        if !event.idempotency_key.is_empty() {
            self.event_idempotency_keys.insert(
                (event.stream.clone(), event.idempotency_key.clone()),
                event.sequence,
            );
        }

        let text = extract_text_from_json(&event.body);
        let seq_str = sequence.to_string();
        transaction
            .execute(
                "INSERT INTO search_index (collection, id, kind, text) VALUES (?1, ?2, 'event', ?3)",
                params![&event.stream, seq_str, text],
            )
            .map_err(ThingdError::from)?;

        transaction.commit().map_err(ThingdError::from)?;

        Ok(event)
    }

    fn append_events_batch(&mut self, events: Vec<MemoryEvent>) -> ThingdResult<Vec<MemoryEvent>> {
        let transaction = self.connection.transaction().map_err(ThingdError::from)?;

        let mut results = Vec::with_capacity(events.len());
        for event in events {
            let (sequence, created_at): (i64, String) = transaction
                .query_row(
                    r"
                    INSERT INTO events (stream, event_type, body, created_at)
                    VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                    RETURNING sequence, created_at
                    ",
                    params![&event.stream, &event.event_type, &event.body],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                )
                .map_err(ThingdError::from)?;

            let mut result = event;
            result.sequence =
                u64::try_from(sequence).map_err(|error| ThingdError::Storage(error.to_string()))?;
            result.created_at = created_at;

            let text = extract_text_from_json(&result.body);
            let seq_str = sequence.to_string();
            transaction
                .execute(
                    "INSERT INTO search_index (collection, id, kind, text) VALUES (?1, ?2, 'event', ?3)",
                    params![&result.stream, seq_str, text],
                )
                .map_err(ThingdError::from)?;

            results.push(result);
        }

        transaction.commit().map_err(ThingdError::from)?;

        Ok(results)
    }

    fn list_events(
        &self,
        stream: Option<&str>,
        options: ListEventsOptions,
    ) -> ThingdResult<Vec<MemoryEvent>> {
        let mut events = Vec::new();

        let mut sql = String::from(
            "SELECT stream, event_type, body, sequence, created_at FROM events WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(stream) = stream {
            let idx = param_values.len() + 1;
            write!(sql, " AND stream = ?{idx}").unwrap();
            param_values.push(Box::new(stream.to_string()));
        }

        if let Some(from_sequence) = options.from_sequence {
            let idx = param_values.len() + 1;
            write!(sql, " AND sequence > ?{idx}").unwrap();
            param_values.push(Box::new(from_sequence.cast_signed()));
        }

        sql.push_str(" ORDER BY sequence");

        if let Some(limit) = options.limit {
            sql.push_str(" LIMIT ?");
            param_values.push(Box::new(limit.cast_signed()));
        }

        let mut statement = self.connection.prepare(&sql).map_err(ThingdError::from)?;

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(AsRef::as_ref).collect();

        let rows = statement
            .query_map(param_refs.as_slice(), row_to_event)
            .map_err(ThingdError::from)?;

        for row in rows {
            events.push(row.map_err(ThingdError::from)?);
        }

        Ok(events)
    }

    fn count_events(&self) -> ThingdResult<u64> {
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .map_err(ThingdError::from)?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    fn list_streams(&self) -> ThingdResult<Vec<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT DISTINCT stream FROM events ORDER BY stream")
            .map_err(ThingdError::from)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(ThingdError::from)?;

        let mut streams = Vec::new();
        for row in rows {
            streams.push(row.map_err(ThingdError::from)?);
        }
        Ok(streams)
    }

    fn delete_last_event(&mut self, stream: &str) -> ThingdResult<Option<MemoryEvent>> {
        let transaction = self.connection.transaction().map_err(ThingdError::from)?;

        let result = transaction
            .query_row(
                r"
                DELETE FROM events
                WHERE sequence = (
                    SELECT MAX(sequence) FROM events WHERE stream = ?1
                )
                RETURNING stream, event_type, body, sequence, created_at
                ",
                params![stream],
                row_to_event,
            )
            .optional()
            .map_err(ThingdError::from)?;

        if let Some(ref event) = result {
            // Delete the single FTS entry for this event instead of re-indexing the entire stream
            let seq_str = event.sequence.to_string();
            transaction
                .execute(
                    "DELETE FROM search_index WHERE collection = ?1 AND kind = 'event' AND id = ?2",
                    params![stream, seq_str],
                )
                .map_err(ThingdError::from)?;
        }

        transaction.commit().map_err(ThingdError::from)?;
        Ok(result)
    }

    fn delete_stream(&mut self, stream: &str) -> ThingdResult<u64> {
        let transaction = self.connection.transaction().map_err(ThingdError::from)?;

        let count = transaction
            .execute("DELETE FROM events WHERE stream = ?1", params![stream])
            .map_err(ThingdError::from)?;

        transaction
            .execute(
                "DELETE FROM search_index WHERE collection = ?1 AND kind = 'event'",
                params![stream],
            )
            .map_err(ThingdError::from)?;

        transaction.commit().map_err(ThingdError::from)?;
        Ok(count as u64)
    }
}

impl QueueStore for SqliteThingStore {
    fn push_job(&mut self, job: QueueJob) -> ThingdResult<QueueJob> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(ThingdError::from)?;

        if let Some(existing) = transaction
            .query_row(
                &queue_job_select_sql("WHERE queue = ?1 AND id = ?2"),
                params![&job.queue, &job.id],
                row_to_queue_job,
            )
            .optional()
            .map_err(ThingdError::from)?
        {
            transaction.commit().map_err(ThingdError::from)?;
            return Ok(existing);
        }

        // Use RETURNING to get created_at in a single round-trip
        let created_at: String = transaction
            .query_row(
                r"
                INSERT INTO queue_jobs (
                    queue,
                    id,
                    body,
                    attempts,
                    max_attempts,
                    status,
                    available_at_ms,
                    leased_at_ms,
                    lease_expires_at_ms,
                    completed_at_ms,
                    dead_at_ms,
                    created_at,
                    updated_at
                )
                VALUES (
                    ?1,
                    ?2,
                    ?3,
                    ?4,
                    ?5,
                    ?6,
                    ?7,
                    ?8,
                    ?9,
                    ?10,
                    ?11,
                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                )
                RETURNING created_at
                ",
                params![
                    &job.queue,
                    &job.id,
                    &job.body,
                    u32_to_i64(job.attempts),
                    u32_to_i64(job.max_attempts),
                    status_to_str(job.status),
                    job.available_at_ms,
                    job.leased_at_ms,
                    job.lease_expires_at_ms,
                    job.completed_at_ms,
                    job.dead_at_ms
                ],
                |row| row.get(0),
            )
            .map_err(ThingdError::from)?;

        transaction.commit().map_err(ThingdError::from)?;

        Ok(QueueJob { created_at, ..job })
    }

    fn push_jobs_batch(&mut self, jobs: Vec<QueueJob>) -> ThingdResult<Vec<QueueJob>> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(ThingdError::from)?;

        let mut results = Vec::with_capacity(jobs.len());
        for job in jobs {
            if let Some(existing) = transaction
                .query_row(
                    &queue_job_select_sql("WHERE queue = ?1 AND id = ?2"),
                    params![&job.queue, &job.id],
                    row_to_queue_job,
                )
                .optional()
                .map_err(ThingdError::from)?
            {
                results.push(existing);
                continue;
            }

            // Use RETURNING to get created_at in a single round-trip
            let created_at: String = transaction
                .query_row(
                    r"
                    INSERT INTO queue_jobs (
                        queue,
                        id,
                        body,
                        attempts,
                        max_attempts,
                        status,
                        available_at_ms,
                        leased_at_ms,
                        lease_expires_at_ms,
                        completed_at_ms,
                        dead_at_ms,
                        created_at,
                        updated_at
                    )
                    VALUES (
                        ?1,
                        ?2,
                        ?3,
                        ?4,
                        ?5,
                        ?6,
                        ?7,
                        ?8,
                        ?9,
                        ?10,
                        ?11,
                        strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    )
                    RETURNING created_at
                    ",
                    params![
                        &job.queue,
                        &job.id,
                        &job.body,
                        u32_to_i64(job.attempts),
                        u32_to_i64(job.max_attempts),
                        status_to_str(job.status),
                        job.available_at_ms,
                        job.leased_at_ms,
                        job.lease_expires_at_ms,
                        job.completed_at_ms,
                        job.dead_at_ms
                    ],
                    |row| row.get(0),
                )
                .map_err(ThingdError::from)?;

            results.push(QueueJob { created_at, ..job });
        }

        transaction.commit().map_err(ThingdError::from)?;

        Ok(results)
    }

    fn claim_job_with_options(
        &mut self,
        queue: &str,
        options: QueueClaimOptions,
    ) -> ThingdResult<Option<QueueJob>> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(ThingdError::from)?;

        release_expired_leases(&transaction, queue)?;
        let now = unix_timestamp_millis();
        let Some(mut job) = transaction
            .query_row(
                &queue_job_select_sql(
                    "WHERE queue = ?1 AND status = 'ready' AND available_at_ms <= ?2 ORDER BY created_at LIMIT 1",
                ),
                params![queue, now],
                row_to_queue_job,
            )
            .optional()
            .map_err(ThingdError::from)?
        else {
            transaction.commit().map_err(ThingdError::from)?;
            return Ok(None);
        };

        job.status = QueueJobStatus::Leased;
        job.attempts += 1;
        job.leased_at_ms = Some(now);
        job.lease_expires_at_ms = Some(now.saturating_add(u64_to_i64(options.lease_ms)));

        transaction
            .execute(
                r"
                UPDATE queue_jobs
                SET attempts = ?3,
                    status = ?4,
                    leased_at_ms = ?5,
                    lease_expires_at_ms = ?6,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                WHERE queue = ?1 AND id = ?2
                ",
                params![
                    &job.queue,
                    &job.id,
                    u32_to_i64(job.attempts),
                    status_to_str(job.status),
                    job.leased_at_ms,
                    job.lease_expires_at_ms
                ],
            )
            .map_err(ThingdError::from)?;

        transaction.commit().map_err(ThingdError::from)?;
        Ok(Some(job))
    }

    fn ack_job(&mut self, queue: &str, id: &str) -> ThingdResult<Option<QueueJob>> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(ThingdError::from)?;

        let Some(mut job) = transaction
            .query_row(
                &queue_job_select_sql("WHERE queue = ?1 AND id = ?2"),
                params![queue, id],
                row_to_queue_job,
            )
            .optional()
            .map_err(ThingdError::from)?
        else {
            transaction.commit().map_err(ThingdError::from)?;
            return Ok(None);
        };

        if job.status != QueueJobStatus::Leased {
            return Err(ThingdError::Conflict(format!(
                "job {id} must be leased before ack"
            )));
        }

        job.status = QueueJobStatus::Completed;
        job.completed_at_ms = Some(unix_timestamp_millis());
        transaction
            .execute(
                r"
                UPDATE queue_jobs
                SET status = ?3,
                    completed_at_ms = ?4,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                WHERE queue = ?1 AND id = ?2
                ",
                params![queue, id, status_to_str(job.status), job.completed_at_ms],
            )
            .map_err(ThingdError::from)?;

        transaction.commit().map_err(ThingdError::from)?;
        Ok(Some(job))
    }

    fn claim_and_ack(
        &mut self,
        queue: &str,
        options: QueueClaimOptions,
    ) -> ThingdResult<Option<QueueJob>> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(ThingdError::from)?;

        release_expired_leases(&transaction, queue)?;
        let now = unix_timestamp_millis();
        let Some(mut job) = transaction
            .query_row(
                &queue_job_select_sql(
                    "WHERE queue = ?1 AND status = 'ready' AND available_at_ms <= ?2 ORDER BY created_at LIMIT 1",
                ),
                params![queue, now],
                row_to_queue_job,
            )
            .optional()
            .map_err(ThingdError::from)?
        else {
            transaction.commit().map_err(ThingdError::from)?;
            return Ok(None);
        };

        // Claim the job
        job.status = QueueJobStatus::Leased;
        job.attempts += 1;
        job.leased_at_ms = Some(now);
        job.lease_expires_at_ms = Some(now.saturating_add(u64_to_i64(options.lease_ms)));

        transaction
            .execute(
                r"
                UPDATE queue_jobs
                SET attempts = ?3,
                    status = ?4,
                    leased_at_ms = ?5,
                    lease_expires_at_ms = ?6,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                WHERE queue = ?1 AND id = ?2
                ",
                params![
                    &job.queue,
                    &job.id,
                    u32_to_i64(job.attempts),
                    status_to_str(job.status),
                    job.leased_at_ms,
                    job.lease_expires_at_ms
                ],
            )
            .map_err(ThingdError::from)?;

        // Immediately ack the job
        job.status = QueueJobStatus::Completed;
        job.completed_at_ms = Some(unix_timestamp_millis());
        transaction
            .execute(
                r"
                UPDATE queue_jobs
                SET status = ?3,
                    completed_at_ms = ?4,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                WHERE queue = ?1 AND id = ?2
                ",
                params![
                    &job.queue,
                    &job.id,
                    status_to_str(job.status),
                    job.completed_at_ms
                ],
            )
            .map_err(ThingdError::from)?;

        transaction.commit().map_err(ThingdError::from)?;
        Ok(Some(job))
    }

    fn nack_job_with_options(
        &mut self,
        queue: &str,
        id: &str,
        options: QueueNackOptions,
    ) -> ThingdResult<Option<QueueJob>> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(ThingdError::from)?;

        let Some(mut job) = transaction
            .query_row(
                &queue_job_select_sql("WHERE queue = ?1 AND id = ?2"),
                params![queue, id],
                row_to_queue_job,
            )
            .optional()
            .map_err(ThingdError::from)?
        else {
            transaction.commit().map_err(ThingdError::from)?;
            return Ok(None);
        };

        if job.status != QueueJobStatus::Leased {
            return Err(ThingdError::Conflict(format!(
                "job {id} must be leased before nack"
            )));
        }

        let now = unix_timestamp_millis();
        job.leased_at_ms = None;
        job.lease_expires_at_ms = None;

        job.status = if job.attempts >= job.max_attempts {
            job.dead_at_ms = Some(now);
            QueueJobStatus::Dead
        } else {
            job.available_at_ms = now.saturating_add(u64_to_i64(options.delay_ms));
            QueueJobStatus::Ready
        };

        if !options.error.is_empty() {
            job.last_error = options.error;
        }

        transaction
            .execute(
                r"
                UPDATE queue_jobs
                SET attempts = ?3,
                    status = ?4,
                    available_at_ms = ?5,
                    leased_at_ms = NULL,
                    lease_expires_at_ms = NULL,
                    dead_at_ms = ?6,
                    last_error = ?7,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                WHERE queue = ?1 AND id = ?2
                ",
                params![
                    queue,
                    id,
                    u32_to_i64(job.attempts),
                    status_to_str(job.status),
                    job.available_at_ms,
                    job.dead_at_ms,
                    job.last_error
                ],
            )
            .map_err(ThingdError::from)?;

        transaction.commit().map_err(ThingdError::from)?;
        Ok(Some(job))
    }

    fn list_jobs(&self, queue: &str) -> ThingdResult<Vec<QueueJob>> {
        let mut statement = self
            .connection
            .prepare(&queue_job_select_sql(
                "WHERE queue = ?1 ORDER BY created_at LIMIT 1000",
            ))
            .map_err(ThingdError::from)?;
        let rows = statement
            .query_map(params![queue], row_to_queue_job)
            .map_err(ThingdError::from)?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(row.map_err(ThingdError::from)?);
        }

        Ok(jobs)
    }

    fn list_dead_jobs(&self, queue: &str) -> ThingdResult<Vec<QueueJob>> {
        let mut statement = self
            .connection
            .prepare(&queue_job_select_sql(
                "WHERE queue = ?1 AND status = 'dead' ORDER BY created_at LIMIT 1000",
            ))
            .map_err(ThingdError::from)?;
        let rows = statement
            .query_map(params![queue], row_to_queue_job)
            .map_err(ThingdError::from)?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(row.map_err(ThingdError::from)?);
        }

        Ok(jobs)
    }

    fn list_queues(&self) -> ThingdResult<Vec<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT DISTINCT queue FROM queue_jobs ORDER BY queue")
            .map_err(ThingdError::from)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(ThingdError::from)?;

        let mut queues = Vec::new();
        for row in rows {
            queues.push(row.map_err(ThingdError::from)?);
        }
        Ok(queues)
    }

    fn count_active_jobs(&self) -> ThingdResult<u64> {
        let count = self
            .connection
            .query_row(
                "SELECT COUNT(id) FROM queue_jobs WHERE status != 'dead'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(ThingdError::from)?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    fn count_dead_jobs(&self) -> ThingdResult<u64> {
        let count = self
            .connection
            .query_row(
                "SELECT COUNT(id) FROM queue_jobs WHERE status = 'dead'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(ThingdError::from)?;
        Ok(u64::try_from(count).unwrap_or(0))
    }
}

impl crate::store::Searcher for SqliteThingStore {
    #[allow(clippy::too_many_lines)]
    fn search(
        &self,
        query: &str,
        options: crate::SearchOptions,
    ) -> ThingdResult<Vec<crate::SearchHit>> {
        let sanitized = sanitize_fts_query(query);
        if sanitized.is_empty() {
            return Ok(Vec::new());
        }
        // Reject single-character queries to prevent FTS5 prefix explosion
        if query.chars().filter(|c| c.is_alphanumeric()).count() < 2 {
            return Ok(Vec::new());
        }

        let mut sql = String::from(
            r"
            SELECT 
                s.kind,
                s.collection,
                s.id,
                s.text,
                o.body AS object_body,
                o.version AS object_version,
                o.created_at AS object_created_at,
                o.updated_at AS object_updated_at,
                e.event_type AS event_type,
                e.body AS event_body,
                e.created_at AS event_created_at,
                bm25(search_index) AS bm25_score,
                (strftime('%s', 'now') - strftime('%s', coalesce(o.created_at, e.created_at))) AS age_seconds
            FROM search_index s
            LEFT JOIN objects o ON s.kind = 'object' AND s.collection = o.collection AND s.id = o.id
            LEFT JOIN events e ON s.kind = 'event' AND s.collection = e.stream AND s.id = CAST(e.sequence AS TEXT)
            WHERE search_index MATCH ?1
              AND (s.kind != 'object' OR o.collection IS NOT NULL)
              AND (s.kind != 'event'  OR e.stream IS NOT NULL)
            ",
        );

        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(sanitized)];

        // Push collection filter to SQL WHERE clause
        if let Some(ref collections) = options.collections
            && !collections.is_empty()
        {
            let placeholders: Vec<String> = (0..collections.len())
                .map(|i| format!("?{}", params.len() + i + 1))
                .collect();
            write!(sql, " AND s.collection IN ({})", placeholders.join(",")).unwrap();
            for coll in collections {
                params.push(Box::new(coll.clone()));
            }
        }

        // Enforce hard upper bound on search results (no LIMIT clause makes
        // the query scan the entire FTS index)
        let effective_limit = options.limit.map_or(100, |l| l.min(1000));

        sql.push_str(" ORDER BY bm25_score");

        // Push LIMIT to SQL (approximate — may fetch extra for post-filter)
        if options.filter.is_none() {
            write!(sql, " LIMIT ?{}", params.len() + 1).unwrap();
            params.push(Box::new(i64::try_from(effective_limit).unwrap_or(100)));
        } else {
            // With metadata filter, fetch extra to compensate for post-filter drops
            let fetch_limit = (effective_limit * 3).min(1000);
            write!(sql, " LIMIT ?{}", params.len() + 1).unwrap();
            params.push(Box::new(i64::try_from(fetch_limit).unwrap_or(1000)));
        }

        let mut statement = self.connection.prepare(&sql).map_err(ThingdError::from)?;

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(AsRef::as_ref).collect();

        let rows = statement
            .query_map(param_refs.as_slice(), |row| {
                let kind: String = row.get(0)?;
                let collection: String = row.get(1)?;
                let id: String = row.get(2)?;
                let text: String = row.get(3)?;
                let bm25_score: f64 = row.get(11)?;
                let age_seconds: Option<i64> = row.get(12)?;

                let relevance_score = -bm25_score;
                let age =
                    f64::from(i32::try_from(age_seconds.unwrap_or(0).max(0)).unwrap_or(i32::MAX));
                let recency_factor = 1.0 / (1.0 + age / 86400.0);
                let score = relevance_score * recency_factor;

                let (body, version, created_at, updated_at, event_type) = if kind == "object" {
                    let object_body: String = row.get(4)?;
                    let object_version: i64 = row.get(5)?;
                    let object_created_at: String = row.get(6)?;
                    let object_updated_at: String = row.get(7)?;
                    (
                        object_body,
                        Some(object_version.cast_unsigned()),
                        object_created_at,
                        Some(object_updated_at),
                        None,
                    )
                } else {
                    let event_type_val: String = row.get(8)?;
                    let event_body: String = row.get(9)?;
                    let event_created_at: String = row.get(10)?;
                    (
                        event_body,
                        None,
                        event_created_at,
                        None,
                        Some(event_type_val),
                    )
                };

                Ok(crate::SearchHit {
                    kind,
                    collection,
                    id,
                    text,
                    score,
                    body,
                    version,
                    created_at,
                    updated_at,
                    event_type,
                })
            })
            .map_err(ThingdError::from)?;

        let mut hits = Vec::new();
        for row in rows {
            let hit = row.map_err(ThingdError::from)?;

            // Apply metadata filter (must remain in Rust — JSON key-value matching)
            if let Some(ref filter) = options.filter
                && !matches_filter(&hit.body, filter)
            {
                continue;
            }

            hits.push(hit);
        }

        // Sort by score descending
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Limit results if requested
        if let Some(limit) = options.limit {
            hits.truncate(limit);
        }

        Ok(hits)
    }
}

impl crate::store::LinkStore for SqliteThingStore {
    fn create_link(&mut self, link: crate::Link) -> ThingdResult<crate::Link> {
        let id = uuid::Uuid::new_v4().to_string();
        self.connection
            .execute(
                r"
                INSERT INTO links (id, from_ref, type, to_ref, weight, metadata_json, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                ",
                params![
                    id,
                    link.from_ref,
                    link.link_type,
                    link.to_ref,
                    link.weight,
                    link.metadata_json
                ],
            )
            .map_err(ThingdError::from)?;

        let created_at: String = self
            .connection
            .query_row(
                "SELECT created_at FROM links WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .map_err(ThingdError::from)?;

        Ok(crate::Link {
            id,
            from_ref: link.from_ref,
            link_type: link.link_type,
            to_ref: link.to_ref,
            weight: link.weight,
            metadata_json: link.metadata_json,
            created_at,
        })
    }

    fn delete_link(&mut self, id: &str) -> ThingdResult<bool> {
        let changed = self
            .connection
            .execute("DELETE FROM links WHERE id = ?1", params![id])
            .map_err(ThingdError::from)?;
        Ok(changed > 0)
    }

    fn get_link(&self, id: &str) -> ThingdResult<Option<crate::Link>> {
        self.connection
            .query_row(
                "SELECT id, from_ref, type, to_ref, weight, metadata_json, created_at FROM links WHERE id = ?1",
                params![id],
                row_to_link,
            )
            .optional()
            .map_err(ThingdError::from)
    }

    fn get_neighbors(
        &self,
        reference: &str,
        direction: crate::LinkDirection,
        options: crate::LinkQueryOptions,
    ) -> ThingdResult<Vec<crate::Link>> {
        let (where_clause, param_value): (&str, String) = match direction {
            crate::LinkDirection::Outgoing => ("WHERE from_ref = ?1", reference.to_string()),
            crate::LinkDirection::Incoming => ("WHERE to_ref = ?1", reference.to_string()),
            crate::LinkDirection::Both => (
                "WHERE (from_ref = ?1 OR to_ref = ?1)",
                reference.to_string(),
            ),
        };

        // Build SQL with parameterized type filter to prevent SQL injection
        let (type_filter_sql, type_param) = options.link_type.as_ref().map_or_else(
            || (String::new(), None),
            |t| (" AND type = ?2".to_string(), Some(t.clone())),
        );

        let limit_clause = options
            .limit
            .map_or_else(String::new, |_| " LIMIT ?".to_string());

        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        params.push(Box::new(param_value));
        if let Some(ref t) = type_param {
            params.push(Box::new(t.clone()));
        }
        if let Some(l) = options.limit {
            params.push(Box::new(i64::try_from(l).unwrap_or(1000)));
        }

        let sql = format!(
            "SELECT id, from_ref, type, to_ref, weight, metadata_json, created_at FROM links {where_clause}{type_filter_sql}{limit_clause}"
        );

        let mut statement = self.connection.prepare(&sql).map_err(ThingdError::from)?;

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(AsRef::as_ref).collect();

        let rows = statement
            .query_map(param_refs.as_slice(), row_to_link)
            .map_err(ThingdError::from)?;

        let mut links = Vec::new();
        for row in rows {
            links.push(row.map_err(ThingdError::from)?);
        }

        Ok(links)
    }

    fn count_links(&self) -> ThingdResult<u64> {
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM links", [], |row| row.get(0))
            .map_err(ThingdError::from)?;
        Ok(u64::try_from(count).unwrap_or(0))
    }
}

fn row_to_link(row: &rusqlite::Row<'_>) -> rusqlite::Result<crate::Link> {
    Ok(crate::Link {
        id: row.get(0)?,
        from_ref: row.get(1)?,
        link_type: row.get(2)?,
        to_ref: row.get(3)?,
        weight: row.get(4)?,
        metadata_json: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn row_to_object(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryObject> {
    let version = row.get::<_, i64>(3)?;

    Ok(MemoryObject {
        key: ObjectKey::new(row.get::<_, String>(0)?, row.get::<_, String>(1)?),
        body: row.get(2)?,
        version: u64::try_from(version).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })?,
        created_at: row.get::<_, String>(4).unwrap_or_default(),
        updated_at: row.get::<_, String>(5).unwrap_or_default(),
    })
}

fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryEvent> {
    let sequence = row.get::<_, i64>(3)?;

    Ok(MemoryEvent {
        stream: row.get(0)?,
        event_type: row.get(1)?,
        body: row.get(2)?,
        sequence: u64::try_from(sequence).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })?,
        created_at: row.get::<_, String>(4).unwrap_or_default(),
        idempotency_key: String::new(),
    })
}

fn queue_job_select_sql(predicate: &str) -> String {
    format!(
        "SELECT queue, id, body, attempts, max_attempts, status, available_at_ms, leased_at_ms, lease_expires_at_ms, completed_at_ms, dead_at_ms, created_at, last_error FROM queue_jobs {predicate}"
    )
}

fn row_to_queue_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueueJob> {
    let attempts = row.get::<_, i64>(3)?;
    let max_attempts = row.get::<_, i64>(4)?;
    let status = row.get::<_, String>(5)?;

    Ok(QueueJob {
        queue: row.get(0)?,
        id: row.get(1)?,
        body: row.get(2)?,
        attempts: u32::try_from(attempts).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })?,
        max_attempts: u32::try_from(max_attempts).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })?,
        status: match status.as_str() {
            "ready" => QueueJobStatus::Ready,
            "leased" => QueueJobStatus::Leased,
            "completed" => QueueJobStatus::Completed,
            "dead" => QueueJobStatus::Dead,
            _other => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(std::fmt::Error),
                ));
            },
        },
        available_at_ms: row.get(6)?,
        leased_at_ms: row.get(7)?,
        lease_expires_at_ms: row.get(8)?,
        completed_at_ms: row.get(9)?,
        dead_at_ms: row.get(10)?,
        created_at: row.get::<_, String>(11).unwrap_or_default(),
        last_error: row.get::<_, String>(12).unwrap_or_default(),
    })
}

fn release_expired_leases(connection: &rusqlite::Connection, queue: &str) -> ThingdResult<()> {
    connection
        .execute(
            r"
            UPDATE queue_jobs
            SET status = 'ready',
                leased_at_ms = NULL,
                lease_expires_at_ms = NULL,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE queue = ?1
                AND status = 'leased'
                AND lease_expires_at_ms IS NOT NULL
                AND lease_expires_at_ms <= ?2
            ",
            params![queue, unix_timestamp_millis()],
        )
        .map_err(ThingdError::from)?;

    Ok(())
}

const fn status_to_str(status: QueueJobStatus) -> &'static str {
    match status {
        QueueJobStatus::Ready => "ready",
        QueueJobStatus::Leased => "leased",
        QueueJobStatus::Completed => "completed",
        QueueJobStatus::Dead => "dead",
    }
}

fn u32_to_i64(value: u32) -> i64 {
    i64::from(value)
}

fn sanitize_fts_query(query: &str) -> String {
    let mut cleaned = String::new();
    let normalized: String = query
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect();

    for word in normalized.split_whitespace() {
        if !word.is_empty() {
            if !cleaned.is_empty() {
                cleaned.push(' ');
            }
            cleaned.push_str(word);
            cleaned.push('*');
        }
    }
    cleaned
}

fn extract_text_from_json(json_str: &str) -> String {
    serde_json::from_str::<serde_json::Value>(json_str).map_or_else(
        |_| json_str.to_string(),
        |value| {
            let mut out = String::new();
            collect_strings(&value, &mut out);
            out.trim().to_string()
        },
    )
}

fn collect_strings(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::String(s) => {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(s);
        },
        serde_json::Value::Array(arr) => {
            for val in arr {
                collect_strings(val, out);
            }
        },
        serde_json::Value::Object(obj) => {
            for (key, val) in obj {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(key);
                collect_strings(val, out);
            }
        },
        serde_json::Value::Number(num) => {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&num.to_string());
        },
        serde_json::Value::Bool(b) => {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&b.to_string());
        },
        serde_json::Value::Null => {},
    }
}

fn matches_filter(body_str: &str, filter: &serde_json::Value) -> bool {
    let Ok(body) = serde_json::from_str::<serde_json::Value>(body_str) else {
        return false;
    };

    let Some(filter_obj) = filter.as_object() else {
        return true;
    };

    for (k, v) in filter_obj {
        if body.get(k) != Some(v) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    use super::*;
    use crate::store::Searcher;
    use crate::{ListObjectsOptions, SearchOptions};

    #[test]
    fn records_schema_version_on_initialize() {
        let store = SqliteThingStore::open_in_memory().unwrap();

        assert_eq!(store.schema_version().unwrap(), SQLITE_SCHEMA_VERSION);
    }

    #[test]
    fn integrity_check_passes_on_fresh_store() {
        let store = SqliteThingStore::open_in_memory().unwrap();
        let ok: String = store
            .connection
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ok, "ok");
    }

    #[test]
    fn wal_checkpoint_returns_zero_frames_on_in_memory() {
        let store = SqliteThingStore::open_in_memory().unwrap();
        // Just verify the method doesn't panic — in-memory DBs aren't in WAL mode
        let (_busy, _frames) = store.wal_checkpoint().unwrap();
    }

    #[test]
    fn backup_to_creates_valid_database() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();
        store
            .put_object(MemoryObject::new("test", "1", r#"{"v":1}"#))
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let backup_path = dir.path().join("backup.db");
        let path_str = backup_path.to_str().unwrap().to_string();

        store.backup_to(&path_str).unwrap();
        assert!(backup_path.exists());

        let backup = SqliteThingStore::open(&backup_path).unwrap();
        let obj = backup.get_object("test", "1").unwrap();
        assert!(obj.is_some());
        assert_eq!(obj.unwrap().key.id, "1");
    }

    #[test]
    fn rejects_newer_schema_versions() {
        let file = NamedTempFile::new().unwrap();
        let connection = Connection::open(file.path()).unwrap();
        connection
            .execute_batch(
                r"
                CREATE TABLE thingd_schema_migrations (
                    version INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    applied_at TEXT NOT NULL
                );

                INSERT INTO thingd_schema_migrations (version, name, applied_at)
                VALUES (999, 'future', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
                ",
            )
            .unwrap();

        let Err(error) = SqliteThingStore::open(file.path()) else {
            panic!("expected newer schema version to be rejected");
        };

        assert!(error.to_string().contains("newer than supported version"));
    }

    #[test]
    fn stores_objects_across_reopen() {
        let file = NamedTempFile::new().unwrap();

        {
            let mut store = SqliteThingStore::open(file.path()).unwrap();
            let object = store
                .put_object(MemoryObject::new(
                    "decisions",
                    "sqlite-backend",
                    "{\"text\":\"Use SQLite\"}",
                ))
                .unwrap();

            assert_eq!(object.version, 1);
        }

        let store = SqliteThingStore::open(file.path()).unwrap();
        let object = store
            .get_object("decisions", "sqlite-backend")
            .unwrap()
            .unwrap();

        assert_eq!(object.body, "{\"text\":\"Use SQLite\"}");
        assert_eq!(object.version, 1);
    }

    #[test]
    fn increments_object_versions() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        let first = store
            .put_object(MemoryObject::new("decisions", "versioned", "{}"))
            .unwrap();
        let second = store
            .put_object(MemoryObject::new("decisions", "versioned", "{\"v\":2}"))
            .unwrap();

        assert_eq!(first.version, 1);
        assert_eq!(second.version, 2);
    }

    #[test]
    fn lists_objects_with_optional_collection_filter() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        store
            .put_object(MemoryObject::new("decisions", "sqlite-backend", "{}"))
            .unwrap();
        store
            .put_object(MemoryObject::new("notes", "agent-guide", "{}"))
            .unwrap();

        let filtered = store
            .list_objects(
                Some(&["decisions".to_string()]),
                &ListObjectsOptions::default(),
            )
            .unwrap();

        assert_eq!(
            store
                .list_objects(None, &ListObjectsOptions::default())
                .unwrap()
                .len(),
            2
        );
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].key.collection, "decisions");
    }

    #[test]
    fn stores_events_across_reopen() {
        let file = NamedTempFile::new().unwrap();

        {
            let mut store = SqliteThingStore::open(file.path()).unwrap();
            let event = store
                .append_event(MemoryEvent::new(
                    "project:thingd",
                    "decision.made",
                    "Use SQLite first",
                ))
                .unwrap();

            assert_eq!(event.sequence, 1);
        }

        let store = SqliteThingStore::open(file.path()).unwrap();
        let events = store
            .list_events(Some("project:thingd"), ListEventsOptions::default())
            .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "decision.made");
        assert_eq!(events[0].sequence, 1);
    }

    #[test]
    fn stores_queue_jobs_across_reopen() {
        let file = NamedTempFile::new().unwrap();

        {
            let mut store = SqliteThingStore::open(file.path()).unwrap();
            let job = store
                .push_job(QueueJob::new("embed", "job-1", "{\"doc\":\"doc-1\"}", 3))
                .unwrap();

            assert_eq!(job.status, QueueJobStatus::Ready);
        }

        let store = SqliteThingStore::open(file.path()).unwrap();
        let jobs = store.list_jobs("embed").unwrap();

        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, "job-1");
        assert_eq!(jobs[0].status, QueueJobStatus::Ready);
    }

    #[test]
    fn returns_existing_queue_job_for_duplicate_push() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        let first = store
            .push_job(QueueJob::new("embed", "job-1", "{\"doc\":\"doc-1\"}", 3))
            .unwrap();
        let second = store
            .push_job(QueueJob::new("embed", "job-1", "{\"doc\":\"doc-2\"}", 3))
            .unwrap();

        assert_eq!(first.body, "{\"doc\":\"doc-1\"}");
        assert_eq!(second.body, first.body);
        assert_eq!(store.list_jobs("embed").unwrap().len(), 1);
    }

    #[test]
    fn claims_and_acks_queue_jobs() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        store
            .push_job(QueueJob::new("embed", "job-1", "{\"doc\":\"doc-1\"}", 3))
            .unwrap();

        let claimed = store.claim_job("embed").unwrap().unwrap();
        let acked = store.ack_job("embed", "job-1").unwrap().unwrap();

        assert_eq!(claimed.status, QueueJobStatus::Leased);
        assert_eq!(claimed.attempts, 1);
        assert!(claimed.leased_at_ms.is_some());
        assert!(claimed.lease_expires_at_ms.is_some());
        assert_eq!(acked.status, QueueJobStatus::Completed);
        assert!(acked.completed_at_ms.is_some());
        assert!(store.claim_job("embed").unwrap().is_none());
    }

    #[test]
    fn nacks_queue_jobs_to_retry_then_dead_letter() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        store
            .push_job(QueueJob::new("embed", "job-1", "{\"doc\":\"doc-1\"}", 2))
            .unwrap();

        store.claim_job("embed").unwrap().unwrap();
        let retried = store.nack_job("embed", "job-1").unwrap().unwrap();
        assert_eq!(retried.status, QueueJobStatus::Ready);
        assert_eq!(retried.attempts, 1);

        store.claim_job("embed").unwrap().unwrap();
        let dead = store.nack_job("embed", "job-1").unwrap().unwrap();
        assert_eq!(dead.status, QueueJobStatus::Dead);
        assert_eq!(dead.attempts, 2);
        assert!(dead.dead_at_ms.is_some());
        assert_eq!(store.list_dead_jobs("embed").unwrap().len(), 1);
    }

    #[test]
    fn does_not_claim_delayed_queue_jobs_before_available() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        store
            .push_job(QueueJob::new("embed", "job-1", "{\"doc\":\"doc-1\"}", 3).delay_by_ms(60_000))
            .unwrap();

        assert!(store.claim_job("embed").unwrap().is_none());
    }

    #[test]
    fn reclaims_queue_jobs_after_lease_expiration() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        store
            .push_job(QueueJob::new("embed", "job-1", "{\"doc\":\"doc-1\"}", 3))
            .unwrap();

        let first = store
            .claim_job_with_options("embed", QueueClaimOptions::new(0))
            .unwrap()
            .unwrap();
        let second = store.claim_job("embed").unwrap().unwrap();

        assert_eq!(first.status, QueueJobStatus::Leased);
        assert_eq!(second.status, QueueJobStatus::Leased);
        assert_eq!(second.attempts, 2);
    }

    #[test]
    fn nacks_queue_jobs_with_retry_delay() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        store
            .push_job(QueueJob::new("embed", "job-1", "{\"doc\":\"doc-1\"}", 3))
            .unwrap();

        store.claim_job("embed").unwrap().unwrap();
        let retried = store
            .nack_job_with_options("embed", "job-1", QueueNackOptions::new(60_000))
            .unwrap()
            .unwrap();

        assert_eq!(retried.status, QueueJobStatus::Ready);
        assert!(store.claim_job("embed").unwrap().is_none());
    }

    #[test]
    fn persists_completed_queue_jobs_across_reopen() {
        let file = NamedTempFile::new().unwrap();

        {
            let mut store = SqliteThingStore::open(file.path()).unwrap();
            store
                .push_job(QueueJob::new("embed", "job-1", "{\"doc\":\"doc-1\"}", 3))
                .unwrap();
            store.claim_job("embed").unwrap().unwrap();
            store.ack_job("embed", "job-1").unwrap().unwrap();
        }

        let store = SqliteThingStore::open(file.path()).unwrap();
        let jobs = store.list_jobs("embed").unwrap();

        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, QueueJobStatus::Completed);
        assert_eq!(jobs[0].attempts, 1);
    }

    #[test]
    fn test_fts5_search_indexing_and_stemming() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        // Put objects
        store
            .put_object(MemoryObject::new(
                "decisions",
                "choice-1",
                "{\"text\":\"I choose this implementation plan because it has great benefits.\", \"status\":\"active\", \"priority\":1}",
            ))
            .unwrap();

        store
            .put_object(MemoryObject::new(
                "decisions",
                "choice-2",
                "{\"text\":\"He chooses that plan.\", \"status\":\"draft\", \"priority\":2}",
            ))
            .unwrap();

        // 1. Basic word match
        let results = store
            .search("implementation", crate::SearchOptions::default())
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "choice-1");

        // 2. Stemming test (choose / choosing / choice / chooses should stem to same root)
        let results_stem = store
            .search("choosing", crate::SearchOptions::default())
            .unwrap();
        assert_eq!(results_stem.len(), 2);

        // 3. Collection filtering
        let options_col = crate::SearchOptions {
            collections: Some(vec!["unrelated_col".to_string()]),
            ..Default::default()
        };
        let results_col = store.search("choose", options_col).unwrap();
        assert_eq!(results_col.len(), 0);

        // 4. Metadata filtering - status = "active"
        let options_filter = crate::SearchOptions {
            filter: Some(serde_json::json!({"status": "active"})),
            ..Default::default()
        };
        let results_filter = store.search("choose", options_filter).unwrap();
        assert_eq!(results_filter.len(), 1);
        assert_eq!(results_filter[0].id, "choice-1");

        // 5. Deletion test
        store.delete_object("decisions", "choice-1").unwrap();
        let results_after_del = store
            .search("choose", crate::SearchOptions::default())
            .unwrap();
        assert_eq!(results_after_del.len(), 1);
        assert_eq!(results_after_del[0].id, "choice-2");
    }

    #[test]
    fn search_consistent_after_batch_delete() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        store
            .put_object(MemoryObject::new(
                "col",
                "a",
                r#"{"label":"target","name":"alpha"}"#,
            ))
            .unwrap();
        store
            .put_object(MemoryObject::new(
                "col",
                "b",
                r#"{"label":"target","name":"bravo"}"#,
            ))
            .unwrap();
        store
            .put_object(MemoryObject::new(
                "col",
                "c",
                r#"{"label":"target","name":"charlie"}"#,
            ))
            .unwrap();

        let results = store
            .search("target", crate::SearchOptions::default())
            .unwrap();
        assert_eq!(results.len(), 3);

        store
            .delete_objects_batch(&[("col".into(), "a".into()), ("col".into(), "b".into())])
            .unwrap();

        let results = store
            .search("target", crate::SearchOptions::default())
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "c");
    }

    #[test]
    fn search_does_not_return_orphaned_fts_entries() {
        let store = SqliteThingStore::open_in_memory().unwrap();

        store
            .connection
            .execute(
                "INSERT INTO search_index (collection, id, kind, text) VALUES (?1, ?2, 'object', ?3)",
                rusqlite::params!["orphan", "ghost", "this object does not exist"],
            )
            .unwrap();

        let results = store
            .search("object", crate::SearchOptions::default())
            .unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn get_and_search_consistent_after_delete() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        store
            .put_object(MemoryObject::new("col", "id-1", r#"{"name":"alice"}"#))
            .unwrap();

        assert!(store.get_object("col", "id-1").unwrap().is_some());

        let results = store
            .search("alice", crate::SearchOptions::default())
            .unwrap();
        assert_eq!(results.len(), 1);

        store.delete_object("col", "id-1").unwrap();

        assert!(store.get_object("col", "id-1").unwrap().is_none());

        let results = store
            .search("alice", crate::SearchOptions::default())
            .unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn counts_objects_correctly_after_deletions() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        assert_eq!(store.count_objects().unwrap(), 0);

        store
            .put_object(MemoryObject::new("col1", "a", "{}"))
            .unwrap();
        store
            .put_object(MemoryObject::new("col1", "b", "{}"))
            .unwrap();
        store
            .put_object(MemoryObject::new("col2", "c", "{}"))
            .unwrap();
        assert_eq!(store.count_objects().unwrap(), 3);

        store.delete_object("col1", "a").unwrap();
        assert_eq!(store.count_objects().unwrap(), 2);

        store.delete_object("col1", "b").unwrap();
        assert_eq!(store.count_objects().unwrap(), 1);

        store.delete_object("col2", "c").unwrap();
        assert_eq!(store.count_objects().unwrap(), 0);
    }

    #[test]
    fn counts_events_correctly() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        assert_eq!(store.count_events().unwrap(), 0);

        store
            .append_event(MemoryEvent::new("test", "a", ""))
            .unwrap();
        store
            .append_event(MemoryEvent::new("test", "b", ""))
            .unwrap();
        assert_eq!(store.count_events().unwrap(), 2);
    }

    #[test]
    fn deletes_last_event_from_stream() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        store
            .append_event(MemoryEvent::new("match:1", "turn.recorded", "{}"))
            .unwrap();
        store
            .append_event(MemoryEvent::new("match:1", "turn.recorded", "{}"))
            .unwrap();
        store
            .append_event(MemoryEvent::new("match:2", "turn.recorded", "{}"))
            .unwrap();

        let deleted = store.delete_last_event("match:1").unwrap().unwrap();
        assert_eq!(deleted.sequence, 2);

        let remaining = store
            .list_events(Some("match:1"), ListEventsOptions::default())
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].sequence, 1);

        // match:2 unaffected
        let match2 = store
            .list_events(Some("match:2"), ListEventsOptions::default())
            .unwrap();
        assert_eq!(match2.len(), 1);
    }

    #[test]
    fn returns_none_when_delete_last_event_on_empty_stream() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();
        assert!(store.delete_last_event("nonexistent").unwrap().is_none());
    }

    #[test]
    fn deletes_stream_and_returns_count() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        store
            .append_event(MemoryEvent::new("match:1", "turn.recorded", "{}"))
            .unwrap();
        store
            .append_event(MemoryEvent::new("match:1", "turn.recorded", "{}"))
            .unwrap();
        store
            .append_event(MemoryEvent::new("match:2", "turn.recorded", "{}"))
            .unwrap();

        let count = store.delete_stream("match:1").unwrap();
        assert_eq!(count, 2);

        let remaining = store
            .list_events(Some("match:1"), ListEventsOptions::default())
            .unwrap();
        assert_eq!(remaining.len(), 0);

        // match:2 unaffected
        let match2 = store
            .list_events(Some("match:2"), ListEventsOptions::default())
            .unwrap();
        assert_eq!(match2.len(), 1);
    }

    #[test]
    fn returns_zero_for_delete_stream_on_empty_stream() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();
        assert_eq!(store.delete_stream("nonexistent").unwrap(), 0);
    }

    #[test]
    fn counts_jobs_correctly() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        assert_eq!(store.count_active_jobs().unwrap(), 0);
        assert_eq!(store.count_dead_jobs().unwrap(), 0);

        store
            .push_job(QueueJob::new("work", "j1", "p1", 3))
            .unwrap();
        store
            .push_job(QueueJob::new("work", "j2", "p2", 3))
            .unwrap();
        store
            .push_job(QueueJob::new("other", "j3", "p3", 1))
            .unwrap();
        assert_eq!(store.count_active_jobs().unwrap(), 3);

        store.claim_job("other").unwrap();
        store.nack_job("other", "j3").unwrap();
        assert_eq!(store.count_dead_jobs().unwrap(), 1);
        assert_eq!(store.count_active_jobs().unwrap(), 2);
    }

    #[test]
    fn lists_collections_streams_and_queues() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        assert!(store.list_collections().unwrap().is_empty());
        assert!(store.list_streams().unwrap().is_empty());
        assert!(store.list_queues().unwrap().is_empty());

        store
            .put_object(MemoryObject::new("col-a", "x", "{}"))
            .unwrap();
        store
            .put_object(MemoryObject::new("col-b", "y", "{}"))
            .unwrap();
        store
            .put_object(MemoryObject::new("col-a", "z", "{}"))
            .unwrap();
        let collections = store.list_collections().unwrap();
        assert_eq!(collections, vec!["col-a", "col-b"]);

        store
            .append_event(MemoryEvent::new("s1", "t", "e1"))
            .unwrap();
        store
            .append_event(MemoryEvent::new("s2", "t", "e2"))
            .unwrap();
        let streams = store.list_streams().unwrap();
        assert_eq!(streams, vec!["s1", "s2"]);

        store
            .push_job(QueueJob::new("work", "j1", "p1", 3))
            .unwrap();
        store
            .push_job(QueueJob::new("jobs", "j2", "p2", 3))
            .unwrap();
        let queues = store.list_queues().unwrap();
        assert_eq!(queues, vec!["jobs", "work"]);
    }

    #[test]
    fn search_respects_filter_and_limit() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        store
            .put_object(MemoryObject::new(
                "docs",
                "a",
                r#"{"text":"hello world","tag":"greeting"}"#,
            ))
            .unwrap();
        store
            .put_object(MemoryObject::new(
                "docs",
                "b",
                r#"{"text":"hello there","tag":"greeting"}"#,
            ))
            .unwrap();
        store
            .put_object(MemoryObject::new(
                "docs",
                "c",
                r#"{"text":"goodbye world","tag":"farewell"}"#,
            ))
            .unwrap();

        let all = store.search("world", SearchOptions::default()).unwrap();
        assert_eq!(all.len(), 2);

        let limited = store
            .search(
                "world",
                SearchOptions {
                    limit: Some(1),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(limited.len(), 1);

        let filtered = store
            .search(
                "hello",
                SearchOptions {
                    collections: Some(vec!["docs".into()]),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(filtered.len(), 2);
    }

    // ── list_objects: filter / limit / offset ─────────────────────────────

    #[test]
    fn list_objects_filter_returns_matching_objects() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        store
            .put_object(MemoryObject::new("w", "a", r#"{"color":"red","size":1}"#))
            .unwrap();
        store
            .put_object(MemoryObject::new("w", "b", r#"{"color":"blue","size":2}"#))
            .unwrap();
        store
            .put_object(MemoryObject::new("w", "c", r#"{"color":"red","size":3}"#))
            .unwrap();

        let opts = ListObjectsOptions {
            filter: vec![("color".into(), serde_json::json!("red"))],
            ..Default::default()
        };
        let results = store.list_objects(Some(&["w".to_string()]), &opts).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|o| o.body.contains("\"red\"")));
    }

    #[test]
    fn list_objects_filter_no_match_returns_empty() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        store
            .put_object(MemoryObject::new("w", "a", r#"{"color":"red"}"#))
            .unwrap();

        let opts = ListObjectsOptions {
            filter: vec![("color".into(), serde_json::json!("green"))],
            ..Default::default()
        };
        let results = store.list_objects(Some(&["w".to_string()]), &opts).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn list_objects_limit_truncates_results() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        for i in 0..5u32 {
            store
                .put_object(MemoryObject::new("col", format!("id-{i}"), "{}"))
                .unwrap();
        }

        let opts = ListObjectsOptions {
            limit: Some(3),
            ..Default::default()
        };
        let results = store
            .list_objects(Some(&["col".to_string()]), &opts)
            .unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn list_objects_offset_skips_results() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        for i in 0..5u32 {
            store
                .put_object(MemoryObject::new("col", format!("id-{i}"), "{}"))
                .unwrap();
        }

        let opts = ListObjectsOptions {
            offset: Some(3),
            ..Default::default()
        };
        let results = store
            .list_objects(Some(&["col".to_string()]), &opts)
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn list_objects_filter_and_limit_combined() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        for i in 0..4u32 {
            store
                .put_object(MemoryObject::new(
                    "col",
                    format!("id-{i}"),
                    r#"{"status":"active"}"#,
                ))
                .unwrap();
        }
        store
            .put_object(MemoryObject::new("col", "id-4", r#"{"status":"inactive"}"#))
            .unwrap();

        let opts = ListObjectsOptions {
            filter: vec![("status".into(), serde_json::json!("active"))],
            limit: Some(2),
            ..Default::default()
        };
        let results = store
            .list_objects(Some(&["col".to_string()]), &opts)
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|o| o.body.contains("active")));
    }

    #[test]
    fn list_objects_numeric_filter() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        store
            .put_object(MemoryObject::new("items", "a", r#"{"score":10,"tag":"x"}"#))
            .unwrap();
        store
            .put_object(MemoryObject::new("items", "b", r#"{"score":20,"tag":"x"}"#))
            .unwrap();
        store
            .put_object(MemoryObject::new("items", "c", r#"{"score":10,"tag":"y"}"#))
            .unwrap();

        let opts = ListObjectsOptions {
            filter: vec![("score".into(), serde_json::json!(10))],
            ..Default::default()
        };
        let results = store
            .list_objects(Some(&["items".to_string()]), &opts)
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    // ── append_event: RETURNING gives correct sequence + created_at ────────

    #[test]
    fn append_event_returning_sets_sequence_and_timestamp() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        let first = store
            .append_event(MemoryEvent::new("s", "ev.first", r#"{"x":1}"#))
            .unwrap();
        let second = store
            .append_event(MemoryEvent::new("s", "ev.second", r#"{"x":2}"#))
            .unwrap();

        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 2);
        assert!(
            !first.created_at.is_empty(),
            "created_at must be set by RETURNING"
        );
        assert!(
            !second.created_at.is_empty(),
            "created_at must be set by RETURNING"
        );
    }

    #[test]
    fn append_event_sequence_monotonically_increases_across_streams() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        let a = store
            .append_event(MemoryEvent::new("stream-a", "t", "{}"))
            .unwrap();
        let b = store
            .append_event(MemoryEvent::new("stream-b", "t", "{}"))
            .unwrap();
        let c = store
            .append_event(MemoryEvent::new("stream-a", "t", "{}"))
            .unwrap();

        assert_eq!(a.sequence, 1);
        assert_eq!(b.sequence, 2);
        assert_eq!(c.sequence, 3);
    }

    // ── optimistic locking / CAS ──────────────────────────────────────

    #[test]
    fn cas_succeeds_on_matching_version() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        let stored = store
            .put_object(MemoryObject::new("col", "id", r#"{"v":1}"#))
            .unwrap();
        assert_eq!(stored.version, 1);

        let opts = crate::PutObjectOptions {
            expected_version: Some(1),
            ..Default::default()
        };
        let updated = store
            .put_object_with_options(MemoryObject::new("col", "id", r#"{"v":2}"#), opts)
            .unwrap();
        assert_eq!(updated.version, 2);
    }

    #[test]
    fn cas_fails_on_version_mismatch() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        store
            .put_object(MemoryObject::new("col", "id", r#"{"v":1}"#))
            .unwrap();

        let opts = crate::PutObjectOptions {
            expected_version: Some(42),
            ..Default::default()
        };
        let err = store
            .put_object_with_options(MemoryObject::new("col", "id", r#"{"v":2}"#), opts)
            .unwrap_err();
        assert!(matches!(err, crate::ThingdError::Conflict(_)));
    }

    #[test]
    fn cas_fails_on_nonexistent_object() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        let opts = crate::PutObjectOptions {
            expected_version: Some(1),
            ..Default::default()
        };
        let err = store
            .put_object_with_options(MemoryObject::new("col", "id", r#"{"v":1}"#), opts)
            .unwrap_err();
        assert!(matches!(err, crate::ThingdError::Conflict(_)));
    }

    #[test]
    fn cas_none_skips_check() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        let stored = store
            .put_object_with_options(
                MemoryObject::new("col", "id", r#"{"v":1}"#),
                crate::PutObjectOptions::default(),
            )
            .unwrap();
        assert_eq!(stored.version, 1);
    }

    // ── batch atomicity ───────────────────────────────────────────────

    #[test]
    fn put_objects_batch_atomicity() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        // Insert 2 objects, then batch-insert a mix of new and existing keys
        store
            .put_object(MemoryObject::new("col", "a", r#"{"name":"old-a"}"#))
            .unwrap();
        store
            .put_object(MemoryObject::new("col", "b", r#"{"name":"old-b"}"#))
            .unwrap();

        // Batch upsert: update 'a', insert 'c' — both should succeed
        let results = store
            .put_objects_batch(vec![
                MemoryObject::new("col", "a", r#"{"name":"new-a"}"#),
                MemoryObject::new("col", "c", r#"{"name":"new-c"}"#),
            ])
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].version, 2); // 'a' updated
        assert_eq!(results[1].version, 1); // 'c' new

        // Verify both were persisted
        assert_eq!(store.count_objects().unwrap(), 3);
    }

    #[test]
    fn delete_objects_batch_atomicity() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        for c in ["a", "b", "c"] {
            store
                .put_object(MemoryObject::new("col", c, r#"{"name":"x"}"#))
                .unwrap();
        }

        assert_eq!(store.count_objects().unwrap(), 3);

        let deleted = store
            .delete_objects_batch(&[("col".into(), "a".into()), ("col".into(), "b".into())])
            .unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(store.count_objects().unwrap(), 1);
        assert!(store.get_object("col", "a").unwrap().is_none());
        assert!(store.get_object("col", "c").unwrap().is_some());
    }

    #[test]
    fn batch_operations_empty_input() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        let results = store.put_objects_batch(vec![]).unwrap();
        assert!(results.is_empty());

        let deleted = store.delete_objects_batch(&[]).unwrap();
        assert_eq!(deleted, 0);
    }

    // ── event idempotency ─────────────────────────────────────────

    #[test]
    fn event_idempotency_returns_existing_event() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        let mut event = MemoryEvent::new("stream", "test", r#"{"key":"val"}"#);
        event.idempotency_key = "idem-1".to_string();

        let first = store.append_event(event.clone()).unwrap();
        assert_eq!(first.sequence, 1);

        let second = store.append_event(event).unwrap();
        assert_eq!(second.sequence, first.sequence);
        assert_eq!(second.body, first.body);
    }

    #[test]
    fn event_idempotency_different_keys_are_distinct() {
        let mut store = SqliteThingStore::open_in_memory().unwrap();

        let mut event_a = MemoryEvent::new("stream", "test", r#"{"key":"a"}"#);
        event_a.idempotency_key = "idem-a".to_string();

        let mut event_b = MemoryEvent::new("stream", "test", r#"{"key":"b"}"#);
        event_b.idempotency_key = "idem-b".to_string();

        let first = store.append_event(event_a).unwrap();
        let second = store.append_event(event_b).unwrap();
        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 2);
    }
}
