//! Local storage benchmark for thingd adapters.
//!
//! Usage:
//!   cargo run --example storage_bench --release --features fjall,search [<iterations>]
//!   THINGD_BENCH_ITERS=10000 cargo run --example storage_bench --release --features fjall,search

#![allow(unused_crate_dependencies)]

use std::env;
use std::error::Error;
use std::hint::black_box;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use thingd::{
    EventLog, FjallEngine, ListEventsOptions, ListObjectsOptions, MemoryEngine, MemoryEvent,
    MemoryObject, ObjectStore, QueueClaimOptions, QueueJob, QueueStore, SearchOptions, Searcher,
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
    bench_concurrent("in-memory", || Ok(MemoryEngine::new()), iterations)?;

    let dir = tempfile::tempdir()?;
    let fjall_engine = FjallEngine::open(dir.path())?;
    bench_store("fjall", fjall_engine, iterations)?;

    let conc_dir = tempfile::tempdir()?;
    bench_concurrent(
        "fjall",
        || {
            let engine = FjallEngine::open(conc_dir.path())?;
            Ok(engine)
        },
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
    S: EventLog + ObjectStore + QueueStore + Searcher,
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
    report(name, "list_objects", all_objects.len(), elapsed);

    let filter_opts = ListObjectsOptions {
        filter: vec![("status".into(), serde_json::json!("active"))],
        ..Default::default()
    };
    let started = Instant::now();
    let filtered = store.list_objects(None, &filter_opts)?;
    let elapsed = started.elapsed();
    black_box(filtered.len());
    report(name, "list_objects_filter", filtered.len().max(1), elapsed);

    let limit_opts = ListObjectsOptions {
        limit: Some(100),
        ..Default::default()
    };
    let started = Instant::now();
    let limited = store.list_objects(Some(&[COLLECTION.to_string()]), &limit_opts)?;
    let elapsed = started.elapsed();
    black_box(limited.len());
    report(name, "list_objects_limit100", limited.len().max(1), elapsed);

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

    let elapsed = time_event_appends(&mut store, iterations)?;
    report(name, "event_append", iterations, elapsed);

    let elapsed = time_event_append_batch(&mut store, iterations)?;
    report(name, "event_batch", iterations, elapsed);

    let started = Instant::now();
    let events = store.list_events(Some(STREAM), ListEventsOptions::default())?;
    let elapsed = started.elapsed();
    black_box(events.len());
    report(name, "event_list", events.len(), elapsed);

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
    report(name, "event_list_from_seq", tail.len().max(1), elapsed);

    let limit_event_opts = ListEventsOptions {
        from_sequence: None,
        limit: Some(100),
        since: None,
    };
    let started = Instant::now();
    let limited_events = store.list_events(Some(STREAM), limit_event_opts)?;
    let elapsed = started.elapsed();
    black_box(limited_events.len());
    report(
        name,
        "event_list_limit100",
        limited_events.len().max(1),
        elapsed,
    );

    let elapsed = time_queue_pushes(&mut store, iterations)?;
    report(name, "queue_push", iterations, elapsed);

    let elapsed = time_queue_push_batch(&mut store, iterations)?;
    report(name, "queue_batch", iterations, elapsed);

    let elapsed = time_queue_claims_and_acks(&mut store, iterations)?;
    report(name, "queue_claim_ack", iterations, elapsed);

    let elapsed = time_queue_claim_and_ack(&mut store, iterations)?;
    report(name, "queue_claim_ack2", iterations, elapsed);

    time_search_benchmarks(name, &store)?;

    time_batch_scale_benchmarks(name, &mut store)?;

    time_count_benchmarks(name, &store)?;

    let elapsed = time_object_deletes(&mut store, iterations)?;
    report(name, "object_delete", iterations, elapsed);

    println!();
    Ok(())
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
    report(name, "search", hits.len().max(1), elapsed);

    let filtered_search = SearchOptions {
        collections: Some(vec![COLLECTION.to_string()]),
        limit: Some(10),
        ..Default::default()
    };
    let started = Instant::now();
    let filtered_hits = store.search("benchmark", filtered_search)?;
    let elapsed = started.elapsed();
    black_box(filtered_hits.len());
    report(name, "search_filtered", filtered_hits.len().max(1), elapsed);

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

fn time_batch_scale_benchmarks<S>(name: &str, store: &mut S) -> Result<(), Box<dyn Error>>
where
    S: ObjectStore,
{
    let batch_sizes = [10usize, 100, 1000];

    for &size in &batch_sizes {
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

    for &size in &batch_sizes {
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
    let elapsed_micros = elapsed.as_micros().max(1);
    let operations_per_second = iterations as u128 * 1_000_000 / elapsed_micros;

    println!(
        "{store:>13} | {operation:<22} | {iterations:>7} ops | {elapsed:>12?} | {operations_per_second:>10} ops/s"
    );
}
