use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thingd::{MemoryEngine, SqliteThingStore, ThingStore};
use tokio::sync::Mutex;

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
}

impl EnginePool {
    pub fn new(default_path: String) -> Self {
        Self {
            engines: RwLock::new(HashMap::new()),
            default_path,
        }
    }

    pub fn get(&self, db_path: &str) -> SharedEngine {
        let path = if db_path.is_empty() {
            &self.default_path
        } else {
            db_path
        };
        let path = path.to_string();

        if let Ok(Some(engine)) = self.engines.read().map(|g| g.get(&path).cloned()) {
            return engine;
        }

        if let Ok(mut guard) = self.engines.write() {
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
                    tracing::error!("Failed to create engine at {path}: {e}");
                    let memory = MemoryEngine::new();
                    let shared =
                        Arc::new(Mutex::new(Box::new(memory) as Box<dyn ThingStore + Send>));
                    guard.insert(path, shared.clone());
                    shared
                },
            }
        } else {
            tracing::error!("Engine pool lock poisoned, falling back to memory engine");
            let memory = MemoryEngine::new();
            Arc::new(Mutex::new(Box::new(memory) as Box<dyn ThingStore + Send>))
        }
    }
}
