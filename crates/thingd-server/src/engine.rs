use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use thingd::{EncryptionConfig, MemoryEngine, PersistentEngine, PersistentOpenOptions, ThingStore};

pub type SharedEngine = Arc<Mutex<Box<dyn ThingStore + Send>>>;

pub fn create_engine(
    db_path: &str,
    options: &PersistentOpenOptions,
) -> Result<Box<dyn ThingStore + Send>, Box<dyn std::error::Error>> {
    if db_path == ":memory:" || db_path.is_empty() {
        return Ok(Box::new(MemoryEngine::new()));
    }

    if let Some(parent) = Path::new(db_path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }

    match PersistentEngine::open_with_options(db_path, options.clone()) {
        Ok(engine) => Ok(Box::new(engine)),
        Err(e) => Err(Box::new(e)),
    }
}

pub struct EnginePool {
    writers: RwLock<HashMap<String, SharedEngine>>,
    default_path: String,
    open_options: PersistentOpenOptions,
}

impl EnginePool {
    pub fn new(default_path: String) -> Self {
        Self {
            writers: RwLock::new(HashMap::new()),
            default_path,
            open_options: PersistentOpenOptions::default(),
        }
    }

    /// Construct a pool with an optional 64-character hexadecimal encryption key.
    pub fn new_with_encryption_key(
        default_path: String,
        key: Option<&str>,
    ) -> Result<Self, String> {
        let encryption = key
            .map(parse_hex_key)
            .transpose()?
            .map(|bytes| EncryptionConfig::from_key(&bytes))
            .transpose()
            .map_err(|error| error.to_string())?;
        let mut pool = Self::new(default_path);
        pool.open_options = PersistentOpenOptions { encryption };
        Ok(pool)
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

        let writer = match create_engine(&path, &self.open_options) {
            Ok(engine) => Arc::new(Mutex::new(engine)),
            Err(e) => panic!("failed to open durable database at {path}: {e}"),
        };

        guard.insert(path.clone(), writer.clone());

        writer
    }

    /// Get a reader engine for a path. All readers share the writer (single-process engine).
    pub fn get_reader(&self, db_path: &str) -> SharedEngine {
        self.get_writer(db_path)
    }

    /// Remove the default engine from the pool and delete its database file.
    /// The next call to `get_reader("")` or `get_writer("")` will create a fresh
    /// empty engine at the default path. Old data is permanently lost.
    pub fn clear_default_engine(&self) -> Result<(), String> {
        let path = self.default_path.clone();

        let mut guard = self.writers.write();
        guard.remove(&path);

        // Also remove any empty-string entries (alias for default)
        guard.remove("");

        if path != ":memory:" && !path.is_empty() {
            // Delete the Persistent directory
            if let Err(e) = std::fs::remove_dir_all(&path)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                return Err(format!("Failed to delete database at {path}: {e}"));
            }
            // Delete legacy single-file SQLite compat path
            let _ = std::fs::remove_file(&path);
        }

        tracing::info!("Default database cleared: {path}");
        Ok(())
    }
}

fn parse_hex_key(value: &str) -> Result<Vec<u8>, String> {
    if value.len() != 64 {
        return Err("encryption key must contain 64 hexadecimal characters".to_string());
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| "encryption key must contain hexadecimal characters".to_string())
        })
        .collect()
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
        let mut engine = create_engine(":memory:", &PersistentOpenOptions::default()).unwrap();
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

        // With Persistent, reader and writer share the same engine
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
        let _ = std::fs::remove_dir_all(&dir);
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
