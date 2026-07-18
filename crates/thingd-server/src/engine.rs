use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
#[cfg(feature = "migrate")]
use thingd::SqliteThingStore;
use thingd::{FjallEngine, MemoryEngine, ThingStore};

pub type SharedEngine = Arc<Mutex<Box<dyn ThingStore + Send>>>;

pub fn create_engine(
    db_path: &str,
) -> Result<Box<dyn ThingStore + Send>, Box<dyn std::error::Error>> {
    if db_path == ":memory:" || db_path.is_empty() {
        return Ok(Box::new(MemoryEngine::new()));
    }

    let fjall_path = db_path.trim_end_matches(".sqlite").trim_end_matches(".db");

    // Auto-migrate from SQLite if old file exists and Fjall doesn't
    #[cfg(feature = "migrate")]
    if !Path::new(fjall_path).exists()
        && let Some(sqlite_path) = find_old_sqlite(fjall_path)
    {
        tracing::info!("thingd: migrating SQLite → Fjall: {sqlite_path} → {fjall_path}");
        let result = migrate_from_sqlite(&sqlite_path, fjall_path);
        match result {
            Ok(engine) => {
                tracing::info!("thingd: migration complete. SQLite file retained as backup.");
                return Ok(engine);
            },
            Err(e) => {
                tracing::error!("thingd: migration FAILED: {e}. Starting fresh.");
            },
        }
    }

    match FjallEngine::open(fjall_path) {
        Ok(engine) => Ok(Box::new(engine)),
        Err(e) => {
            tracing::warn!("Failed to open Fjall at {fjall_path}: {e}. Falling back to memory.");
            Ok(Box::new(MemoryEngine::new()))
        },
    }
}

/// Find an old SQLite database file at or near the given path.
#[cfg(feature = "migrate")]
fn find_old_sqlite(fjall_path: &str) -> Option<String> {
    let candidates = vec![format!("{fjall_path}.sqlite"), format!("{fjall_path}.db")];
    for candidate in &candidates {
        if Path::new(candidate).exists() {
            return Some(candidate.clone());
        }
    }
    None
}

/// Read all data from a SQLite file and write it to a new Fjall database.
#[cfg(feature = "migrate")]
fn migrate_from_sqlite(
    sqlite_path: &str,
    fjall_path: &str,
) -> Result<Box<dyn ThingStore + Send>, Box<dyn std::error::Error>> {
    use thingd::{EventLog, LinkStore, ObjectStore, QueueStore};
    use thingd::{ListEventsOptions, ListObjectsOptions};

    let source = SqliteThingStore::open(sqlite_path)?;
    let mut dest = FjallEngine::open(fjall_path)?;

    // Migrate objects
    let collections = source.list_collections()?;
    for collection in &collections {
        let mut offset = 0u64;
        loop {
            let batch = source.list_objects(
                Some(std::slice::from_ref(collection)),
                &ListObjectsOptions {
                    limit: Some(100),
                    offset: Some(offset),
                    ..Default::default()
                },
            )?;
            if batch.is_empty() {
                break;
            }
            dest.put_objects_batch(batch)?;
            offset += 100;
        }
        tracing::info!("  migrated collection '{collection}': {offset} objects");
    }

    // Migrate events
    let streams = source.list_streams()?;
    for stream in &streams {
        let mut seq = 0u64;
        loop {
            let batch = source.list_events(
                Some(stream),
                ListEventsOptions {
                    from_sequence: Some(seq),
                    limit: Some(100),
                    ..Default::default()
                },
            )?;
            if batch.is_empty() {
                break;
            }
            seq = batch.last().map(|e| e.sequence).unwrap_or(seq);
            dest.append_events_batch(batch)?;
        }
        tracing::info!("  migrated stream '{stream}': {seq} events");
    }

    // Migrate queues
    let queues = source.list_queues()?;
    for queue in &queues {
        let jobs = source.list_jobs(queue)?;
        if !jobs.is_empty() {
            dest.push_jobs_batch(jobs)?;
            tracing::info!("  migrated queue '{queue}': jobs");
        }
    }

    // Migrate links (skip — auto-generated IDs won't match)
    let link_count = source.count_links()?;
    if link_count > 0 {
        tracing::warn!("  skipped {link_count} links (auto-generated IDs). Recreate if needed.");
    }

    Ok(Box::new(dest))
}

pub struct EnginePool {
    writers: RwLock<HashMap<String, SharedEngine>>,
    default_path: String,
}

impl EnginePool {
    pub fn new(default_path: String) -> Self {
        Self {
            writers: RwLock::new(HashMap::new()),
            default_path,
        }
    }

    fn resolve_path(&self, db_path: &str) -> String {
        if db_path.is_empty() {
            self.default_path.clone()
        } else {
            db_path.to_string()
        }
    }

    /// Get or create the writer engine for a path.
    pub fn get_writer(&self, db_path: &str) -> SharedEngine {
        let path = self.resolve_path(db_path);

        // Fast path: check if writer already exists
        if let Some(guard) = self.writers.try_read()
            && let Some(engine) = guard.get(&path)
        {
            return engine.clone();
        }

        // Slow path: acquire write lock and create
        let mut guard = self.writers.write();
        if let Some(engine) = guard.get(&path) {
            return engine.clone();
        }

        let writer = match create_engine(&path) {
            Ok(engine) => Arc::new(Mutex::new(engine)),
            Err(e) => {
                tracing::error!(
                    "CRITICAL: Failed to open database at {path}: {e}. \
                     Falling back to in-memory storage. ALL DATA WILL BE LOST ON RESTART."
                );
                eprintln!(
                    "CRITICAL: Failed to open database at {path}: {e}. \
                     Falling back to in-memory storage. ALL DATA WILL BE LOST ON RESTART."
                );
                Arc::new(Mutex::new(
                    Box::new(MemoryEngine::new()) as Box<dyn ThingStore + Send>
                ))
            },
        };

        guard.insert(path.clone(), writer.clone());

        writer
    }

    /// Get a reader engine for a path. All readers share the writer (single-process engine).
    pub fn get_reader(&self, db_path: &str) -> SharedEngine {
        self.get_writer(db_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thingd::MemoryObject;

    fn setup_pool() -> EnginePool {
        EnginePool::new(":memory:".to_string())
    }

    #[tokio::test]
    async fn creates_in_memory_engine() {
        let mut engine = create_engine(":memory:").unwrap();
        let obj = MemoryObject::new("test", "1", r#"{"hello":"world"}"#);
        let stored = engine.put_object(obj).unwrap();
        assert_eq!(stored.key.id, "1");
        assert_eq!(stored.key.collection, "test");
    }

    #[tokio::test]
    async fn pool_creates_engine_on_first_get_writer() {
        let pool = setup_pool();
        let engine = pool.get_writer("");
        let obj = MemoryObject::new("test", "1", r#"{}"#);
        let stored = engine.lock().put_object(obj).unwrap();
        assert_eq!(stored.key.id, "1");
    }

    #[tokio::test]
    async fn pool_returns_same_writer_for_same_path() {
        let pool = setup_pool();
        let e1 = pool.get_writer("");
        let e2 = pool.get_writer("");
        assert!(Arc::ptr_eq(&e1, &e2));
    }

    #[tokio::test]
    async fn reader_and_writer_are_same_for_persistent_path() {
        let dir = std::env::temp_dir().join("thingd-test-engine-pool-rd");
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test.db").to_str().unwrap().to_string();

        let pool = EnginePool::new(db_path.clone());
        let writer = pool.get_writer(&db_path);
        let reader = pool.get_reader(&db_path);

        // With Fjall, reader and writer share the same engine
        assert!(Arc::ptr_eq(&writer, &reader));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn pool_returns_different_engines_for_different_paths() {
        let dir = std::env::temp_dir().join("thingd-test-engine-pool");
        let _ = std::fs::create_dir_all(&dir);
        let p1 = dir.join("a.db").to_str().unwrap().to_string();
        let p2 = dir.join("b.db").to_str().unwrap().to_string();
        let pool = EnginePool::new(p1.clone());
        {
            let e1 = pool.get_writer(&p1);
            let mut g1 = e1.lock();
            g1.put_object(MemoryObject::new("col", "id", r#"{}"#))
                .unwrap();
        }
        {
            let e2 = pool.get_writer(&p2);
            let g2 = e2.lock();
            let obj = g2.get_object("col", "id").unwrap();
            assert!(obj.is_none(), "different engines should not share data");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn pool_handles_empty_db_path_as_default() {
        let pool = setup_pool();
        let engine = pool.get_writer("");
        let stored = engine
            .lock()
            .put_object(MemoryObject::new("test", "1", r#"{}"#))
            .unwrap();
        assert_eq!(stored.key.collection, "test");
    }

    #[tokio::test]
    async fn concurrent_readers_share_writer() {
        let dir = std::env::temp_dir().join("thingd-test-concurrent-readers");
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test.db").to_str().unwrap().to_string();
        let pool = Arc::new(EnginePool::new(db_path.clone()));

        let writer = pool.get_writer(&db_path);
        {
            let mut g = writer.lock();
            for i in 0..100 {
                let obj = MemoryObject::new("test", format!("obj-{i}"), r#"{"v":1}"#);
                g.put_object(obj).unwrap();
            }
        }

        let mut handles = Vec::new();
        for t in 0..4 {
            let p = Arc::clone(&pool);
            let path = db_path.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..25 {
                    let id = format!("obj-{}", t * 25 + i);
                    let engine = p.get_reader(&path);
                    let guard = engine.lock();
                    let obj = guard.get_object("test", &id).unwrap();
                    assert!(obj.is_some(), "reader {t} failed to find {id}");
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn readers_share_writer() {
        let pool = setup_pool();
        let writer = pool.get_writer("");
        let reader = pool.get_reader("");
        assert!(Arc::ptr_eq(&writer, &reader));
    }
}
