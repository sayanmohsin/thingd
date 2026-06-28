use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::Arc;
use thingd::{MemoryEngine, SqliteThingStore, ThingStore};

pub type SharedEngine = Arc<Mutex<Box<dyn ThingStore + Send>>>;

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
    engines: RwLock<HashMap<String, SharedEngine>>,
    default_path: String,
    has_fallback: parking_lot::Mutex<bool>,
}

impl EnginePool {
    pub fn new(default_path: String) -> Self {
        Self {
            engines: RwLock::new(HashMap::new()),
            default_path,
            has_fallback: parking_lot::Mutex::new(false),
        }
    }

    #[expect(dead_code)]
    pub fn has_fallback(&self) -> bool {
        *self.has_fallback.lock()
    }

    /// Get the raw shared engine for a path.
    pub fn get(&self, db_path: &str) -> SharedEngine {
        let path = if db_path.is_empty() {
            &self.default_path
        } else {
            db_path
        };
        let path = path.to_string();

        // Check if already cached (read lock — concurrent pool access is safe)
        if let Some(guard) = self.engines.try_read()
            && let Some(engine) = guard.get(&path)
        {
            return engine.clone();
        }

        // Acquire write lock to create or retrieve
        let mut guard = self
            .engines
            .try_write()
            .unwrap_or_else(|| self.engines.write());

        if let Some(engine) = guard.get(&path) {
            return engine.clone();
        }

        match create_engine(&path) {
            Ok(engine) => {
                let shared = Arc::new(Mutex::new(engine));
                guard.insert(path, shared.clone());
                shared
            },
            Err(e) => {
                tracing::error!(
                    "CRITICAL: Failed to open database at {path}: {e}. \
                     Falling back to in-memory storage. ALL DATA WILL BE LOST ON RESTART. \
                     This may be a permissions issue, corrupt database, or disk-full condition."
                );
                *self.has_fallback.lock() = true;
                let memory = MemoryEngine::new();
                let shared = Arc::new(Mutex::new(Box::new(memory) as Box<dyn ThingStore + Send>));
                guard.insert(path, shared.clone());
                shared
            },
        }
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
    async fn pool_creates_engine_on_first_get() {
        let pool = setup_pool();
        let engine = pool.get("");
        let obj = MemoryObject::new("test", "1", r#"{}"#);
        let stored = engine.lock().put_object(obj).unwrap();
        assert_eq!(stored.key.id, "1");
    }

    #[tokio::test]
    async fn pool_returns_same_engine_for_same_path() {
        let pool = setup_pool();
        let e1 = pool.get("");
        let e2 = pool.get("");
        assert!(Arc::ptr_eq(&e1, &e2));
    }

    #[tokio::test]
    async fn pool_returns_different_engines_for_different_paths() {
        let dir = std::env::temp_dir().join("thingd-test-engine-pool");
        let _ = std::fs::create_dir_all(&dir);
        let p1 = dir.join("a.db").to_str().unwrap().to_string();
        let p2 = dir.join("b.db").to_str().unwrap().to_string();
        let pool = EnginePool::new(p1.clone());
        {
            let e1 = pool.get(&p1);
            let mut g1 = e1.lock();
            g1.put_object(MemoryObject::new("col", "id", r#"{}"#))
                .unwrap();
        }
        {
            let e2 = pool.get(&p2);
            let g2 = e2.lock();
            let obj = g2.get_object("col", "id").unwrap();
            assert!(obj.is_none(), "different engines should not share data");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn pool_falls_back_to_memory_on_sqlite_failure() {
        let pool = EnginePool::new("/nonexistent/path/db.sqlite".to_string());
        let engine = pool.get("");
        let obj = MemoryObject::new("test", "1", r#"{}"#);
        let result = engine.lock().put_object(obj);
        assert!(result.is_ok(), "fallback engine should accept operations");
    }

    #[tokio::test]
    async fn pool_handles_empty_db_path_as_default() {
        let pool = setup_pool();
        let engine = pool.get("");
        let stored = engine
            .lock()
            .put_object(MemoryObject::new("test", "1", r#"{}"#))
            .unwrap();
        assert_eq!(stored.key.collection, "test");
    }

    #[tokio::test]
    async fn concurrent_readers_do_not_interfere() {
        let pool = Arc::new(setup_pool());
        let engine = pool.get("");
        {
            let mut g = engine.lock();
            for i in 0..100 {
                let obj = MemoryObject::new("test", format!("obj-{i}"), r#"{"v":1}"#);
                g.put_object(obj).unwrap();
            }
        }

        let p = Arc::clone(&pool);
        let mut handles = Vec::new();
        for t in 0..4 {
            let p = Arc::clone(&p);
            handles.push(tokio::spawn(async move {
                let engine = p.get("");
                for i in 0..25 {
                    let id = format!("obj-{}", t * 25 + i);
                    let guard = engine.lock();
                    let obj = guard.get_object("test", &id).unwrap();
                    assert!(obj.is_some(), "reader {t} failed to find {id}");
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }
    }

    #[tokio::test]
    async fn concurrent_read_does_not_block_other_reads() {
        let pool = Arc::new(setup_pool());
        let engine = pool.get("");
        {
            let mut g = engine.lock();
            for i in 0..10 {
                let obj = MemoryObject::new("test", format!("obj-{i}"), r#"{}"#);
                g.put_object(obj).unwrap();
            }
        }

        let p1 = Arc::clone(&pool);
        let h1 = tokio::spawn(async move {
            let engine = p1.get("");
            let guard = engine.lock();
            let obj = guard.get_object("test", "obj-0").unwrap();
            assert!(obj.is_some());
        });

        let p2 = Arc::clone(&pool);
        let h2 = tokio::spawn(async move {
            let engine = p2.get("");
            let guard = engine.lock();
            let obj = guard.get_object("test", "obj-1").unwrap();
            assert!(obj.is_some());
        });

        h1.await.unwrap();
        h2.await.unwrap();
    }

    #[tokio::test]
    async fn read_and_write_do_not_deadlock() {
        let pool = Arc::new(setup_pool());
        let engine = pool.get("");
        {
            let mut g = engine.lock();
            g.put_object(MemoryObject::new("test", "shared", r#"{}"#))
                .unwrap();
        }

        let p_reader = Arc::clone(&pool);
        let reader = tokio::spawn(async move {
            let engine = p_reader.get("");
            let guard = engine.lock();
            let obj = guard.get_object("test", "shared").unwrap();
            drop(guard);
            assert!(obj.is_some());
        });

        let p_writer = Arc::clone(&pool);
        let writer = tokio::spawn(async move {
            let engine = p_writer.get("");
            let mut guard = engine.lock();
            let obj = MemoryObject::new("test", "new", r#"{}"#);
            guard.put_object(obj).unwrap();
        });

        reader.await.unwrap();
        writer.await.unwrap();
    }
}
