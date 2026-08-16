//! Local storage benchmark for thingd adapters.
//!
//! Usage:
//!   cargo run --example `storage_bench` --release --features persistent,search [<iterations>]
//!   `THINGD_BENCH_ITERS=10000` cargo run --example `storage_bench` --release --features persistent,search

#![allow(unused_crate_dependencies)]

use std::env;
use std::error::Error;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::hint::black_box;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

use serde::Serialize;
use thingd::{
    EncryptionConfig, EventLog, ListEventsOptions, ListObjectsOptions, MemoryEngine, MemoryEvent,
    MemoryObject, ObjectStore, PersistentEngine, PersistentOpenOptions, QueueClaimOptions,
    QueueJob, QueueStore, SearchOptions, Searcher, VectorSearchOptions, VectorStore,
};

const DEFAULT_ITERATIONS: usize = 5_000;
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
    RocksDb,
    ThingDb,
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
}

#[derive(Clone, Debug, Serialize)]
struct BenchResult {
    repetition: usize,
    driver: String,
    operation: String,
    operations: usize,
    total_ns: u128,
    throughput_ops_per_second: u128,
    min_ns: u128,
    p50_ns: u128,
    p95_ns: u128,
    p99_ns: u128,
    max_ns: u128,
    error_count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct StorageSnapshot {
    driver: String,
    repetition: usize,
    path: String,
    bytes_on_disk: u64,
    file_count: usize,
}

#[derive(Debug, Serialize)]
struct BenchOutput {
    metadata: BenchMetadata,
    results: Vec<BenchResult>,
    summaries: Vec<BenchSummary>,
    storage: Vec<StorageSnapshot>,
    wal: Vec<WalSnapshot>,
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
}

static RESULTS: std::sync::OnceLock<Mutex<BenchOutput>> = std::sync::OnceLock::new();
static CURRENT_REPETITION: AtomicUsize = AtomicUsize::new(0);

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
            },
            results: Vec::new(),
            summaries: Vec::new(),
            storage: Vec::new(),
            wal: Vec::new(),
        }))
        .map_err(|_| "benchmark output was initialized more than once")?;

    let iterations = config.iterations;

    println!("thingd storage benchmark");
    println!("iterations: {iterations}");
    println!("seed: {}", config.seed);
    println!("backend: {}", config.backend.name());
    println!("repetitions: {}", config.repetitions);
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
            bench_store("in-memory", MemoryEngine::new(), iterations, config.seed)?;
            bench_concurrent("in-memory", || Ok(MemoryEngine::new()), iterations)?;
        }

        if config.backend.includes(BackendSelection::RocksDb) {
            let dir = tempfile::tempdir()?;
            let persistent_options =
                benchmark_persistent_options(thingd::PersistentBackend::RocksDb);
            let persistent_engine =
                PersistentEngine::open_with_options(dir.path(), persistent_options.clone())?;
            let lifecycle_dir = tempfile::tempdir()?;
            time_persistent_lifecycle(lifecycle_dir.path(), persistent_options.clone())?;
            bench_store("persistent", persistent_engine, iterations, config.seed)?;
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
            )?;
            record_storage("thingdb-experimental", thingdb_dir.path());
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
    }

    run_correctness_smoke(config.seed)?;
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
                "--backend" => {
                    backend = match value("--backend")?.as_str() {
                        "all" => BackendSelection::All,
                        "memory" => BackendSelection::InMemory,
                        "rocksdb" => BackendSelection::RocksDb,
                        "thingdb" => BackendSelection::ThingDb,
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
        })
    }
}

impl BackendSelection {
    fn includes(self, backend: Self) -> bool {
        self == Self::All || self == backend
    }

    const fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::InMemory => "memory",
            Self::RocksDb => "rocksdb",
            Self::ThingDb => "thingdb",
        }
    }
}

fn bench_store<S>(
    name: &str,
    mut store: S,
    iterations: usize,
    _seed: u64,
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

    let elapsed = time_queue_pushes(&mut store, iterations)?;
    report(name, "queue_push", iterations, elapsed);

    let elapsed = time_queue_push_batch(&mut store, iterations)?;
    report(name, "queue_batch", iterations, elapsed);

    let elapsed = time_queue_claims_and_acks(&mut store, iterations)?;
    report(name, "queue_claim_ack", iterations, elapsed);

    let elapsed = time_queue_claim_and_ack(&mut store, iterations)?;
    report(name, "queue_claim_ack2", iterations, elapsed);

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

fn bench_wal_workloads(
    name: &str,
    path: &Path,
    mut options: PersistentOpenOptions,
    iterations: usize,
    seed: u64,
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
    let reopened = PersistentEngine::open_with_options(path, options)?;
    report(name, "wal-recovery", 1, started.elapsed());
    let after_reopen = reopened
        .wal_diagnostics()?
        .ok_or("ThingDB diagnostics unavailable after reopen")?;
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
            last_error: after.last_error.or(before.last_error),
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
        min_ns: samples[0],
        p50_ns: percentile(&samples, 50),
        p95_ns: percentile(&samples, 95),
        p99_ns: percentile(&samples, 99),
        max_ns: *samples.last().unwrap_or(&0),
        error_count: 0,
    };
    println!(
        "{driver:>13} | {operation:<22} | p50={:?} p95={:?} p99={:?} max={:?}",
        nanos_duration(result.p50_ns),
        nanos_duration(result.p95_ns),
        nanos_duration(result.p99_ns),
        nanos_duration(result.max_ns),
    );
    results().lock().unwrap().results.push(result);
}

fn percentile(samples: &[u128], percentile: usize) -> u128 {
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
        min_ns: total_ns / iterations.max(1) as u128,
        p50_ns: total_ns / iterations.max(1) as u128,
        p95_ns: total_ns / iterations.max(1) as u128,
        p99_ns: total_ns / iterations.max(1) as u128,
        max_ns: total_ns / iterations.max(1) as u128,
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

fn record_storage(driver: &str, path: &Path) {
    let (bytes_on_disk, file_count) = directory_stats(path);
    results().lock().unwrap().storage.push(StorageSnapshot {
        driver: driver.to_string(),
        repetition: CURRENT_REPETITION.load(Ordering::Relaxed),
        path: path.display().to_string(),
        bytes_on_disk,
        file_count,
    });
    println!("{driver:>13} | storage_snapshot       | bytes={bytes_on_disk} files={file_count}");
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
                "commit,rust,os,arch,iterations,repetitions,seed,backend,repetition,driver,operation,operations,total_ns,throughput_ops_per_second,min_ns,p50_ns,p95_ns,p99_ns,max_ns,error_count\n",
            );
            for result in &output.results {
                let _ = writeln!(
                    csv,
                    "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    csv_escape(&output.metadata.commit),
                    csv_escape(&output.metadata.rust),
                    output.metadata.os,
                    output.metadata.arch,
                    output.metadata.iterations,
                    output.metadata.repetitions,
                    output.metadata.seed,
                    output.metadata.backend,
                    result.repetition,
                    csv_escape(&result.driver),
                    csv_escape(&result.operation),
                    result.operations,
                    result.total_ns,
                    result.throughput_ops_per_second,
                    result.min_ns,
                    result.p50_ns,
                    result.p95_ns,
                    result.p99_ns,
                    result.max_ns,
                    result.error_count,
                );
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
