//! Local storage benchmark for thingd-core adapters.
//!
//! Usage:
//!   cargo run --example storage_bench --release --all-features [<iterations>]
//!   THINGD_BENCH_ITERS=10000 cargo run --example storage_bench --release --all-features

use std::env;
use std::error::Error;
use std::hint::black_box;
use std::time::{Duration, Instant};

use chrono as _;
#[cfg(feature = "connectors")]
use csv as _;
use rusqlite as _;
use serde_json as _;
use thingd_core::{
    EventLog, ListEventsOptions, ListObjectsOptions, MemoryEngine, MemoryEvent, MemoryObject,
    ObjectStore, QueueClaimOptions, QueueJob, QueueStore, SqliteThingStore,
};
use uuid as _;

const DEFAULT_ITERATIONS: usize = 5_000;
const COLLECTION: &str = "bench_objects";
const QUEUE: &str = "bench_queue";
const STREAM: &str = "bench:events";

/// Object body with a `status` field so we can benchmark filtered list_objects.
const OBJECT_BODY_ACTIVE: &str =
    r#"{"text":"benchmark object","project":"thingd","status":"active","confidence":0.95}"#;
const OBJECT_BODY_INACTIVE: &str =
    r#"{"text":"benchmark object","project":"thingd","status":"inactive","confidence":0.10}"#;
const EVENT_BODY: &str = r#"{"text":"benchmark event","project":"thingd","actor":"benchmark"}"#;

fn main() -> Result<(), Box<dyn Error>> {
    let iterations = iterations();

    println!("thingd storage benchmark");
    println!("iterations: {iterations}");
    println!();
    println!(
        "{:>13} | {:<22} | {:>7} | {:>12} | {:>12}",
        "driver", "operation", "ops", "total", "ops/s"
    );
    println!("{}", "-".repeat(80));

    bench_store("in-memory", MemoryEngine::new(), iterations)?;
    bench_store(
        "sqlite-memory",
        SqliteThingStore::open_in_memory()?,
        iterations,
    )?;

    let directory = tempfile::tempdir()?;
    let database_path = directory.path().join("thingd-bench.db");
    bench_store(
        "sqlite-file",
        SqliteThingStore::open(database_path)?,
        iterations,
    )?;

    Ok(())
}

fn iterations() -> usize {
    if let Some(value) = env::args().nth(1) {
        return value.parse().unwrap_or(DEFAULT_ITERATIONS);
    }

    env::var("THINGD_BENCH_ITERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_ITERATIONS)
}

fn bench_store<S>(name: &str, mut store: S, iterations: usize) -> Result<(), Box<dyn Error>>
where
    S: EventLog + ObjectStore + QueueStore,
{
    // ── Object writes ──────────────────────────────────────────────────────
    let elapsed = time_object_puts(&mut store, iterations)?;
    report(name, "object_put", iterations, elapsed);

    let elapsed = time_object_put_batch(&mut store, iterations)?;
    report(name, "object_batch", iterations, elapsed);

    // ── Object reads ───────────────────────────────────────────────────────
    let elapsed = time_object_gets(&store, iterations)?;
    report(name, "object_get", iterations, elapsed);

    // ── list_objects: no options (full collection) ─────────────────────────
    let started = Instant::now();
    let all_objects = store.list_objects(None, &ListObjectsOptions::default())?;
    let elapsed = started.elapsed();
    black_box(all_objects.len());
    report(name, "list_objects", all_objects.len(), elapsed);

    // ── list_objects: filtered by status="active" (json_extract pushdown) ──
    let filter_opts = ListObjectsOptions {
        filter: vec![("status".into(), serde_json::json!("active"))],
        ..Default::default()
    };
    let started = Instant::now();
    let filtered = store.list_objects(None, &filter_opts)?;
    let elapsed = started.elapsed();
    black_box(filtered.len());
    report(name, "list_objects_filter", filtered.len().max(1), elapsed);

    // ── list_objects: limit 100 ────────────────────────────────────────────
    let limit_opts = ListObjectsOptions {
        limit: Some(100),
        ..Default::default()
    };
    let started = Instant::now();
    let limited = store.list_objects(Some(&[COLLECTION.to_string()]), &limit_opts)?;
    let elapsed = started.elapsed();
    black_box(limited.len());
    report(name, "list_objects_limit100", limited.len().max(1), elapsed);

    // ── list_objects: limit 100 + offset 50 ───────────────────────────────
    let paginate_opts = ListObjectsOptions {
        limit: Some(100),
        offset: Some(50),
        ..Default::default()
    };
    let started = Instant::now();
    let page = store.list_objects(Some(&[COLLECTION.to_string()]), &paginate_opts)?;
    let elapsed = started.elapsed();
    black_box(page.len());
    report(name, "list_objects_page", page.len().max(1), elapsed);

    // ── Events ─────────────────────────────────────────────────────────────
    let elapsed = time_event_appends(&mut store, iterations)?;
    report(name, "event_append", iterations, elapsed);

    let elapsed = time_event_append_batch(&mut store, iterations)?;
    report(name, "event_batch", iterations, elapsed);

    // event_list: all events in stream
    let started = Instant::now();
    let events = store.list_events(Some(STREAM), ListEventsOptions::default())?;
    let elapsed = started.elapsed();
    black_box(events.len());
    report(name, "event_list", events.len(), elapsed);

    // event_list: from halfway through (fromSequence pushdown)
    let midpoint = events.len() as u64 / 2;
    let from_opts = ListEventsOptions {
        from_sequence: Some(midpoint),
        limit: None,
    };
    let started = Instant::now();
    let tail = store.list_events(Some(STREAM), from_opts)?;
    let elapsed = started.elapsed();
    black_box(tail.len());
    report(name, "event_list_from_seq", tail.len().max(1), elapsed);

    // event_list: limited to 100
    let limit_event_opts = ListEventsOptions {
        from_sequence: None,
        limit: Some(100),
    };
    let started = Instant::now();
    let limited_events = store.list_events(Some(STREAM), limit_event_opts)?;
    let elapsed = started.elapsed();
    black_box(limited_events.len());
    report(name, "event_list_limit100", limited_events.len().max(1), elapsed);

    // ── Queues ─────────────────────────────────────────────────────────────
    let elapsed = time_queue_pushes(&mut store, iterations)?;
    report(name, "queue_push", iterations, elapsed);

    let elapsed = time_queue_push_batch(&mut store, iterations)?;
    report(name, "queue_batch", iterations, elapsed);

    let elapsed = time_queue_claims_and_acks(&mut store, iterations)?;
    report(name, "queue_claim_ack", iterations, elapsed);

    let elapsed = time_queue_claim_and_ack(&mut store, iterations)?;
    report(name, "queue_claim_ack2", iterations, elapsed);

    println!();
    Ok(())
}

fn time_object_puts<S>(store: &mut S, iterations: usize) -> Result<Duration, Box<dyn Error>>
where
    S: ObjectStore,
{
    let started = Instant::now();

    for index in 0..iterations {
        // Alternate active/inactive so filtered benchmarks have data on both sides.
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

fn time_event_append_batch<S>(
    store: &mut S,
    iterations: usize,
) -> Result<Duration, Box<dyn Error>>
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

fn time_queue_claim_and_ack<S>(
    store: &mut S,
    iterations: usize,
) -> Result<Duration, Box<dyn Error>>
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

fn report(store: &str, operation: &str, iterations: usize, elapsed: Duration) {
    let elapsed_micros = elapsed.as_micros().max(1);
    let operations_per_second = iterations as u128 * 1_000_000 / elapsed_micros;

    println!(
        "{store:>13} | {operation:<22} | {iterations:>7} ops | {elapsed:>12?} | {operations_per_second:>10} ops/s"
    );
}
