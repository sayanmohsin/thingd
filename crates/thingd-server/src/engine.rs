use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use thingd::{MemoryEngine, SqliteThingStore, ThingStore};

pub type SharedEngine = Arc<Mutex<Box<dyn ThingStore + Send>>>;

const DEFAULT_READER_COUNT: usize = 3;

pub fn create_engine(
    db_path: &str,
) -> Result<Box<dyn ThingStore + Send>, Box<dyn std::error::Error>> {
    if db_path == ":memory:" || db_path.is_empty() {
        let engine = MemoryEngine::new();
        return Ok(Box::new(engine));
    }

    let engine = SqliteThingStore::open(db_path)?;
    Ok(Box::new(engine))
}

pub struct EnginePool {
    writers: RwLock<HashMap<String, SharedEngine>>,
    reader_pools: RwLock<HashMap<String, Vec<SharedEngine>>>,
    default_path: String,
    has_fallback: parking_lot::Mutex<bool>,
    reader_count: usize,
    next_reader: AtomicUsize,
}

impl EnginePool {
    pub fn new(default_path: String) -> Self {
        Self::with_reader_count(default_path, DEFAULT_READER_COUNT)
    }

    pub fn with_reader_count(default_path: String, reader_count: usize) -> Self {
        Self {
            writers: RwLock::new(HashMap::new()),
            reader_pools: RwLock::new(HashMap::new()),
            default_path,
            has_fallback: parking_lot::Mutex::new(false),
            reader_count: reader_count.max(1),
            next_reader: AtomicUsize::new(0),
        }
    }

    #[expect(dead_code)]
    pub fn has_fallback(&self) -> bool {
        *self.has_fallback.lock()
    }

    fn resolve_path(&self, db_path: &str) -> String {
        if db_path.is_empty() { self.default_path.clone() } else { db_path.to_string() }
    }

    fn is_in_memory_path(path: &str) -> bool {
        path == ":memory:" || path.is_empty() || path.starts_with("/nonexistent")
    }

    /// Get the writer engine for a path. Creates the engine and reader pool lazily.
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

        let is_memory = Self::is_in_memory_path(&path);

        let (writer, readers) = match create_engine(&path) {
            Ok(engine) => {
                let writer = Arc::new(Mutex::new(engine));

                // Create reader pool (separate connections to the same file)
                let readers = if is_memory {
                    // For in-memory, readers share the writer (no concurrency gain)
                    vec![writer.clone()]
                } else {
                    let mut pool = Vec::with_capacity(self.reader_count);
                    for _ in 0..self.reader_count {
                        if let Ok(reader) = create_engine(&path) {
                            pool.push(Arc::new(Mutex::new(reader)));
                        }
                    }
                    if pool.is_empty() {
                        vec![writer.clone()]
                    } else {
                        pool
                    }
                };

                (writer, readers)
            },
            Err(e) => {
                tracing::error!(
                    "CRITICAL: Failed to open database at {path}: {e}. \
                     Falling back to in-memory storage. ALL DATA WILL BE LOST ON RESTART. \
                     This may be a permissions issue, corrupt database, or disk-full condition."
                );
                eprintln!(
                    "CRITICAL: Failed to open database at {path}: {e}. \
                     Falling back to in-memory storage. ALL DATA WILL BE LOST ON RESTART."
                );
                *self.has_fallback.lock() = true;
                let memory = MemoryEngine::new();
                let shared = Arc::new(Mutex::new(Box::new(memory) as Box<dyn ThingStore + Send>));
                (shared.clone(), vec![shared])
            },
        };

        guard.insert(path.clone(), writer.clone());
        self.reader_pools.write().insert(path, readers);

        writer
    }

    /// Get a reader engine for a path. Round-robins across the reader pool.
    pub fn get_reader(&self, db_path: &str) -> SharedEngine {
        let path = self.resolve_path(db_path);

        // Fast path: check if reader pool exists
        if let Some(guard) = self.reader_pools.try_read()
            && let Some(pool) = guard.get(&path)
            && !pool.is_empty()
        {
            let idx = self.next_reader.fetch_add(1, Ordering::Relaxed) % pool.len();
            return pool[idx].clone();
        }

        // Ensure writer is created (which creates reader pool)
        let writer = self.get_writer(&path);

        // Retry reader pool
        if let Some(guard) = self.reader_pools.try_read()
            && let Some(pool) = guard.get(&path)
            && !pool.is_empty()
        {
            let idx = self.next_reader.fetch_add(1, Ordering::Relaxed) % pool.len();
            return pool[idx].clone();
        }

        writer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thingd::MemoryObject;

    fn setup_pool() -> EnginePool {
        EnginePool::with_reader_count(":memory:".to_string(), 3)
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
    async fn reader_and_writer_are_different_for_sqlite_path() {
        let dir = std::env::temp_dir().join("thingd-test-engine-pool-rd");
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test.db").to_str().unwrap().to_string();

        let pool = EnginePool::with_reader_count(db_path.clone(), 3);
        let writer = pool.get_writer(&db_path);
        let reader = pool.get_reader(&db_path);

        // Write data through writer
        writer.lock().put_object(MemoryObject::new("col", "id", r#"{"v":1}"#)).unwrap();

        // Read it back through reader (separate connection)
        let obj = reader.lock().get_object("col", "id").unwrap();
        assert!(obj.is_some(), "reader should see data written by writer in WAL mode");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn pool_returns_different_engines_for_different_paths() {
        let dir = std::env::temp_dir().join("thingd-test-engine-pool");
        let _ = std::fs::create_dir_all(&dir);
        let p1 = dir.join("a.db").to_str().unwrap().to_string();
        let p2 = dir.join("b.db").to_str().unwrap().to_string();
        let pool = EnginePool::with_reader_count(p1.clone(), 3);
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
    async fn pool_falls_back_to_memory_on_sqlite_failure() {
        let pool = EnginePool::with_reader_count("/nonexistent/path/db.sqlite".to_string(), 3);
        let engine = pool.get_writer("");
        let obj = MemoryObject::new("test", "1", r#"{}"#);
        let result = engine.lock().put_object(obj);
        assert!(result.is_ok(), "fallback engine should accept operations");
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
    async fn concurrent_readers_do_not_interfere() {
        let dir = std::env::temp_dir().join("thingd-test-concurrent-readers");
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test.db").to_str().unwrap().to_string();
        let pool = Arc::new(EnginePool::with_reader_count(db_path.clone(), 4));

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
                    let guard = p.get_reader(&path).lock();
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
    async fn concurrent_read_does_not_block_other_reads() {
        let dir = std::env::temp_dir().join("thingd-test-rd-not-block");
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test.db").to_str().unwrap().to_string();
        let pool = Arc::new(EnginePool::with_reader_count(db_path.clone(), 4));

        let writer = pool.get_writer(&db_path);
        {
            let mut g = writer.lock();
            for i in 0..10 {
                let obj = MemoryObject::new("test", format!("obj-{i}"), r#"{}"#);
                g.put_object(obj).unwrap();
            }
        }

        let p1 = Arc::clone(&pool);
        let p2 = Arc::clone(&pool);
        let path1 = db_path.clone();
        let path2 = db_path.clone();

        let h1 = tokio::spawn(async move {
            let guard = p1.get_reader(&path1).lock();
            let obj = guard.get_object("test", "obj-0").unwrap();
            assert!(obj.is_some());
        });

        let h2 = tokio::spawn(async move {
            let guard = p2.get_reader(&path2).lock();
            let obj = guard.get_object("test", "obj-1").unwrap();
            assert!(obj.is_some());
        });

        h1.await.unwrap();
        h2.await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn readers_round_robin_across_pool() {
        let dir = std::env::temp_dir().join("thingd-test-round-robin");
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test.db").to_str().unwrap().to_string();
        let pool = EnginePool::with_reader_count(db_path.clone(), 3);

        // Force writer creation
        pool.get_writer(&db_path);

        let r1 = pool.get_reader(&db_path);
        let r2 = pool.get_reader(&db_path);
        let r3 = pool.get_reader(&db_path);
        let r4 = pool.get_reader(&db_path);

        // With 3 readers, r1 and r4 should have the same address (round-robin wraps)
        assert!(Arc::ptr_eq(&r1, &r4), "r1 and r4 should wrap to same reader");
        assert!(
            !Arc::ptr_eq(&r1, &r2),
            "r1 and r2 should be different readers"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
