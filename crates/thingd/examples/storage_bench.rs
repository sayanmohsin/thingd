//! Local storage benchmark for thingd adapters.
//!
//! Usage:
//!   cargo run --example `storage_bench` --release --features persistent,search [<iterations>]
//!   `THINGD_BENCH_ITERS=10000` cargo run --example `storage_bench` --release --features persistent,search
//!   Add `--reliability` to run the correctness and isolation preflight.
//!   Add `--qualification` to run durable reopen, compaction, repack, and validation checks.

#![allow(unused_crate_dependencies)]

use std::env;
use std::error::Error;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::hint::black_box;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Barrier, Mutex,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

use serde::Serialize;
use thingd::{
    EncryptionConfig, EventLog, ListEventsOptions, ListObjectsOptions, MemoryEngine, MemoryEvent,
    MemoryObject, ObjectStore, PersistentEngine, PersistentOpenOptions, QueueClaimOptions,
    QueueJob, QueueStore, SearchOptions, Searcher, VectorSearchOptions, VectorStore,
};
use thingd::{Link, LinkStore};
use thingdb::{CacheOptions, MemoryCache};

const DEFAULT_ITERATIONS: usize = 5_000;
const DEFAULT_MEMTABLE_BYTES: u64 = 8 * 1024 * 1024;
const COLLECTION: &str = "bench_objects";
const QUEUE: &str = "bench_queue";
const STREAM: &str = "bench:events";

const OBJECT_BODY_ACTIVE: &str =
    r#"{"text":"benchmark object","project":"thingd","status":"active","confidence":0.95}"#;
const OBJECT_BODY_INACTIVE: &str =
    r#"{"text":"benchmark object","project":"thingd","status":"inactive","confidence":0.10}"#;
const EVENT_BODY: &str = r#"{"text":"benchmark event","project":"thingd","actor":"benchmark"}"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BackendSelection {
    All,
    InMemory,
    ThingDbMemory,
    RocksDb,
    ThingDb,
    Cache,
}

#[derive(Debug)]
struct BenchConfig {
    iterations: usize,
    repetitions: usize,
    seed: u64,
    backend: BackendSelection,
    output: Option<PathBuf>,
    history: PathBuf,
    phase: String,
    memtable_bytes: u64,
    reliability: bool,
    qualification: bool,
    queue_iterations: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
struct BenchResult {
    repetition: usize,
    driver: String,
    operation: String,
    operations: usize,
    total_ns: u128,
    throughput_ops_per_second: u128,
    min_ns: Option<u128>,
    p50_ns: Option<u128>,
    p95_ns: Option<u128>,
    p99_ns: Option<u128>,
    max_ns: Option<u128>,
    latency_sampled: bool,
    error_count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct StorageSnapshot {
    driver: String,
    repetition: usize,
    path: String,
    bytes_on_disk: u64,
    file_count: usize,
    filesystem_artifacts: usize,
    in_memory: bool,
}

#[derive(Debug, Serialize)]
struct BenchOutput {
    metadata: BenchMetadata,
    results: Vec<BenchResult>,
    summaries: Vec<BenchSummary>,
    storage: Vec<StorageSnapshot>,
    wal: Vec<WalSnapshot>,
    ram: Vec<RamSnapshot>,
    qualification: Vec<QualificationSnapshot>,
    queue: Vec<QueueSnapshot>,
}

#[derive(Debug, Serialize)]
struct QueueSnapshot {
    driver: String,
    repetition: usize,
    diagnostics: thingd::QueueDiagnostics,
}

#[derive(Debug, Serialize)]
struct QualificationSnapshot {
    driver: String,
    repetition: usize,
    operation: String,
    duration_ns: u128,
    passed: bool,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct BenchSummary {
    driver: String,
    operation: String,
    repetitions: usize,
    median_throughput_ops_per_second: u128,
    minimum_throughput_ops_per_second: u128,
    maximum_throughput_ops_per_second: u128,
    spread_throughput_ops_per_second: u128,
}

#[derive(Debug, Serialize)]
struct WalSnapshot {
    driver: String,
    repetition: usize,
    diagnostics: thingdb::WalDiagnostics,
}

#[derive(Debug, Serialize)]
struct RamSnapshot {
    driver: String,
    repetition: usize,
    diagnostics: thingdb::RamDiagnostics,
}

#[derive(Debug, Serialize)]
struct BenchMetadata {
    date: String,
    phase: String,
    branch: String,
    iterations: usize,
    repetitions: usize,
    seed: u64,
    backend: String,
    commit: String,
    rust: String,
    os: String,
    arch: String,
    parallelism: usize,
    thingdb_memtable_bytes: u64,
    reliability_preflight: bool,
    qualification_preflight: bool,
    queue_iterations: Option<usize>,
    peak_rss_bytes: Option<u64>,
    peak_rss_status: String,
    cpu_time_ns: Option<u64>,
    cpu_time_status: String,
    cpu_model: String,
    filesystem: String,
}

static RESULTS: std::sync::OnceLock<Mutex<BenchOutput>> = std::sync::OnceLock::new();
static CURRENT_REPETITION: AtomicUsize = AtomicUsize::new(0);
static PEAK_RSS_BYTES: AtomicU64 = AtomicU64::new(0);
static CPU_TIME_NS: AtomicU64 = AtomicU64::new(0);

#[allow(clippy::too_many_lines)]
fn main() -> Result<(), Box<dyn Error>> {
    let config = BenchConfig::from_args()?;
    RESULTS
        .set(Mutex::new(BenchOutput {
            metadata: BenchMetadata {
                date: command_output("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"]),
                phase: config.phase.clone(),
                branch: command_output("git", &["branch", "--show-current"]),
                iterations: config.iterations,
                repetitions: config.repetitions,
                seed: config.seed,
                backend: config.backend.name().to_string(),
                commit: command_output("git", &["rev-parse", "HEAD"]),
                rust: command_output("rustc", &["-Vv"]),
                os: env::consts::OS.to_string(),
                arch: env::consts::ARCH.to_string(),
                parallelism: std::thread::available_parallelism().map_or(1, usize::from),
                thingdb_memtable_bytes: config.memtable_bytes,
                reliability_preflight: config.reliability,
                qualification_preflight: config.qualification,
                queue_iterations: config.queue_iterations,
                peak_rss_bytes: None,
                peak_rss_status: "pending".to_string(),
                cpu_time_ns: None,
                cpu_time_status: "pending".to_string(),
                cpu_model: cpu_model(),
                filesystem: filesystem_type(),
            },
            results: Vec::new(),
            summaries: Vec::new(),
            storage: Vec::new(),
            wal: Vec::new(),
            ram: Vec::new(),
            qualification: Vec::new(),
            queue: Vec::new(),
        }))
        .map_err(|_| "benchmark output was initialized more than once")?;

    let iterations = config.iterations;
    sample_peak_rss();

    println!("thingd storage benchmark");
    println!("iterations: {iterations}");
    println!("seed: {}", config.seed);
    println!("backend: {}", config.backend.name());
    println!("repetitions: {}", config.repetitions);
    println!(
        "queue iterations: {}",
        config
            .queue_iterations
            .map_or_else(|| iterations.to_string(), |value| value.to_string())
    );
    println!(
        "ThingDB benchmark memtable bound: {} bytes",
        config.memtable_bytes
    );
    println!();
    println!(
        "{:>13} | {:<22} | {:>7} | {:>12} | {:>12}",
        "driver", "operation", "ops", "total", "ops/s"
    );
    println!("{}", "-".repeat(80));

    for repetition in 0..config.repetitions {
        CURRENT_REPETITION.store(repetition, Ordering::Relaxed);
        println!("repetition: {}", repetition + 1);

        if config.backend.includes(BackendSelection::InMemory) {
            bench_store(
                "memory-engine",
                MemoryEngine::new(),
                iterations,
                config.seed,
                config.queue_iterations,
            )?;
            bench_concurrent("memory-engine", || Ok(MemoryEngine::new()), iterations)?;
        }

        if config.backend.includes_thingdb_memory() {
            let thingdb_memory =
                PersistentEngine::open_in_memory_with_backend(thingd::PersistentBackend::ThingDb)?;
            bench_store(
                "thingdb-memory",
                thingdb_memory,
                iterations,
                config.seed,
                config.queue_iterations,
            )?;
            record_memory_storage("thingdb-memory");
            bench_memory_latency_distribution("thingdb-memory", iterations.min(256), config.seed)?;
            bench_queue_diagnostics("thingdb-memory", None, config.seed)?;
        }

        if config.backend.includes(BackendSelection::ThingDb)
            || config.backend.includes(BackendSelection::ThingDbMemory)
            || config.backend.includes(BackendSelection::Cache)
        {
            bench_cache("thingdb-cache", iterations, config.seed)?;
        }

        if config.backend.includes(BackendSelection::RocksDb) {
            let dir = tempfile::tempdir()?;
            let persistent_options =
                benchmark_persistent_options(thingd::PersistentBackend::RocksDb);
            let persistent_engine =
                PersistentEngine::open_with_options(dir.path(), persistent_options.clone())?;
            let lifecycle_dir = tempfile::tempdir()?;
            time_persistent_lifecycle(lifecycle_dir.path(), persistent_options.clone())?;
            bench_store(
                "persistent",
                persistent_engine,
                iterations,
                config.seed,
                config.queue_iterations,
            )?;
            record_storage("persistent", dir.path());
            let latency_dir = tempfile::tempdir()?;
            bench_latency_distribution(
                "persistent",
                latency_dir.path(),
                persistent_options,
                iterations.min(256),
                config.seed,
            )?;
        }

        if config.backend.includes(BackendSelection::ThingDb) {
            let thingdb_dir = tempfile::tempdir()?;
            let thingdb_options = benchmark_persistent_options(thingd::PersistentBackend::ThingDb);
            let thingdb_engine =
                PersistentEngine::open_with_options(thingdb_dir.path(), thingdb_options.clone())?;
            bench_store(
                "thingdb-experimental",
                thingdb_engine,
                iterations,
                config.seed,
                config.queue_iterations,
            )?;
            record_storage("thingdb-experimental", thingdb_dir.path());
            bench_queue_diagnostics(
                "thingdb-experimental",
                Some(thingdb_dir.path()),
                config.seed,
            )?;
            let latency_dir = tempfile::tempdir()?;
            bench_latency_distribution(
                "thingdb-experimental",
                latency_dir.path(),
                thingdb_options,
                iterations.min(256),
                config.seed,
            )?;
            let wal_dir = tempfile::tempdir()?;
            bench_wal_workloads(
                "thingdb-experimental",
                wal_dir.path(),
                benchmark_persistent_options(thingd::PersistentBackend::ThingDb),
                iterations,
                config.seed,
                config.memtable_bytes,
            )?;
        }

        if config.backend.includes(BackendSelection::RocksDb) {
            let conc_dir = tempfile::tempdir()?;
            let persistent_options =
                benchmark_persistent_options(thingd::PersistentBackend::RocksDb);
            bench_concurrent(
                "persistent",
                || {
                    let engine = PersistentEngine::open_with_options(
                        conc_dir.path(),
                        persistent_options.clone(),
                    )?;
                    Ok(engine)
                },
                iterations,
            )?;
        }

        if config.backend.includes(BackendSelection::ThingDb) {
            let thingdb_conc_dir = tempfile::tempdir()?;
            let thingdb_options = benchmark_persistent_options(thingd::PersistentBackend::ThingDb);
            bench_concurrent(
                "thingdb-experimental",
                || {
                    let engine = PersistentEngine::open_with_options(
                        thingdb_conc_dir.path(),
                        thingdb_options.clone(),
                    )?;
                    Ok(engine)
                },
                iterations,
            )?;
            bench_concurrent(
                "thingdb-memory",
                || {
                    Ok(PersistentEngine::open_in_memory_with_backend(
                        thingd::PersistentBackend::ThingDb,
                    )?)
                },
                iterations,
            )?;
        }

        if config.backend.includes(BackendSelection::RocksDb) {
            let encrypted_dir = tempfile::tempdir()?;
            let encrypted_options = PersistentOpenOptions {
                backend: thingd::PersistentBackend::RocksDb,
                encryption: Some(EncryptionConfig::from_key(&[0x42_u8; 32])?),
                ..PersistentOpenOptions::default()
            };
            let encrypted_engine =
                PersistentEngine::open_with_options(encrypted_dir.path(), encrypted_options)?;
            bench_store(
                "persistent-encrypted",
                encrypted_engine,
                iterations,
                config.seed,
                config.queue_iterations,
            )?;
            record_storage("persistent-encrypted", encrypted_dir.path());
        }

        if config.backend.includes(BackendSelection::RocksDb) {
            let encrypted_conc_dir = tempfile::tempdir()?;
            let encrypted_options = PersistentOpenOptions {
                backend: thingd::PersistentBackend::RocksDb,
                encryption: Some(EncryptionConfig::from_key(&[0x42_u8; 32])?),
                ..PersistentOpenOptions::default()
            };
            bench_concurrent(
                "persistent-encrypted",
                || {
                    Ok(PersistentEngine::open_with_options(
                        encrypted_conc_dir.path(),
                        encrypted_options.clone(),
                    )?)
                },
                iterations,
            )?;
        }

        if config.backend == BackendSelection::ThingDbMemory {
            bench_concurrent(
                "thingdb-memory",
                || {
                    Ok(PersistentEngine::open_in_memory_with_backend(
                        thingd::PersistentBackend::ThingDb,
                    )?)
                },
                iterations,
            )?;
        }

        if config.qualification {
            run_durable_qualification(config.seed.saturating_add(repetition as u64))?;
        }
    }

    run_correctness_smoke(config.seed)?;
    if config.reliability {
        run_reliability_preflight(config.seed)?;
    }
    update_summaries();
    if let Some(path) = config.output.as_deref() {
        write_output(path)?;
        println!("structured results: {}", path.display());
    }
    append_history(&config.history)?;
    println!("benchmark history: {}", config.history.display());

    Ok(())
}

fn benchmark_persistent_options(backend: thingd::PersistentBackend) -> PersistentOpenOptions {
    let search_mode = match env::var("THINGD_BENCH_SEARCH_MODE").as_deref() {
        Ok("disabled") => thingd::PersistentSearchMode::Disabled,
        Ok("persistent-no-rebuild") => thingd::PersistentSearchMode::PersistentNoRebuild,
        _ => thingd::PersistentSearchMode::Persistent,
    };
    PersistentOpenOptions {
        backend,
        search_mode,
        ..PersistentOpenOptions::default()
    }
}

#[allow(clippy::too_many_lines)]
fn bench_cache(name: &str, iterations: usize, seed: u64) -> Result<(), Box<dyn Error>> {
    let cache = MemoryCache::new(CacheOptions {
        max_entries: iterations.max(1),
        max_bytes: iterations.saturating_mul(256).max(256),
        default_ttl: Duration::from_mins(1),
    })?;
    let value = b"{\"text\":\"thingdb cache benchmark\",\"kind\":\"catalog\"}";
    let started = Instant::now();
    for index in 0..iterations {
        let key = cache_key(seed, index);
        cache.insert(&key, value)?;
    }
    report(name, "cache_insert", iterations, started.elapsed());

    let hot_keys = (0..iterations.clamp(1, 1_024))
        .map(|index| cache_key(seed, index))
        .collect::<Vec<_>>();
    let started = Instant::now();
    let mut hits = 0;
    for index in 0..iterations {
        if cache.get(&hot_keys[index % hot_keys.len()])?.is_some() {
            hits += 1;
        }
    }
    if hits != iterations {
        return Err(format!(
            "ThingDB cache hit benchmark lost {}/{} hits",
            iterations - hits,
            iterations
        )
        .into());
    }
    report(name, "cache_hot_get", iterations, started.elapsed());

    let latency = measure_samples(iterations.clamp(1, 1_024), |index| {
        let key = &hot_keys[index % hot_keys.len()];
        black_box(cache.get(key)?);
        Ok(())
    })?;
    record_latency(name, "cache_get_latency", latency);

    let mixed_started = Instant::now();
    for index in 0..iterations {
        let key = cache_key(seed ^ 0x9e37_79b9, index);
        if index % 4 == 0 {
            cache.insert(&key, value)?;
        } else {
            black_box(cache.get(&key)?);
        }
    }
    report(name, "cache_mixed", iterations, mixed_started.elapsed());

    let stats = cache.stats()?;
    if stats.current_bytes > stats.max_bytes || stats.current_entries > stats.max_entries {
        return Err("ThingDB cache exceeded its configured bounds".into());
    }
    println!(
        "{name} | stats hits={} misses={} evictions={} bytes={}",
        stats.hits, stats.misses, stats.evictions, stats.current_bytes
    );

    let concurrent = Arc::new(MemoryCache::new(CacheOptions {
        max_entries: 4_096,
        max_bytes: 4 * 1024 * 1024,
        default_ttl: Duration::from_mins(1),
    })?);
    for index in 0..512 {
        concurrent.insert(&cache_key(seed, index), value)?;
    }
    let started = Instant::now();
    let mut workers = Vec::new();
    for thread in 0..4 {
        let cache = Arc::clone(&concurrent);
        workers.push(std::thread::spawn(move || -> Result<usize, String> {
            let mut hits = 0;
            for index in 0..iterations {
                let key = cache_key(
                    seed ^ u64::try_from(thread).unwrap_or(u64::MAX),
                    index % 512,
                );
                if index % 8 == 0 {
                    cache
                        .insert(&key, value)
                        .map_err(|error| error.to_string())?;
                } else if cache
                    .get(&key)
                    .map_err(|error| error.to_string())?
                    .is_some()
                {
                    hits += 1;
                }
            }
            Ok(hits)
        }));
    }
    let hits = workers
        .into_iter()
        .map(|worker| {
            worker
                .join()
                .map_err(|_| "cache worker panicked".to_string())?
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .sum::<usize>();
    report(
        name,
        "cache_concurrent_4t",
        iterations * 4,
        started.elapsed(),
    );
    println!("{name} | concurrent_hits={hits}");
    Ok(())
}

fn cache_key(seed: u64, index: usize) -> Vec<u8> {
    format!("catalog:{seed}:{index}").into_bytes()
}

fn time_persistent_lifecycle(
    path: &std::path::Path,
    options: PersistentOpenOptions,
) -> Result<(), Box<dyn Error>> {
    let mut engine = PersistentEngine::open_with_options(path, options.clone())?;
    engine.put_object(MemoryObject::new(
        "bench_startup",
        "first-request",
        OBJECT_BODY_ACTIVE,
    ))?;
    drop(engine);

    let started = Instant::now();
    let engine = PersistentEngine::open_with_options(path, options)?;
    let startup = started.elapsed();
    let first_request = Instant::now();
    let hits = engine.search("benchmark", SearchOptions::default())?;
    let first_request_elapsed = first_request.elapsed();
    println!(
        "lifecycle | startup={:?} first_request={:?} search_hits={}",
        startup,
        first_request_elapsed,
        hits.len()
    );
    Ok(())
}

impl BenchConfig {
    fn from_args() -> Result<Self, Box<dyn Error>> {
        let mut iterations = env::var("THINGD_BENCH_ITERS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_ITERATIONS);
        let mut repetitions = env::var("THINGD_BENCH_REPETITIONS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1);
        let mut seed = 0x5eed_u64;
        let mut backend = BackendSelection::All;
        let mut output = env::var_os("THINGD_BENCH_OUTPUT").map(PathBuf::from);
        let mut history = env::var_os("THINGD_BENCH_HISTORY").map_or_else(
            || PathBuf::from("target/storage-benchmark-history.jsonl"),
            PathBuf::from,
        );
        let mut phase = env::var("THINGD_BENCH_PHASE").unwrap_or_else(|_| "baseline".to_string());
        let mut memtable_bytes = env::var("THINGD_BENCH_MEMTABLE_BYTES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_MEMTABLE_BYTES);
        let mut reliability = env::var("THINGD_BENCH_RELIABILITY")
            .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
        let mut qualification = env::var("THINGD_BENCH_QUALIFICATION")
            .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
        let mut queue_iterations: Option<usize> = env::var("THINGD_BENCH_QUEUE_ITERS")
            .ok()
            .and_then(|value| value.parse().ok());
        let mut positional_iterations = None;
        let mut args = env::args().skip(1);
        while let Some(argument) = args.next() {
            let mut value = |flag: &str| -> Result<String, Box<dyn Error>> {
                args.next()
                    .ok_or_else(|| format!("missing value for {flag}").into())
            };
            match argument.as_str() {
                "--iterations" => iterations = value("--iterations")?.parse()?,
                "--repetitions" => repetitions = value("--repetitions")?.parse()?,
                "--seed" => seed = value("--seed")?.parse()?,
                "--output" => output = Some(PathBuf::from(value("--output")?)),
                "--history" => history = PathBuf::from(value("--history")?),
                "--phase" => phase = value("--phase")?,
                "--memtable-bytes" => memtable_bytes = value("--memtable-bytes")?.parse()?,
                "--reliability" => reliability = true,
                "--qualification" => qualification = true,
                "--queue-iterations" => {
                    queue_iterations = Some(value("--queue-iterations")?.parse()?);
                },
                "--backend" => {
                    backend = match value("--backend")?.as_str() {
                        "all" => BackendSelection::All,
                        "memory" => BackendSelection::InMemory,
                        "rocksdb" => BackendSelection::RocksDb,
                        "thingdb" => BackendSelection::ThingDb,
                        "thingdb-memory" => BackendSelection::ThingDbMemory,
                        "cache" => BackendSelection::Cache,
                        other => return Err(format!("unknown backend {other:?}").into()),
                    };
                },
                value if !value.starts_with('-') && positional_iterations.is_none() => {
                    positional_iterations = Some(value.parse()?);
                },
                other => return Err(format!("unknown benchmark argument {other:?}").into()),
            }
        }
        if let Some(value) = positional_iterations {
            iterations = value;
        }
        Ok(Self {
            iterations,
            repetitions: repetitions.max(1),
            seed,
            backend,
            output,
            history,
            phase,
            memtable_bytes: memtable_bytes.max(1),
            reliability,
            qualification,
            queue_iterations: queue_iterations.map(|value| value.max(1)),
        })
    }
}

impl BackendSelection {
    fn includes(self, backend: Self) -> bool {
        self == Self::All || self == backend
    }

    fn includes_thingdb_memory(self) -> bool {
        self.includes(Self::ThingDb) || self.includes(Self::ThingDbMemory)
    }

    const fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::InMemory => "memory",
            Self::ThingDbMemory => "thingdb-memory",
            Self::RocksDb => "rocksdb",
            Self::ThingDb => "thingdb",
            Self::Cache => "cache",
        }
    }
}

fn bench_store<S>(
    name: &str,
    mut store: S,
    iterations: usize,
    _seed: u64,
    queue_iterations: Option<usize>,
) -> Result<(), Box<dyn Error>>
where
    S: EventLog + ObjectStore + QueueStore + Searcher + VectorStore,
{
    let elapsed = time_object_puts(&mut store, iterations)?;
    report(name, "object_put", iterations, elapsed);

    let elapsed = time_object_put_batch(&mut store, iterations)?;
    report(name, "object_batch", iterations, elapsed);

    let elapsed = time_object_gets(&store, iterations)?;
    report(name, "object_get", iterations, elapsed);

    let started = Instant::now();
    let all_objects = store.list_objects(None, &ListObjectsOptions::default())?;
    let elapsed = started.elapsed();
    black_box(all_objects.len());
    report(name, "list_objects", 1, elapsed);

    let filter_opts = ListObjectsOptions {
        filter: vec![("status".into(), serde_json::json!("active"))],
        ..Default::default()
    };
    let started = Instant::now();
    let filtered = store.list_objects(None, &filter_opts)?;
    let elapsed = started.elapsed();
    black_box(filtered.len());
    report(name, "list_objects_filter", 1, elapsed);

    let limit_opts = ListObjectsOptions {
        limit: Some(100),
        ..Default::default()
    };
    let started = Instant::now();
    let limited = store.list_objects(Some(&[COLLECTION.to_string()]), &limit_opts)?;
    let elapsed = started.elapsed();
    black_box(limited.len());
    report(name, "list_objects_limit100", 1, elapsed);

    let paginate_opts = ListObjectsOptions {
        limit: Some(100),
        offset: Some(50),
        ..Default::default()
    };
    let started = Instant::now();
    let page = store.list_objects(Some(&[COLLECTION.to_string()]), &paginate_opts)?;
    let elapsed = started.elapsed();
    black_box(page.len());
    report(name, "list_objects_page", 1, elapsed);

    let elapsed = time_event_appends(&mut store, iterations)?;
    report(name, "event_append", iterations, elapsed);

    let elapsed = time_event_append_batch(&mut store, iterations)?;
    report(name, "event_batch", iterations, elapsed);

    let started = Instant::now();
    let events = store.list_events(Some(STREAM), ListEventsOptions::default())?;
    let elapsed = started.elapsed();
    black_box(events.len());
    report(name, "event_list", 1, elapsed);

    let midpoint = events.len() as u64 / 2;
    let from_opts = ListEventsOptions {
        from_sequence: Some(midpoint),
        limit: None,
        since: None,
    };
    let started = Instant::now();
    let tail = store.list_events(Some(STREAM), from_opts)?;
    let elapsed = started.elapsed();
    black_box(tail.len());
    report(name, "event_list_from_seq", 1, elapsed);

    let limit_event_opts = ListEventsOptions {
        from_sequence: None,
        limit: Some(100),
        since: None,
    };
    let started = Instant::now();
    let limited_events = store.list_events(Some(STREAM), limit_event_opts)?;
    let elapsed = started.elapsed();
    black_box(limited_events.len());
    report(name, "event_list_limit100", 1, elapsed);

    let queue_iterations = queue_iterations.unwrap_or(iterations).min(iterations);
    let elapsed = time_queue_pushes(&mut store, queue_iterations)?;
    report(name, "queue_push", queue_iterations, elapsed);

    let elapsed = time_queue_push_batch(&mut store, queue_iterations)?;
    report(name, "queue_batch", queue_iterations, elapsed);

    let elapsed = time_queue_claims_and_acks(&mut store, queue_iterations)?;
    report(name, "queue_claim_ack", queue_iterations, elapsed);

    let elapsed = time_queue_claim_and_ack(&mut store, queue_iterations)?;
    report(name, "queue_claim_ack2", queue_iterations, elapsed);

    time_search_benchmarks(name, &store)?;
    time_vector_benchmarks(name, &mut store, iterations)?;

    time_batch_scale_benchmarks(name, &mut store, iterations)?;

    time_count_benchmarks(name, &store)?;

    let elapsed = time_object_deletes(&mut store, iterations)?;
    report(name, "object_delete", iterations, elapsed);

    println!();
    Ok(())
}

fn bench_latency_distribution(
    name: &str,
    path: &Path,
    options: PersistentOpenOptions,
    samples: usize,
    seed: u64,
) -> Result<(), Box<dyn Error>> {
    let mut store = PersistentEngine::open_with_options(path, options)?;
    let samples = samples.max(1);
    let put_latencies = measure_samples(samples, |index| {
        let id = format!("latency-{}", deterministic_index(seed, index, samples));
        store.put_object(MemoryObject::new(COLLECTION, id, OBJECT_BODY_ACTIVE))?;
        Ok(())
    })?;
    record_latency(name, "latency_object_put", put_latencies);

    let get_latencies = measure_samples(samples, |index| {
        let id = format!("latency-{}", deterministic_index(seed, index, samples));
        black_box(store.get_object(COLLECTION, &id)?);
        Ok(())
    })?;
    record_latency(name, "latency_object_get", get_latencies);

    let event_latencies = measure_samples(samples, |index| {
        let event = MemoryEvent::new(
            STREAM,
            "benchmark.latency",
            format!("{EVENT_BODY}:latency-{index}"),
        );
        black_box(store.append_event(event)?);
        Ok(())
    })?;
    record_latency(name, "latency_event_append", event_latencies);
    Ok(())
}

fn bench_memory_latency_distribution(
    name: &str,
    samples: usize,
    seed: u64,
) -> Result<(), Box<dyn Error>> {
    let mut store =
        PersistentEngine::open_in_memory_with_backend(thingd::PersistentBackend::ThingDb)?;
    let samples = samples.max(1);
    let put_latencies = measure_samples(samples, |index| {
        let id = format!(
            "memory-latency-{}",
            deterministic_index(seed, index, samples)
        );
        store.put_object(MemoryObject::new(COLLECTION, id, OBJECT_BODY_ACTIVE))?;
        Ok(())
    })?;
    record_latency(name, "latency_object_put", put_latencies);

    let get_latencies = measure_samples(samples, |index| {
        let id = format!(
            "memory-latency-{}",
            deterministic_index(seed, index, samples)
        );
        black_box(store.get_object(COLLECTION, &id)?);
        Ok(())
    })?;
    record_latency(name, "latency_object_get", get_latencies);

    let event_latencies = measure_samples(samples, |index| {
        let event = MemoryEvent::new(
            STREAM,
            "benchmark.memory-latency",
            format!("{EVENT_BODY}:memory-latency-{index}"),
        );
        black_box(store.append_event(event)?);
        Ok(())
    })?;
    record_latency(name, "latency_event_append", event_latencies);
    black_box(store.search("benchmark", SearchOptions::default())?);
    if let Some(diagnostics) = store.ram_diagnostics()? {
        results().lock().unwrap().ram.push(RamSnapshot {
            driver: name.to_string(),
            repetition: CURRENT_REPETITION.load(Ordering::Relaxed),
            diagnostics,
        });
    }
    Ok(())
}

fn bench_wal_workloads(
    name: &str,
    path: &Path,
    mut options: PersistentOpenOptions,
    iterations: usize,
    seed: u64,
    memtable_bytes: u64,
) -> Result<(), Box<dyn Error>> {
    options.search_mode = thingd::PersistentSearchMode::Disabled;
    let mut engine = PersistentEngine::open_with_options(path, options.clone())?;
    let samples = measure_samples(iterations.clamp(1, 256), |index| {
        let id = format!(
            "wal-single-{}",
            deterministic_index(seed, index, iterations.max(1))
        );
        engine.put_object(MemoryObject::new(COLLECTION, id, OBJECT_BODY_ACTIVE))?;
        Ok(())
    })?;
    record_latency(name, "wal-single-write", samples);

    let batch_size = iterations.clamp(1, 1_000);
    let objects = (0..batch_size)
        .map(|index| {
            MemoryObject::new(COLLECTION, format!("wal-batch-{index}"), OBJECT_BODY_ACTIVE)
        })
        .collect();
    let started = Instant::now();
    let stored = engine.put_objects_batch(objects)?;
    black_box(stored.len());
    report(name, "wal-explicit-batch", batch_size, started.elapsed());

    let before_reopen = engine
        .wal_diagnostics()?
        .ok_or("ThingDB diagnostics unavailable")?;
    drop(engine);
    let started = Instant::now();
    let mut reopened = PersistentEngine::open_with_options(path, options.clone())?;
    report(name, "wal-recovery", 1, started.elapsed());

    let started = Instant::now();
    for index in 0..iterations {
        let id = format!(
            "wal-single-{}",
            deterministic_index(seed, index, iterations.max(1))
        );
        black_box(reopened.get_object(COLLECTION, &id)?);
    }
    report(name, "table-point-read", iterations, started.elapsed());

    let started = Instant::now();
    reopened.compact_storage()?;
    report(name, "table-compaction", 1, started.elapsed());
    drop(reopened);

    let started = Instant::now();
    let reopened = PersistentEngine::open_with_options(path, options)?;
    report(name, "table-recovery", 1, started.elapsed());
    let after_reopen = reopened
        .wal_diagnostics()?
        .ok_or("ThingDB diagnostics unavailable after reopen")?;
    results().lock().unwrap().wal.push(WalSnapshot {
        driver: name.to_string(),
        repetition: CURRENT_REPETITION.load(Ordering::Relaxed),
        diagnostics: WalDiagnosticsSnapshot::merge(before_reopen, after_reopen),
    });

    let bounded_path = path.join("bounded-memtable");
    let bounded_db = thingdb::Database::builder(&bounded_path)
        .max_memtable_bytes(memtable_bytes)
        .open()?;
    let bounded_keyspace =
        bounded_db.keyspace("objects", thingdb::KeyspaceCreateOptions::default)?;
    let started = Instant::now();
    for index in 0..iterations {
        let key = format!("bounded-{seed}-{index}");
        bounded_keyspace.insert(key.as_bytes(), OBJECT_BODY_ACTIVE.as_bytes())?;
    }
    report(
        name,
        "memtable-bounded-write",
        iterations,
        started.elapsed(),
    );
    let before_reopen = bounded_db.wal_diagnostics()?;
    let expected = iterations;
    drop(bounded_keyspace);
    drop(bounded_db);
    let started = Instant::now();
    let bounded_reopened = thingdb::Database::open(&bounded_path)?;
    report(name, "memtable-bounded-recovery", 1, started.elapsed());
    let bounded_keyspace =
        bounded_reopened.keyspace("objects", thingdb::KeyspaceCreateOptions::default)?;
    let actual = bounded_keyspace.iter().count();
    if actual != expected {
        return Err(format!(
            "bounded memtable correctness mismatch: expected {expected}, got {actual}"
        )
        .into());
    }
    let after_reopen = bounded_reopened.wal_diagnostics()?;
    results().lock().unwrap().wal.push(WalSnapshot {
        driver: name.to_string(),
        repetition: CURRENT_REPETITION.load(Ordering::Relaxed),
        diagnostics: WalDiagnosticsSnapshot::merge(before_reopen, after_reopen),
    });
    bench_thingdb_concurrent_writes(name, path, iterations, seed)?;
    Ok(())
}

fn bench_thingdb_concurrent_writes(
    name: &str,
    path: &Path,
    iterations: usize,
    seed: u64,
) -> Result<(), Box<dyn Error>> {
    let concurrent_path = path.join("group-commit");
    let db = thingdb::Database::open(&concurrent_path)?;
    let keyspace = db.keyspace("objects", thingdb::KeyspaceCreateOptions::default)?;
    let writers = iterations.clamp(1, 32);
    let per_writer = iterations.div_ceil(writers);
    let barrier = Arc::new(Barrier::new(writers));
    let started = Instant::now();
    let handles: Vec<_> = (0..writers)
        .map(|writer| {
            let keyspace = keyspace.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || -> thingdb::Result<usize> {
                barrier.wait();
                for offset in 0..per_writer {
                    let index = writer * per_writer + offset;
                    let key = format!(
                        "group-{}-{}-{index}",
                        seed,
                        deterministic_index(seed, index, iterations.max(1))
                    );
                    keyspace.insert(key.as_bytes(), OBJECT_BODY_ACTIVE.as_bytes())?;
                }
                Ok(per_writer)
            })
        })
        .collect();
    let mut completed = 0;
    for handle in handles {
        completed += handle
            .join()
            .map_err(|_| "ThingDB concurrent benchmark writer panicked")??;
    }
    report(name, "wal-concurrent-write", completed, started.elapsed());

    let before_reopen = db.wal_diagnostics()?;
    drop(keyspace);
    drop(db);
    let started = Instant::now();
    let reopened = thingdb::Database::open(&concurrent_path)?;
    report(name, "wal-concurrent-recovery", 1, started.elapsed());
    let after_reopen = reopened.wal_diagnostics()?;
    results().lock().unwrap().wal.push(WalSnapshot {
        driver: name.to_string(),
        repetition: CURRENT_REPETITION.load(Ordering::Relaxed),
        diagnostics: WalDiagnosticsSnapshot::merge(before_reopen, after_reopen),
    });
    Ok(())
}

struct WalDiagnosticsSnapshot;

impl WalDiagnosticsSnapshot {
    fn merge(
        before: thingdb::WalDiagnostics,
        after: thingdb::WalDiagnostics,
    ) -> thingdb::WalDiagnostics {
        thingdb::WalDiagnostics {
            journal_bytes: before.journal_bytes.max(after.journal_bytes),
            frame_count: before.frame_count.max(after.frame_count),
            recovery_bytes: after.recovery_bytes,
            recovery_duration_ns: after.recovery_duration_ns,
            encode_duration_ns: before.encode_duration_ns,
            append_duration_ns: before.append_duration_ns,
            sync_duration_ns: before.sync_duration_ns,
            state_apply_duration_ns: before.state_apply_duration_ns,
            lock_duration_ns: before.lock_duration_ns,
            logical_commit_count: before.logical_commit_count,
            physical_sync_count: before.physical_sync_count,
            total_group_size: before.total_group_size,
            max_group_size: before.max_group_size,
            queue_wait_duration_ns: before.queue_wait_duration_ns,
            recovery_required: after.recovery_required,
            wal_over_budget: before.wal_over_budget || after.wal_over_budget,
            memtable_bytes: before.memtable_bytes.max(after.memtable_bytes),
            flush_count: before.flush_count.max(after.flush_count),
            automatic_flush_count: before
                .automatic_flush_count
                .max(after.automatic_flush_count),
            flush_duration_ns: before.flush_duration_ns,
            memtable_over_budget: before.memtable_over_budget || after.memtable_over_budget,
            last_error: after.last_error.or(before.last_error),
            table_lookup_count: before.table_lookup_count,
            mutable_state_lookup_count: before.mutable_state_lookup_count,
            pending_table_lookup_count: before.pending_table_lookup_count,
            immutable_layer_lookup_count: before.immutable_layer_lookup_count,
            table_layers_consulted: before.table_layers_consulted,
            table_bytes_read: before.table_bytes_read,
            table_read_duration_ns: before.table_read_duration_ns,
            table_open_duration_ns: before.table_open_duration_ns,
            scan_duration_ns: before.scan_duration_ns,
            scan_count: before.scan_count,
            table_layer_count: before.table_layer_count.max(after.table_layer_count),
            compaction_count: before.compaction_count.max(after.compaction_count),
            compaction_duration_ns: before.compaction_duration_ns,
            compaction_input_bytes: before.compaction_input_bytes,
            compaction_output_bytes: before.compaction_output_bytes,
        }
    }
}

fn run_correctness_smoke(seed: u64) -> Result<(), Box<dyn Error>> {
    for backend in [
        thingd::PersistentBackend::RocksDb,
        thingd::PersistentBackend::ThingDb,
    ] {
        let directory = tempfile::tempdir()?;
        let options = PersistentOpenOptions {
            backend,
            search_mode: thingd::PersistentSearchMode::Disabled,
            ..PersistentOpenOptions::default()
        };
        let first_id = format!("correctness-{seed}-first");
        let second_id = format!("correctness-{seed}-second");
        let event_body = format!("{{\"seed\":{seed}}}");

        {
            let mut engine =
                PersistentEngine::open_with_options(directory.path(), options.clone())?;
            let first = engine.put_object(MemoryObject::new(
                "correctness",
                &first_id,
                OBJECT_BODY_ACTIVE,
            ))?;
            let updated = engine.put_object(MemoryObject::new(
                "correctness",
                &first_id,
                OBJECT_BODY_INACTIVE,
            ))?;
            if updated.version <= first.version || updated.body != OBJECT_BODY_INACTIVE {
                return Err(format!("{backend:?}: update/version mismatch").into());
            }

            let batch = engine.put_objects_batch(vec![MemoryObject::new(
                "correctness",
                &second_id,
                OBJECT_BODY_ACTIVE,
            )])?;
            if batch.len() != 1 {
                return Err(
                    format!("{backend:?}: atomic batch returned {} records", batch.len()).into(),
                );
            }
            engine.append_event(MemoryEvent::new("correctness", "smoke", event_body.clone()))?;
            if engine.count_objects()? != 2 || engine.count_events()? != 1 {
                return Err(format!("{backend:?}: count mismatch before reopen").into());
            }
        }

        let engine = PersistentEngine::open_with_options(directory.path(), options)?;
        let reopened = engine
            .get_object("correctness", &first_id)?
            .ok_or_else(|| format!("{backend:?}: updated object missing after reopen"))?;
        if reopened.body != OBJECT_BODY_INACTIVE || reopened.version < 2 {
            return Err(format!("{backend:?}: reopened object mismatch").into());
        }
        let second = engine
            .get_object("correctness", &second_id)?
            .ok_or_else(|| format!("{backend:?}: batched object missing after reopen"))?;
        if second.body != OBJECT_BODY_ACTIVE || engine.count_events()? != 1 {
            return Err(format!("{backend:?}: reopened batch/event mismatch").into());
        }
        println!("correctness | {backend:?} | passed");
    }
    Ok(())
}

fn run_reliability_preflight(seed: u64) -> Result<(), Box<dyn Error>> {
    run_reliability_scenario("memory-engine", &mut MemoryEngine::new(), seed)?;
    run_reliability_scenario(
        "thingdb-memory",
        &mut PersistentEngine::open_in_memory_with_backend(thingd::PersistentBackend::ThingDb)?,
        seed,
    )?;

    run_concurrent_reliability("memory-engine", || Ok(MemoryEngine::new()), seed)?;
    run_concurrent_reliability(
        "thingdb-memory",
        || {
            Ok(PersistentEngine::open_in_memory_with_backend(
                thingd::PersistentBackend::ThingDb,
            )?)
        },
        seed,
    )?;

    for cycle in 0..3 {
        let mut engine =
            PersistentEngine::open_in_memory_with_backend(thingd::PersistentBackend::ThingDb)?;
        if !engine.is_in_memory() || engine.count_objects()? != 0 || engine.count_events()? != 0 {
            return Err(format!("thingdb-memory isolation failed at cycle {cycle}").into());
        }
        run_reliability_scenario("thingdb-memory-reopen", &mut engine, seed + cycle)?;
    }

    println!("reliability | memory backends | passed");
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn run_durable_qualification(seed: u64) -> Result<(), Box<dyn Error>> {
    for (source_backend, destination_backend) in [
        (
            thingd::PersistentBackend::RocksDb,
            thingd::PersistentBackend::ThingDb,
        ),
        (
            thingd::PersistentBackend::ThingDb,
            thingd::PersistentBackend::RocksDb,
        ),
    ] {
        let source_dir = tempfile::tempdir()?;
        let source_options = PersistentOpenOptions {
            backend: source_backend,
            search_mode: thingd::PersistentSearchMode::Disabled,
            ..PersistentOpenOptions::default()
        };
        let object_id = format!("qualification-{seed}");
        let started = Instant::now();
        {
            let mut source =
                PersistentEngine::open_with_options(source_dir.path(), source_options.clone())?;
            source.put_object(MemoryObject::new(
                "qualification",
                &object_id,
                OBJECT_BODY_ACTIVE,
            ))?;
            source.put_objects_batch(vec![MemoryObject::new(
                "qualification",
                format!("{object_id}-batch"),
                OBJECT_BODY_INACTIVE,
            )])?;
            source.append_event(MemoryEvent::new(
                "qualification",
                "qualification",
                format!("{{\"seed\":{seed}}}"),
            ))?;
            source.push_job(QueueJob::new(
                "qualification-queue",
                &object_id,
                r#"{"task":"qualification"}"#,
                3,
            ))?;
            let claimed = source
                .claim_job("qualification-queue")?
                .ok_or("qualification queue claim returned no job")?;
            source
                .ack_job("qualification-queue", &claimed.id)?
                .ok_or("qualification queue ack returned no job")?;
            source.compact_storage()?;
        }
        let reopened = PersistentEngine::open_with_options(source_dir.path(), source_options)?;
        let object = reopened
            .get_object("qualification", &object_id)?
            .ok_or("qualification source object missing after reopen")?;
        if object.body != OBJECT_BODY_ACTIVE || reopened.count_events()? != 1 {
            return Err("qualification source logical verification failed".into());
        }
        PersistentEngine::validate_path_with_backend(source_dir.path(), source_backend)?;
        drop(reopened);

        let destination_parent = tempfile::tempdir()?;
        let destination = destination_parent.path().join("repacked");
        PersistentEngine::repack_to_with_backends(
            source_dir.path(),
            &destination,
            source_backend,
            destination_backend,
            None,
        )?;
        PersistentEngine::validate_path_with_backend(&destination, destination_backend)?;
        let destination_options = PersistentOpenOptions {
            backend: destination_backend,
            search_mode: thingd::PersistentSearchMode::Disabled,
            ..PersistentOpenOptions::default()
        };
        let repacked = PersistentEngine::open_with_options(&destination, destination_options)?;
        let repacked_object = repacked
            .get_object("qualification", &object_id)?
            .ok_or("repacked qualification object missing")?;
        if repacked_object.body != OBJECT_BODY_ACTIVE || repacked.count_events()? != 1 {
            return Err("qualification repack logical verification failed".into());
        }
        if !source_dir.path().exists() {
            return Err("qualification repack removed the source directory".into());
        }
        let duration_ns = started.elapsed().as_nanos();
        let driver = format!("{source_backend:?}-to-{destination_backend:?}");
        results()
            .lock()
            .unwrap()
            .qualification
            .push(QualificationSnapshot {
                driver: driver.clone(),
                repetition: CURRENT_REPETITION.load(Ordering::Relaxed),
                operation: "reopen-compact-repack-validate".to_string(),
                duration_ns,
                passed: true,
                error: None,
            });
        println!(
            "qualification | {driver} | passed ({:?})",
            nanos_duration(duration_ns)
        );
    }
    Ok(())
}

fn bench_queue_diagnostics(
    name: &str,
    path: Option<&Path>,
    seed: u64,
) -> Result<(), Box<dyn Error>> {
    let mut engine = match path {
        Some(path) => PersistentEngine::open_with_options(
            path,
            benchmark_persistent_options(thingd::PersistentBackend::ThingDb),
        )?,
        None => PersistentEngine::open_in_memory_with_backend(thingd::PersistentBackend::ThingDb)?,
    };
    let jobs = (0..64)
        .map(|index| {
            QueueJob::new(
                "diagnostics",
                format!("{seed}-{index}"),
                r#"{"task":"diagnostics"}"#,
                3,
            )
        })
        .collect();
    engine.push_jobs_batch(jobs)?;
    for _ in 0..64 {
        let Some(job) = engine.claim_job("diagnostics")? else {
            return Err("queue diagnostics could not claim all jobs".into());
        };
        engine
            .ack_job("diagnostics", &job.id)?
            .ok_or("queue diagnostics acknowledgement failed")?;
    }
    results().lock().unwrap().queue.push(QueueSnapshot {
        driver: name.to_string(),
        repetition: CURRENT_REPETITION.load(Ordering::Relaxed),
        diagnostics: engine.queue_diagnostics(),
    });
    Ok(())
}

fn run_reliability_scenario<S>(name: &str, store: &mut S, seed: u64) -> Result<(), Box<dyn Error>>
where
    S: EventLog + LinkStore + ObjectStore + QueueStore + Searcher + VectorStore,
{
    let objects = (0..16)
        .map(|index| {
            MemoryObject::new(
                "reliability",
                format!("object-{seed}-{index}"),
                if index % 2 == 0 {
                    OBJECT_BODY_ACTIVE
                } else {
                    OBJECT_BODY_INACTIVE
                },
            )
        })
        .collect::<Vec<_>>();
    let stored = store.put_objects_batch(objects)?;
    if stored.len() != 16 {
        return Err(format!("{name}: atomic object batch mismatch").into());
    }

    let first_id = format!("object-{seed}-0");
    let updated = store.put_object(MemoryObject::new(
        "reliability",
        &first_id,
        r#"{"text":"reliabilityupdated"}"#,
    ))?;
    if updated.version != 2 {
        return Err(format!("{name}: object version mismatch").into());
    }

    let event = store.append_event(MemoryEvent::new(
        "reliability",
        "preflight",
        format!("{{\"seed\":{seed}}}"),
    ))?;
    if event.sequence != 1 {
        return Err(format!("{name}: event sequence mismatch").into());
    }

    let job_id = format!("job-{seed}");
    store.push_job(QueueJob::new(
        QUEUE,
        &job_id,
        r#"{"task":"reliability"}"#,
        3,
    ))?;
    let claimed = store
        .claim_job(QUEUE)?
        .ok_or_else(|| format!("{name}: queue claim failed"))?;
    store
        .ack_job(QUEUE, &claimed.id)?
        .ok_or_else(|| format!("{name}: queue ack failed"))?;

    let link = store.create_link(Link::new(
        format!("reliability/{first_id}"),
        "preflight",
        format!("reliability/object-{seed}-1"),
    ))?;
    if store.count_links()? != 1 || link.link_type != "preflight" {
        return Err(format!("{name}: link state mismatch").into());
    }

    let search_hits = store.search("reliabilityupdated", SearchOptions::default())?;
    if search_hits.len() != 1 {
        return Err(format!("{name}: search update mismatch").into());
    }
    if !store.delete_object("reliability", &first_id)?
        || !store
            .search("reliabilityupdated", SearchOptions::default())?
            .is_empty()
    {
        return Err(format!("{name}: search delete mismatch").into());
    }
    if store.count_objects()? != 15 || store.count_events()? != 1 {
        return Err(format!("{name}: final count mismatch").into());
    }
    Ok(())
}

fn run_concurrent_reliability<S, F>(name: &str, factory: F, seed: u64) -> Result<(), Box<dyn Error>>
where
    S: ObjectStore + Send + 'static,
    F: Fn() -> Result<S, Box<dyn Error>>,
{
    let store = Arc::new(Mutex::new(factory()?));
    {
        let mut guard = store.lock().unwrap();
        for index in 0..32 {
            guard.put_object(MemoryObject::new(
                "concurrent",
                format!("seed-{seed}-{index}"),
                OBJECT_BODY_ACTIVE,
            ))?;
        }
        drop(guard);
    }

    std::thread::scope(|scope| {
        for reader in 0..4 {
            let store = Arc::clone(&store);
            scope.spawn(move || {
                for index in 0..32 {
                    let id = format!("seed-{seed}-{}", (index + reader) % 32);
                    let object = store.lock().unwrap().get_object("concurrent", &id);
                    assert!(object.unwrap().is_some());
                }
            });
        }
        let store = Arc::clone(&store);
        scope.spawn(move || {
            for index in 0..16 {
                store
                    .lock()
                    .unwrap()
                    .put_object(MemoryObject::new(
                        "concurrent",
                        format!("writer-{seed}-{index}"),
                        OBJECT_BODY_INACTIVE,
                    ))
                    .unwrap();
            }
        });
    });

    let count = store.lock().unwrap().count_objects()?;
    if count != 48 {
        return Err(format!("{name}: concurrent count mismatch: {count}").into());
    }
    Ok(())
}

fn measure_samples<F>(count: usize, mut operation: F) -> Result<Vec<u128>, Box<dyn Error>>
where
    F: FnMut(usize) -> Result<(), Box<dyn Error>>,
{
    let mut samples = Vec::with_capacity(count);
    for index in 0..count {
        let started = Instant::now();
        operation(index)?;
        samples.push(started.elapsed().as_nanos());
    }
    Ok(samples)
}

fn record_latency(driver: &str, operation: &str, mut samples: Vec<u128>) {
    samples.sort_unstable();
    let total_ns = samples.iter().sum::<u128>();
    let operations = samples.len();
    let result = BenchResult {
        repetition: CURRENT_REPETITION.load(Ordering::Relaxed),
        driver: driver.to_string(),
        operation: operation.to_string(),
        operations,
        total_ns,
        throughput_ops_per_second: throughput(operations, total_ns),
        min_ns: Some(samples[0]),
        p50_ns: Some(percentile(&samples, 50)),
        p95_ns: Some(percentile(&samples, 95)),
        p99_ns: Some(percentile(&samples, 99)),
        max_ns: Some(*samples.last().unwrap_or(&0)),
        latency_sampled: true,
        error_count: 0,
    };
    println!(
        "{driver:>13} | {operation:<22} | p50={:?} p95={:?} p99={:?} max={:?}",
        nanos_duration(result.p50_ns.unwrap_or_default()),
        nanos_duration(result.p95_ns.unwrap_or_default()),
        nanos_duration(result.p99_ns.unwrap_or_default()),
        nanos_duration(result.max_ns.unwrap_or_default()),
    );
    results().lock().unwrap().results.push(result);
}

const fn percentile(samples: &[u128], percentile: usize) -> u128 {
    let index = ((samples.len() - 1) * percentile).div_ceil(100);
    samples[index]
}

fn update_summaries() {
    let mut output = results().lock().unwrap();
    let mut grouped = std::collections::BTreeMap::<(String, String), Vec<u128>>::new();
    for result in &output.results {
        grouped
            .entry((result.driver.clone(), result.operation.clone()))
            .or_default()
            .push(result.throughput_ops_per_second);
    }
    output.summaries = grouped
        .into_iter()
        .map(|((driver, operation), mut throughputs)| {
            throughputs.sort_unstable();
            let minimum = throughputs[0];
            let maximum = *throughputs.last().unwrap_or(&minimum);
            let middle = throughputs.len() / 2;
            let median = if throughputs.len() % 2 == 0 {
                throughputs[middle - 1].saturating_add(throughputs[middle]) / 2
            } else {
                throughputs[middle]
            };
            BenchSummary {
                driver,
                operation,
                repetitions: throughputs.len(),
                median_throughput_ops_per_second: median,
                minimum_throughput_ops_per_second: minimum,
                maximum_throughput_ops_per_second: maximum,
                spread_throughput_ops_per_second: maximum.saturating_sub(minimum),
            }
        })
        .collect();
}

const fn deterministic_index(seed: u64, index: usize, count: usize) -> usize {
    let mut value = seed ^ index as u64;
    value ^= value >> 12;
    value ^= value << 25;
    value ^= value >> 27;
    #[allow(clippy::cast_possible_truncation)]
    {
        (value.wrapping_mul(2_685_821_657_736_338_717) as usize) % count
    }
}

fn nanos_duration(nanos: u128) -> Duration {
    Duration::from_nanos(u64::try_from(nanos.min(u128::from(u64::MAX))).unwrap_or(u64::MAX))
}

fn bench_concurrent<S, F>(name: &str, factory: F, iterations: usize) -> Result<(), Box<dyn Error>>
where
    S: ObjectStore + Send + 'static,
    F: Fn() -> Result<S, Box<dyn Error>>,
{
    let store = factory()?;
    let shared = Arc::new(Mutex::new(store));

    {
        let mut guard = shared.lock().unwrap();
        for index in 0..iterations {
            let body = OBJECT_BODY_ACTIVE;
            let object = MemoryObject::new(COLLECTION, format!("object-{index}"), body);
            guard.put_object(object)?;
        }
    }

    for &threads in &[1, 2, 4, 8] {
        let (elapsed, actual) = measure_concurrent_reads(&shared, iterations, threads);
        let label = format!("concurrent_read_{threads}t");
        report(name, &label, actual, elapsed);
    }

    let (elapsed, actual) = measure_lock_contention(&shared, iterations);
    report(name, "contention_4r1w", actual, elapsed);

    println!();
    Ok(())
}

fn time_object_puts<S>(store: &mut S, iterations: usize) -> Result<Duration, Box<dyn Error>>
where
    S: ObjectStore,
{
    let started = Instant::now();

    for index in 0..iterations {
        let body = if index % 2 == 0 {
            OBJECT_BODY_ACTIVE
        } else {
            OBJECT_BODY_INACTIVE
        };
        let object = MemoryObject::new(COLLECTION, format!("object-{index}"), body);
        let stored = store.put_object(object)?;
        black_box(stored.version);
    }

    Ok(started.elapsed())
}

fn time_object_put_batch<S>(store: &mut S, iterations: usize) -> Result<Duration, Box<dyn Error>>
where
    S: ObjectStore,
{
    let objects: Vec<MemoryObject> = (0..iterations)
        .map(|index| {
            let body = if index % 2 == 0 {
                OBJECT_BODY_ACTIVE
            } else {
                OBJECT_BODY_INACTIVE
            };
            MemoryObject::new(COLLECTION, format!("batch-{index}"), body)
        })
        .collect();

    let started = Instant::now();
    let results = store.put_objects_batch(objects)?;
    black_box(results.len());
    Ok(started.elapsed())
}

fn time_object_gets<S>(store: &S, iterations: usize) -> Result<Duration, Box<dyn Error>>
where
    S: ObjectStore,
{
    let started = Instant::now();

    for index in 0..iterations {
        let id = format!("object-{index}");
        let object = store.get_object(COLLECTION, black_box(id.as_str()))?;
        black_box(object);
    }

    Ok(started.elapsed())
}

fn time_event_appends<S>(store: &mut S, iterations: usize) -> Result<Duration, Box<dyn Error>>
where
    S: EventLog,
{
    let started = Instant::now();

    for index in 0..iterations {
        let event = MemoryEvent::new(STREAM, "benchmark.event", format!("{EVENT_BODY}:{index}"));
        let stored = store.append_event(event)?;
        black_box(stored.sequence);
    }

    Ok(started.elapsed())
}

fn time_event_append_batch<S>(store: &mut S, iterations: usize) -> Result<Duration, Box<dyn Error>>
where
    S: EventLog,
{
    let events: Vec<MemoryEvent> = (0..iterations)
        .map(|index| {
            MemoryEvent::new(
                STREAM,
                "benchmark.event",
                format!("{EVENT_BODY}:batch-{index}"),
            )
        })
        .collect();

    let started = Instant::now();
    let results = store.append_events_batch(events)?;
    black_box(results.len());
    Ok(started.elapsed())
}

fn time_queue_pushes<S>(store: &mut S, iterations: usize) -> Result<Duration, Box<dyn Error>>
where
    S: QueueStore,
{
    let started = Instant::now();

    for index in 0..iterations {
        let job = QueueJob::new(QUEUE, format!("job-{index}"), format!("payload-{index}"), 3);
        let stored = store.push_job(job)?;
        black_box(stored.status);
    }

    Ok(started.elapsed())
}

fn time_queue_push_batch<S>(store: &mut S, iterations: usize) -> Result<Duration, Box<dyn Error>>
where
    S: QueueStore,
{
    let jobs: Vec<QueueJob> = (0..iterations)
        .map(|index| {
            QueueJob::new(
                QUEUE,
                format!("batch-{index}"),
                format!("payload-{index}"),
                3,
            )
        })
        .collect();

    let started = Instant::now();
    let results = store.push_jobs_batch(jobs)?;
    black_box(results.len());
    Ok(started.elapsed())
}

fn time_queue_claims_and_acks<S>(
    store: &mut S,
    iterations: usize,
) -> Result<Duration, Box<dyn Error>>
where
    S: QueueStore,
{
    let started = Instant::now();

    for _ in 0..iterations {
        if let Some(job) = store.claim_job(QUEUE)? {
            let acked = store.ack_job(QUEUE, &job.id)?;
            black_box(acked);
        }
    }

    Ok(started.elapsed())
}

fn time_queue_claim_and_ack<S>(store: &mut S, iterations: usize) -> Result<Duration, Box<dyn Error>>
where
    S: QueueStore,
{
    let started = Instant::now();

    for _ in 0..iterations {
        let result = store.claim_and_ack(QUEUE, QueueClaimOptions::default())?;
        black_box(result);
    }

    Ok(started.elapsed())
}

fn time_object_deletes<S>(store: &mut S, iterations: usize) -> Result<Duration, Box<dyn Error>>
where
    S: ObjectStore,
{
    let started = Instant::now();

    for index in 0..iterations {
        let id = format!("object-{index}");
        let deleted = store.delete_object(COLLECTION, black_box(id.as_str()))?;
        black_box(deleted);
    }

    Ok(started.elapsed())
}

fn time_search_benchmarks<S>(name: &str, store: &S) -> Result<(), Box<dyn Error>>
where
    S: Searcher,
{
    let search_opts = SearchOptions::default();
    let started = Instant::now();
    let hits = store.search("benchmark", search_opts)?;
    let elapsed = started.elapsed();
    black_box(hits.len());
    report(name, "search", 1, elapsed);

    let filtered_search = SearchOptions {
        collections: Some(vec![COLLECTION.to_string()]),
        limit: Some(10),
        ..Default::default()
    };
    let started = Instant::now();
    let filtered_hits = store.search("benchmark", filtered_search)?;
    let elapsed = started.elapsed();
    black_box(filtered_hits.len());
    report(name, "search_filtered", 1, elapsed);

    Ok(())
}

fn time_vector_benchmarks<S>(
    name: &str,
    store: &mut S,
    iterations: usize,
) -> Result<(), Box<dyn Error>>
where
    S: ObjectStore + VectorStore,
{
    let count = iterations.max(10);
    for index in 0..count {
        #[allow(clippy::cast_precision_loss)]
        let ratio = index as f32 / count as f32;
        let vector = vec![ratio.sin(), ratio.cos(), 0.5, 0.25];
        store.put_object(
            MemoryObject::new("bench_vectors", format!("vec-{index}"), "{}").with_vector(vector),
        )?;
    }
    let started = Instant::now();
    let hits = store.vector_search(
        "bench_vectors",
        &[0.0, 1.0, 0.5, 0.25],
        VectorSearchOptions {
            top_k: Some(10),
            filter: None,
        },
    )?;
    let elapsed = started.elapsed();
    black_box(hits.len());
    report(name, "vector_search_top10", 1, elapsed);
    Ok(())
}

fn time_count_benchmarks<S>(name: &str, store: &S) -> Result<(), Box<dyn Error>>
where
    S: ObjectStore + EventLog,
{
    let started = Instant::now();
    let count = store.count_objects()?;
    let elapsed = started.elapsed();
    black_box(count);
    report(name, "count_objects", 1, elapsed);

    let started = Instant::now();
    let count = store.count_events()?;
    let elapsed = started.elapsed();
    black_box(count);
    report(name, "count_events", 1, elapsed);

    Ok(())
}

fn time_batch_scale_benchmarks<S>(
    name: &str,
    store: &mut S,
    iterations: usize,
) -> Result<(), Box<dyn Error>>
where
    S: ObjectStore,
{
    let batch_sizes = [10usize, 100, 1000]
        .into_iter()
        .filter(|size| *size <= iterations.max(10));

    for size in batch_sizes.clone() {
        let objects: Vec<MemoryObject> = (0..size)
            .map(|i| MemoryObject::new("scale_batch", format!("s-{size}-{i}"), OBJECT_BODY_ACTIVE))
            .collect();

        let started = Instant::now();
        let results = store.put_objects_batch(objects)?;
        let elapsed = started.elapsed();
        black_box(results.len());
        let label = format!("put_batch_{size}");
        report(name, &label, size, elapsed);
    }

    for size in batch_sizes {
        let keys: Vec<(String, String)> = (0..size)
            .map(|i| ("scale_batch".to_string(), format!("s-{size}-{i}")))
            .collect();

        let started = Instant::now();
        let count = store.delete_objects_batch(&keys)?;
        let elapsed = started.elapsed();
        black_box(count);
        let label = format!("delete_batch_{size}");
        report(name, &label, size, elapsed);
    }

    Ok(())
}

fn measure_concurrent_reads<S>(
    store: &Arc<Mutex<S>>,
    total_ops: usize,
    threads: usize,
) -> (Duration, usize)
where
    S: ObjectStore + Send,
{
    let ops_per = total_ops / threads;
    let started = Instant::now();

    std::thread::scope(|scope| {
        for t in 0..threads {
            let store = Arc::clone(store);
            scope.spawn(move || {
                for i in 0..ops_per {
                    let idx = t * ops_per + i;
                    let id = format!("object-{idx}");
                    let object = store.lock().unwrap().get_object(COLLECTION, &id).unwrap();
                    black_box(object);
                }
            });
        }
    });

    let actual = ops_per * threads;
    (started.elapsed(), actual)
}

fn measure_lock_contention<S>(store: &Arc<Mutex<S>>, total_ops: usize) -> (Duration, usize)
where
    S: ObjectStore + Send,
{
    let reader_ops = total_ops / 5;
    let writer_ops = total_ops / 5;
    let started = Instant::now();

    std::thread::scope(|scope| {
        for t in 0..4 {
            let store = Arc::clone(store);
            scope.spawn(move || {
                for i in 0..reader_ops {
                    let idx = t * reader_ops + i;
                    let id = format!("object-{idx}");
                    let object = store.lock().unwrap().get_object(COLLECTION, &id).unwrap();
                    black_box(object);
                }
            });
        }

        let store = Arc::clone(store);
        scope.spawn(move || {
            for i in 0..writer_ops {
                let body = OBJECT_BODY_ACTIVE;
                let object = MemoryObject::new(COLLECTION, format!("contention-{i}"), body);
                let stored = store.lock().unwrap().put_object(object).unwrap();
                black_box(stored.version);
            }
        });
    });

    let actual = reader_ops * 4 + writer_ops;
    (started.elapsed(), actual)
}

fn report(store: &str, operation: &str, iterations: usize, elapsed: Duration) {
    sample_peak_rss();
    let total_ns = elapsed.as_nanos().max(1);
    let operations_per_second = throughput(iterations, total_ns);

    println!(
        "{store:>13} | {operation:<22} | {iterations:>7} ops | {elapsed:>12?} | {operations_per_second:>10} ops/s"
    );
    results().lock().unwrap().results.push(BenchResult {
        repetition: CURRENT_REPETITION.load(Ordering::Relaxed),
        driver: store.to_string(),
        operation: operation.to_string(),
        operations: iterations,
        total_ns,
        throughput_ops_per_second: operations_per_second,
        min_ns: None,
        p50_ns: None,
        p95_ns: None,
        p99_ns: None,
        max_ns: None,
        latency_sampled: false,
        error_count: 0,
    });
}

fn throughput(operations: usize, total_ns: u128) -> u128 {
    operations as u128 * 1_000_000_000 / total_ns.max(1)
}

fn results() -> &'static Mutex<BenchOutput> {
    RESULTS.get().expect("benchmark output is initialized")
}

fn command_output(command: &str, arguments: &[&str]) -> String {
    std::process::Command::new(command)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map_or_else(
            || "unknown".to_string(),
            |output| String::from_utf8_lossy(&output.stdout).trim().to_string(),
        )
}

fn sample_peak_rss() {
    let pid = std::process::id().to_string();
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=,cputime=", "-p", &pid])
        .output();
    let Some((bytes, cpu_ns)) = output
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|value| {
            let mut fields = value.split_whitespace();
            let rss = fields.next()?.parse::<u64>().ok()?;
            let cpu = parse_process_duration_ns(fields.next()?)?;
            Some((rss.saturating_mul(1024), cpu))
        })
    else {
        return;
    };
    PEAK_RSS_BYTES.fetch_max(bytes, Ordering::Relaxed);
    CPU_TIME_NS.fetch_max(cpu_ns, Ordering::Relaxed);
}

fn parse_process_duration_ns(value: &str) -> Option<u64> {
    let parts = value.split(':').collect::<Vec<_>>();
    if !(1..=3).contains(&parts.len()) {
        return None;
    }
    let seconds = parts.last()?.split_once('.');
    let whole_seconds = match seconds {
        Some((whole, _)) => whole.parse::<u64>().ok()?,
        None => parts.last()?.parse::<u64>().ok()?,
    };
    let fraction_ns = seconds
        .and_then(|(_, fraction)| {
            let digits = fraction.as_bytes();
            if digits.iter().any(|digit| !digit.is_ascii_digit()) {
                return None;
            }
            let take = digits.len().min(9);
            let value = std::str::from_utf8(&digits[..take])
                .ok()?
                .parse::<u64>()
                .ok()?;
            Some(value.saturating_mul(10_u64.pow(u32::try_from(9 - take).ok()?)))
        })
        .unwrap_or(0);
    let minutes = parts
        .get(parts.len().saturating_sub(2))
        .map_or(0, |value| value.parse::<u64>().unwrap_or(0));
    let hours = parts
        .get(parts.len().saturating_sub(3))
        .map_or(0, |value| value.parse::<u64>().unwrap_or(0));
    Some(
        hours
            .saturating_mul(3_600_000_000_000)
            .saturating_add(minutes.saturating_mul(60_000_000_000))
            .saturating_add(whole_seconds.saturating_mul(1_000_000_000))
            .saturating_add(fraction_ns),
    )
}

fn cpu_model() -> String {
    let model = command_output("sysctl", &["-n", "machdep.cpu.brand_string"]);
    if model != "unknown" {
        return model;
    }
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|value| {
            value.lines().find_map(|line| {
                line.strip_prefix("model name:")
                    .map(|model| model.trim().to_string())
            })
        })
        .unwrap_or_else(|| "unsupported: CPU model unavailable".to_string())
}

fn filesystem_type() -> String {
    let path = env::current_dir().map_or_else(|_| ".".into(), |path| path.display().to_string());
    let output = std::process::Command::new("df")
        .args(["-P", &path])
        .output();
    output
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|value| value.lines().nth(1).map(str::to_owned))
        .and_then(|line| line.split_whitespace().next().map(str::to_owned))
        .unwrap_or_else(|| "unsupported: filesystem type unavailable".to_string())
}

fn record_storage(driver: &str, path: &Path) {
    let (bytes_on_disk, file_count) = directory_stats(path);
    results().lock().unwrap().storage.push(StorageSnapshot {
        driver: driver.to_string(),
        repetition: CURRENT_REPETITION.load(Ordering::Relaxed),
        path: path.display().to_string(),
        bytes_on_disk,
        file_count,
        filesystem_artifacts: file_count,
        in_memory: false,
    });
    println!("{driver:>13} | storage_snapshot       | bytes={bytes_on_disk} files={file_count}");
}

fn record_memory_storage(driver: &str) {
    results().lock().unwrap().storage.push(StorageSnapshot {
        driver: driver.to_string(),
        repetition: CURRENT_REPETITION.load(Ordering::Relaxed),
        path: "<memory>".to_string(),
        bytes_on_disk: 0,
        file_count: 0,
        filesystem_artifacts: 0,
        in_memory: true,
    });
    println!("{driver:>13} | storage_snapshot       | bytes=0 files=0 (in-memory)");
}

fn directory_stats(path: &Path) -> (u64, usize) {
    let mut bytes = 0;
    let mut files = 0;
    let Ok(entries) = fs::read_dir(path) else {
        return (0, 0);
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if let Ok(metadata) = entry.metadata() {
            if metadata.is_dir() {
                let (nested_bytes, nested_files) = directory_stats(&entry_path);
                bytes += nested_bytes;
                files += nested_files;
            } else {
                bytes += metadata.len();
                files += 1;
            }
        }
    }
    (bytes, files)
}

fn write_output(path: &Path) -> Result<(), Box<dyn Error>> {
    sample_peak_rss();
    if let Ok(mut output) = results().lock() {
        output.metadata.peak_rss_bytes = match PEAK_RSS_BYTES.load(Ordering::Relaxed) {
            0 => {
                output.metadata.peak_rss_status =
                    "unsupported: process RSS sampling unavailable".to_string();
                None
            },
            bytes => {
                output.metadata.peak_rss_status = "measured: ps rss".to_string();
                Some(bytes)
            },
        };
        output.metadata.cpu_time_ns = match CPU_TIME_NS.load(Ordering::Relaxed) {
            0 => {
                output.metadata.cpu_time_status =
                    "unsupported: process CPU time sampling unavailable".to_string();
                None
            },
            nanos => {
                output.metadata.cpu_time_status = "measured: ps cputime".to_string();
                Some(nanos)
            },
        };
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    if path.extension().and_then(|extension| extension.to_str()) == Some("csv") {
        let csv = {
            let output = results().lock().unwrap();
            let mut csv = String::from(
                "date,phase,branch,commit,rust,os,arch,cpu_model,filesystem,iterations,repetitions,seed,backend,peak_rss_bytes,peak_rss_status,cpu_time_ns,cpu_time_status,repetition,driver,operation,operations,total_ns,throughput_ops_per_second,min_ns,p50_ns,p95_ns,p99_ns,max_ns,latency_sampled,error_count\n",
            );
            for result in &output.results {
                let fields = [
                    csv_escape(&output.metadata.date),
                    csv_escape(&output.metadata.phase),
                    csv_escape(&output.metadata.branch),
                    csv_escape(&output.metadata.commit),
                    csv_escape(&output.metadata.rust),
                    csv_escape(&output.metadata.os),
                    csv_escape(&output.metadata.arch),
                    csv_escape(&output.metadata.cpu_model),
                    csv_escape(&output.metadata.filesystem),
                    output.metadata.iterations.to_string(),
                    output.metadata.repetitions.to_string(),
                    output.metadata.seed.to_string(),
                    csv_escape(&output.metadata.backend),
                    option_csv_u64(output.metadata.peak_rss_bytes),
                    csv_escape(&output.metadata.peak_rss_status),
                    option_csv_u64(output.metadata.cpu_time_ns),
                    csv_escape(&output.metadata.cpu_time_status),
                    result.repetition.to_string(),
                    csv_escape(&result.driver),
                    csv_escape(&result.operation),
                    result.operations.to_string(),
                    result.total_ns.to_string(),
                    result.throughput_ops_per_second.to_string(),
                    option_csv(result.min_ns),
                    option_csv(result.p50_ns),
                    option_csv(result.p95_ns),
                    option_csv(result.p99_ns),
                    option_csv(result.max_ns),
                    result.latency_sampled.to_string(),
                    result.error_count.to_string(),
                ];
                let _ = writeln!(csv, "{}", fields.join(","));
            }
            drop(output);
            csv
        };
        fs::write(path, csv)?;
    } else {
        let serialized = serde_json::to_string_pretty(&*results().lock().unwrap())?;
        fs::write(path, serialized)?;
    }
    Ok(())
}

fn append_history(path: &Path) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(&*results().lock().unwrap())?;
    let mut history = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(history, "{line}")?;
    Ok(())
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn option_csv(value: Option<u128>) -> String {
    value.map_or_else(String::new, |value| value.to_string())
}

fn option_csv_u64(value: Option<u64>) -> String {
    value.map_or_else(String::new, |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::{option_csv, parse_process_duration_ns};

    #[test]
    fn parses_process_cpu_time_without_floating_point_rounding() {
        assert_eq!(parse_process_duration_ns("00:00.50"), Some(500_000_000));
        assert_eq!(
            parse_process_duration_ns("01:02:03.25"),
            Some(3_723_250_000_000)
        );
    }

    #[test]
    fn leaves_unsampled_percentiles_empty_in_csv() {
        assert_eq!(option_csv(None), "");
        assert_eq!(option_csv(Some(42)), "42");
    }
}
