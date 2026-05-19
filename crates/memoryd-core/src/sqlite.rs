//! `SQLite`-backed storage adapter.
//!
//! This adapter currently implements durable object and event storage. Queue
//! persistence is intentionally left for the next phase because queue leasing
//! needs careful transactional semantics.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::{
    EventLog, MemoryEvent, MemoryObject, MemorydError, MemorydResult, ObjectKey, ObjectStore,
    QueueJob, QueueStore,
};

/// `SQLite`-backed memory store.
pub struct SqliteMemoryStore {
    connection: Connection,
}

impl SqliteMemoryStore {
    /// Open a `SQLite` database file and initialize the schema.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` cannot open the path or initialize schema.
    pub fn open(path: impl AsRef<Path>) -> MemorydResult<Self> {
        let connection = Connection::open(path).map_err(MemorydError::from)?;
        let store = Self { connection };
        store.initialize()?;
        Ok(store)
    }

    /// Open an in-memory `SQLite` database and initialize the schema.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` cannot initialize schema.
    pub fn open_in_memory() -> MemorydResult<Self> {
        let connection = Connection::open_in_memory().map_err(MemorydError::from)?;
        let store = Self { connection };
        store.initialize()?;
        Ok(store)
    }

    fn initialize(&self) -> MemorydResult<()> {
        self.connection
            .execute_batch(
                r"
                PRAGMA journal_mode = WAL;
                PRAGMA foreign_keys = ON;

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
                ",
            )
            .map_err(MemorydError::from)?;

        Ok(())
    }
}

impl ObjectStore for SqliteMemoryStore {
    fn put_object(&mut self, mut object: MemoryObject) -> MemorydResult<MemoryObject> {
        let transaction = self.connection.transaction().map_err(MemorydError::from)?;
        let version = transaction
            .query_row(
                "SELECT version FROM objects WHERE collection = ?1 AND id = ?2",
                params![&object.key.collection, &object.key.id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(MemorydError::from)?
            .map_or(Ok::<u64, MemorydError>(1), |existing| {
                u64::try_from(existing)
                    .map(|existing| existing + 1)
                    .map_err(|error| MemorydError::Storage(error.to_string()))
            })?;

        object.version = version;
        let stored_version = i64::try_from(object.version)
            .map_err(|error| MemorydError::Storage(error.to_string()))?;

        transaction
            .execute(
                r"
                INSERT INTO objects (collection, id, body, version, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                ON CONFLICT(collection, id) DO UPDATE SET
                    body = excluded.body,
                    version = excluded.version,
                    updated_at = excluded.updated_at
                ",
                params![
                    &object.key.collection,
                    &object.key.id,
                    &object.body,
                    stored_version
                ],
            )
            .map_err(MemorydError::from)?;

        transaction.commit().map_err(MemorydError::from)?;

        Ok(object)
    }

    fn get_object(&self, collection: &str, id: &str) -> MemorydResult<Option<MemoryObject>> {
        self.connection
            .query_row(
                "SELECT collection, id, body, version FROM objects WHERE collection = ?1 AND id = ?2",
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
                    })
                },
            )
            .optional()
            .map_err(MemorydError::from)
    }

    fn delete_object(&mut self, collection: &str, id: &str) -> MemorydResult<bool> {
        let changed = self
            .connection
            .execute(
                "DELETE FROM objects WHERE collection = ?1 AND id = ?2",
                params![collection, id],
            )
            .map_err(MemorydError::from)?;

        Ok(changed > 0)
    }
}

impl EventLog for SqliteMemoryStore {
    fn append_event(&mut self, mut event: MemoryEvent) -> MemorydResult<MemoryEvent> {
        self.connection
            .execute(
                r"
                INSERT INTO events (stream, event_type, body, created_at)
                VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                ",
                params![&event.stream, &event.event_type, &event.body],
            )
            .map_err(MemorydError::from)?;

        event.sequence = u64::try_from(self.connection.last_insert_rowid())
            .map_err(|error| MemorydError::Storage(error.to_string()))?;

        Ok(event)
    }

    fn list_events(&self, stream: Option<&str>) -> MemorydResult<Vec<MemoryEvent>> {
        let mut events = Vec::new();

        if let Some(stream) = stream {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT stream, event_type, body, sequence FROM events WHERE stream = ?1 ORDER BY sequence",
                )
                .map_err(MemorydError::from)?;
            let rows = statement
                .query_map(params![stream], row_to_event)
                .map_err(MemorydError::from)?;

            for row in rows {
                events.push(row.map_err(MemorydError::from)?);
            }
        } else {
            let mut statement = self
                .connection
                .prepare("SELECT stream, event_type, body, sequence FROM events ORDER BY sequence")
                .map_err(MemorydError::from)?;
            let rows = statement
                .query_map([], row_to_event)
                .map_err(MemorydError::from)?;

            for row in rows {
                events.push(row.map_err(MemorydError::from)?);
            }
        }

        Ok(events)
    }
}

impl QueueStore for SqliteMemoryStore {
    fn push_job(&mut self, _job: QueueJob) -> MemorydResult<QueueJob> {
        Err(MemorydError::Storage(
            "SQLite queue storage is not implemented yet".to_owned(),
        ))
    }

    fn claim_job(&mut self, _queue: &str) -> MemorydResult<Option<QueueJob>> {
        Err(MemorydError::Storage(
            "SQLite queue storage is not implemented yet".to_owned(),
        ))
    }

    fn ack_job(&mut self, _queue: &str, _id: &str) -> MemorydResult<Option<QueueJob>> {
        Err(MemorydError::Storage(
            "SQLite queue storage is not implemented yet".to_owned(),
        ))
    }

    fn nack_job(&mut self, _queue: &str, _id: &str) -> MemorydResult<Option<QueueJob>> {
        Err(MemorydError::Storage(
            "SQLite queue storage is not implemented yet".to_owned(),
        ))
    }

    fn list_jobs(&self, _queue: &str) -> MemorydResult<Vec<QueueJob>> {
        Err(MemorydError::Storage(
            "SQLite queue storage is not implemented yet".to_owned(),
        ))
    }

    fn list_dead_jobs(&self, _queue: &str) -> MemorydResult<Vec<QueueJob>> {
        Err(MemorydError::Storage(
            "SQLite queue storage is not implemented yet".to_owned(),
        ))
    }
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
    })
}

#[cfg(test)]
mod tests {
    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn stores_objects_across_reopen() {
        let file = NamedTempFile::new().unwrap();

        {
            let mut store = SqliteMemoryStore::open(file.path()).unwrap();
            let object = store
                .put_object(MemoryObject::new(
                    "decisions",
                    "sqlite-backend",
                    "{\"text\":\"Use SQLite\"}",
                ))
                .unwrap();

            assert_eq!(object.version, 1);
        }

        let store = SqliteMemoryStore::open(file.path()).unwrap();
        let object = store
            .get_object("decisions", "sqlite-backend")
            .unwrap()
            .unwrap();

        assert_eq!(object.body, "{\"text\":\"Use SQLite\"}");
        assert_eq!(object.version, 1);
    }

    #[test]
    fn increments_object_versions() {
        let mut store = SqliteMemoryStore::open_in_memory().unwrap();

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
    fn stores_events_across_reopen() {
        let file = NamedTempFile::new().unwrap();

        {
            let mut store = SqliteMemoryStore::open(file.path()).unwrap();
            let event = store
                .append_event(MemoryEvent::new(
                    "project:memoryd",
                    "decision.made",
                    "Use SQLite first",
                ))
                .unwrap();

            assert_eq!(event.sequence, 1);
        }

        let store = SqliteMemoryStore::open(file.path()).unwrap();
        let events = store.list_events(Some("project:memoryd")).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "decision.made");
        assert_eq!(events[0].sequence, 1);
    }
}
