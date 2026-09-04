use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::{Mutex, RwLock};
use thingd::{
    EncryptionConfig, PersistentBackend, PersistentEngine, PersistentOpenOptions,
    PersistentSearchMode, ThingStore,
};

pub type SharedEngine = Arc<Mutex<Box<dyn ThingStore + Send>>>;

fn spawn_storage_recovery(engine: SharedEngine) {
    let _ = thread::Builder::new()
        .name("thingd-storage-recovery".to_string())
        .spawn(move || {
            let mut retry_delay = Duration::from_millis(100);
            let mut compacted = false;
            loop {
                let budget = engine
                    .try_lock()
                    .map(|guard| guard.recovery_budget())
                    .unwrap_or_default();
                if let Some(limit) = budget.memory_limit_bytes
                    && resident_memory_bytes().is_some_and(|used| used > limit)
                {
                    if let Some(mut guard) = engine.try_lock() {
                        guard.fail_storage_recovery(format!(
                            "recovery memory ceiling exceeded: {} bytes",
                            limit
                        ));
                    }
                    tracing::error!(limit, "storage recovery stopped at memory ceiling");
                    break;
                }
                let result = if let Some(mut guard) = engine.try_lock() {
                    let maintenance = guard.storage_maintenance_status();
                    if !guard.search_rebuild_required() && maintenance.state == "idle" {
                        drop(guard);
                        thread::sleep(Duration::from_millis(100));
                        continue;
                    }
                    if !compacted {
                        let result = guard.compact_storage();
                        if result.is_ok() {
                            compacted = true;
                        }
                        result.map(|_| false)
                    } else {
                        guard.search_rebuild_step(budget.batch_size.max(1))
                    }
                } else {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                };
                match result {
                    Ok(true) => {
                        retry_delay = Duration::from_millis(100);
                        thread::sleep(Duration::from_millis(100));
                    },
                    Ok(false) => {
                        let action = engine.try_lock().map(|mut guard| {
                            let status = guard.storage_maintenance_status();
                            match status.state.as_str() {
                                "degraded"
                                    if status.retry_count < budget.max_retries
                                        && guard.retry_search_rebuild() =>
                                {
                                    1_i8
                                },
                                "degraded" | "failed" => -1_i8,
                                _ => 0_i8,
                            }
                        });
                        if action == Some(-1) {
                            tracing::warn!("storage recovery stopped in degraded or failed state");
                            break;
                        }
                        thread::sleep(retry_delay);
                        retry_delay = (retry_delay * 2).min(Duration::from_secs(2));
                    },
                    Err(error) => {
                        tracing::error!(error = %error, "asynchronous search rebuild stopped");
                        break;
                    },
                }
                if budget.pause_ms > 0 {
                    thread::sleep(Duration::from_millis(budget.pause_ms));
                }
            }
        });
}

#[cfg(target_os = "linux")]
fn resident_memory_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let value = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?;
    value
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()
        .map(|kb| kb * 1024)
}

#[cfg(not(target_os = "linux"))]
fn resident_memory_bytes() -> Option<u64> {
    None
}

pub fn create_engine(
    db_path: &str,
    options: &PersistentOpenOptions,
) -> Result<Box<dyn ThingStore + Send>, Box<dyn std::error::Error>> {
    if db_path == ":memory:" || db_path.is_empty() {
        return Ok(Box::new(PersistentEngine::open_in_memory_with_backend(
            PersistentBackend::ThingDb,
        )?));
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

    /// Construct a pool with an optional encryption key and search mode.
    #[allow(dead_code)]
    pub fn new_with_encryption_key_and_search_mode(
        default_path: String,
        key: Option<&str>,
        search_mode: PersistentSearchMode,
    ) -> Result<Self, String> {
        Self::new_with_encryption_key_search_mode_and_journal_limit(
            default_path,
            key,
            search_mode,
            32 * 1024 * 1024,
        )
    }

    pub fn new_with_encryption_key_search_mode_and_journal_limit(
        default_path: String,
        key: Option<&str>,
        search_mode: PersistentSearchMode,
        max_journal_bytes: u64,
    ) -> Result<Self, String> {
        Self::new_with_encryption_key_search_mode_journal_limit_and_recovery_budget(
            default_path,
            key,
            search_mode,
            max_journal_bytes,
            32,
            50,
            3,
            None,
            250,
            32,
            10_000,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_encryption_key_search_mode_journal_limit_and_recovery_budget(
        default_path: String,
        key: Option<&str>,
        search_mode: PersistentSearchMode,
        max_journal_bytes: u64,
        recovery_batch_size: usize,
        recovery_pause_ms: u64,
        recovery_max_retries: u64,
        recovery_memory_limit_bytes: Option<u64>,
        search_commit_interval_ms: u64,
        search_commit_batch_size: usize,
        search_queue_max_keys: usize,
    ) -> Result<Self, String> {
        Self::new_with_encryption_key_search_mode_journal_limit_and_recovery_budget_and_backend(
            default_path,
            key,
            search_mode,
            max_journal_bytes,
            recovery_batch_size,
            recovery_pause_ms,
            recovery_max_retries,
            recovery_memory_limit_bytes,
            search_commit_interval_ms,
            search_commit_batch_size,
            search_queue_max_keys,
            PersistentBackend::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_encryption_key_search_mode_journal_limit_and_recovery_budget_and_backend(
        default_path: String,
        key: Option<&str>,
        search_mode: PersistentSearchMode,
        max_journal_bytes: u64,
        recovery_batch_size: usize,
        recovery_pause_ms: u64,
        recovery_max_retries: u64,
        recovery_memory_limit_bytes: Option<u64>,
        search_commit_interval_ms: u64,
        search_commit_batch_size: usize,
        search_queue_max_keys: usize,
        backend: PersistentBackend,
    ) -> Result<Self, String> {
        let encryption = key
            .map(parse_hex_key)
            .transpose()?
            .map(|bytes| EncryptionConfig::from_key(&bytes))
            .transpose()
            .map_err(|error| error.to_string())?;
        let mut pool = Self::new(default_path);
        pool.open_options = PersistentOpenOptions {
            backend,
            encryption,
            search_mode,
            max_journal_bytes,
            recovery_batch_size,
            recovery_pause_ms,
            recovery_max_retries,
            recovery_memory_limit_bytes,
            search_commit_interval_ms,
            search_commit_batch_size,
            search_queue_max_keys,
            ..PersistentOpenOptions::default()
        };
        // Validate and open the configured default database during startup. This
        // makes missing or wrong keys a startup error instead of a request-time
        // panic or an accidental fallback to memory storage.
        let default_path = pool.default_path.clone();
        let engine = create_engine(&default_path, &pool.open_options).map_err(|error| {
            format!("failed to open durable database at {default_path}: {error}")
        })?;
        let shared = Arc::new(Mutex::new(engine));
        let rebuild_required = shared.lock().search_rebuild_required();
        pool.writers.write().insert(default_path, shared.clone());
        if rebuild_required || shared.lock().storage_maintenance_status().state != "idle" {
            spawn_storage_recovery(shared);
        }
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
        match self.try_get_writer(db_path) {
            Ok(engine) => engine,
            Err(error) => panic!("failed to open durable database at {db_path}: {error}"),
        }
    }

    fn try_get_writer(&self, db_path: &str) -> Result<SharedEngine, String> {
        let path = self.resolve_path(db_path);

        // Fast path: check if writer already exists
        if let Some(guard) = self.writers.try_read()
            && let Some(engine) = guard.get(&path)
        {
            return Ok(engine.clone());
        }

        // Slow path: acquire write lock and create
        let mut guard = self.writers.write();
        if let Some(engine) = guard.get(&path) {
            return Ok(engine.clone());
        }

        let engine: Box<dyn ThingStore + Send> = match create_engine(&path, &self.open_options) {
            Ok(engine) => engine,
            Err(error) => {
                let message = error.to_string();
                let is_validation_error = message.contains("storage validation failed")
                    || message.contains("missing required lock file")
                    || message.contains("lock file");
                if is_validation_error && !path.is_empty() && path != ":memory:" {
                    // Validate tenant path is confined to the configured database prefix
                    // to avoid path traversal via user-provided tenant IDs (CodeQL js/path-injection).
                    let is_safe = {
                        if path.contains("..") || path.contains('\0') {
                            false
                        } else if let Some(parent) = Path::new(&self.default_path)
                            .parent()
                            .filter(|p| !p.as_os_str().is_empty())
                        {
                            Path::new(&path).starts_with(parent)
                        } else {
                            // No parent configured (e.g. in-memory fallback); allow only simple absolute paths
                            Path::new(&path).is_absolute()
                        }
                    };
                    if !is_safe {
                        return Err(format!(
                            "failed to open durable database at {path}: {message} (refused unsafe path)"
                        ));
                    }
                    let count = Path::new(&path)
                        .read_dir()
                        .map(|entries| entries.count())
                        .unwrap_or(usize::MAX);
                    let should_recreate = count <= 1;
                    if should_recreate {
                        tracing::warn!(
                            path,
                            error = %message,
                            "removing invalid tenant database directory and recreating"
                        );
                        let _ = std::fs::remove_dir_all(&path);
                        let _ = std::fs::remove_file(&path);
                        match create_engine(&path, &self.open_options) {
                            Ok(recreated) => {
                                let writer = Arc::new(Mutex::new(recreated));
                                guard.insert(path.clone(), writer.clone());
                                if writer.lock().search_rebuild_required()
                                    || writer.lock().storage_maintenance_status().state != "idle"
                                {
                                    spawn_storage_recovery(writer.clone());
                                }
                                return Ok(writer);
                            },
                            Err(retry_error) => {
                                return Err(format!(
                                    "failed to open durable database at {path}: {retry_error} (original: {message})"
                                ));
                            },
                        }
                    }
                }
                return Err(format!(
                    "failed to open durable database at {path}: {message}"
                ));
            },
        };

        let writer = Arc::new(Mutex::new(engine));
        guard.insert(path.clone(), writer.clone());

        if writer.lock().search_rebuild_required()
            || writer.lock().storage_maintenance_status().state != "idle"
        {
            spawn_storage_recovery(writer.clone());
        }

        Ok(writer)
    }

    /// Get a reader engine for a path. All readers share the writer (single-process engine).
    pub fn get_reader(&self, db_path: &str) -> SharedEngine {
        self.get_writer(db_path)
    }

    pub fn storage_maintenance_status(&self, db_path: &str) -> thingd::StorageMaintenanceStatus {
        self.get_reader(db_path).lock().storage_maintenance_status()
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
            // Delete any legacy single-file compatibility path
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
    use thingd::{EventLog, ListEventsOptions, MemoryEvent, MemoryObject, ObjectStore};

    fn setup_pool() -> EnginePool {
        EnginePool::new(":memory:".to_string())
    }

    #[tokio::test]
    async fn creates_in_memory_engine() {
        let mut engine = create_engine(":memory:", &PersistentOpenOptions::default()).unwrap();
        let diagnostics = engine.storage_diagnostics().unwrap();
        assert_eq!(diagnostics.journal_bytes, 0);
        assert_eq!(diagnostics.journal_count, 0);
        let obj = MemoryObject::new("test", "1", r#"{"hello":"world"}"#);
        let stored = engine.put_object(obj).unwrap();
        assert_eq!(stored.key.id, "1");
        assert_eq!(stored.key.collection, "test");
    }

    #[tokio::test]
    async fn embedded_store_validates_and_reopens_through_server_engine() {
        let dir = std::env::temp_dir().join(format!(
            "thingd-server-compat-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        {
            let mut embedded = PersistentEngine::open(&dir).unwrap();
            embedded
                .put_object(MemoryObject::new(
                    "compat",
                    "object",
                    r#"{"text":"compatibility-search-token"}"#,
                ))
                .unwrap();
            let mut event = MemoryEvent::new("compat", "created", r#"{"id":"object"}"#);
            event.idempotency_key = "compat-1".to_string();
            embedded.append_event(event).unwrap();
        }

        let validation = PersistentEngine::validate_path(&dir).unwrap();
        assert!(validation.lock_present);
        let standalone =
            create_engine(dir.to_str().unwrap(), &PersistentOpenOptions::default()).unwrap();
        assert!(standalone.get_object("compat", "object").unwrap().is_some());
        assert_eq!(
            standalone
                .list_events(Some("compat"), ListEventsOptions::default())
                .unwrap()
                .len(),
            1
        );
        assert!(
            !standalone
                .search("compatibility-search-token", Default::default())
                .unwrap()
                .is_empty()
        );
        drop(standalone);
        std::fs::remove_dir_all(dir).unwrap();
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
        let _ = std::fs::remove_dir_all(&dir);
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
        let _ = std::fs::remove_dir_all(&dir);
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
        let dir = std::env::temp_dir().join(format!(
            "thingd-test-concurrent-readers-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
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

    #[test]
    fn configured_pool_opens_database_during_startup() {
        let dir = std::env::temp_dir().join(format!(
            "thingd-server-encryption-startup-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("encrypted").to_string_lossy().to_string();
        let key = "11".repeat(32);
        let pool = EnginePool::new_with_encryption_key_and_search_mode(
            path.clone(),
            Some(&key),
            PersistentSearchMode::Persistent,
        )
        .unwrap();
        assert!(pool.writers.read().contains_key(&path));
        let missing = EnginePool::new_with_encryption_key_and_search_mode(
            path,
            None,
            PersistentSearchMode::Persistent,
        );
        assert!(missing.is_err());
        drop(pool);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn readers_share_writer() {
        let pool = setup_pool();
        let writer = pool.get_writer("");
        let reader = pool.get_reader("");
        assert!(Arc::ptr_eq(&writer, &reader));
    }
}
