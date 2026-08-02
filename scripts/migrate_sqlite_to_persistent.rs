/// One-shot migration: reads all data from an existing SQLite file and writes it to persistent.
///
/// Usage:
///   cargo run --example migrate_sqlite_to_persistent -- /path/to/data.sqlite /path/to/persistent-dir
///
/// This is intended to be run ONCE per database. After migration, use the persistent directory.
///
/// Note: This script requires the old sqlite feature temporarily.
/// Temporarily add to Cargo.toml:
///   sqlite = ["dep:rusqlite", "dep:uuid"]
/// And add to [dependencies]:
///   rusqlite = { version = "0.40.1", features = ["bundled"] }
///   uuid = { version = "1", features = ["v4"] }

use std::path::Path;

use thingd::{
    PersistentEngine, MemoryEvent, MemoryObject, QueueJob,
    store::{EventLog, LinkStore, ObjectStore, QueueStore},
    ListEventsOptions, ListObjectsOptions,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <sqlite-path> <persistent-dir>", args[0]);
        std::process::exit(1);
    }

    let sqlite_path = &args[1];
    let persistent_path = &args[2];

    if !Path::new(sqlite_path).exists() {
        eprintln!("Error: SQLite file not found: {sqlite_path}");
        std::process::exit(1);
    }

    if Path::new(persistent_path).exists() {
        eprintln!("Error: persistent directory already exists at {persistent_path}. Remove it first or use a different path.");
        std::process::exit(1);
    }

    println!("Opening SQLite: {sqlite_path}");
    let source = SqliteThingStore::open(sqlite_path).expect("Failed to open SQLite");

    println!("Creating persistent: {persistent_path}");
    let mut dest = PersistentEngine::open(persistent_path).expect("Failed to create persistent");

    // Migrate objects
    println!("Migrating objects...");
    let collections = source.list_collections().expect("Failed to list collections");
    for collection in &collections {
        let mut offset = 0u64;
        loop {
            let batch = source
                .list_objects(
                    Some(&[collection.clone()]),
                    &ListObjectsOptions {
                        limit: Some(100),
                        offset: Some(offset),
                        ..Default::default()
                    },
                )
                .expect("Failed to list objects");
            if batch.is_empty() {
                break;
            }
            let count = batch.len();
            dest.put_objects_batch(batch)
                .expect("Failed to write objects to persistent");
            offset += count as u64;
            print!("  {collection}: {offset} objects\r");
        }
        println!("  {collection}: {offset} objects done");
    }

    // Migrate events
    println!("Migrating events...");
    let streams = source.list_streams().expect("Failed to list streams");
    for stream in &streams {
        let mut seq = 0u64;
        loop {
            let batch = source
                .list_events(
                    Some(stream),
                    ListEventsOptions {
                        from_sequence: Some(seq),
                        limit: Some(100),
                        ..Default::default()
                    },
                )
                .expect("Failed to list events");
            if batch.is_empty() {
                break;
            }
            seq = batch.last().map(|e| e.sequence).unwrap_or(seq);
            let count = batch.len();
            dest.append_events_batch(batch)
                .expect("Failed to write events to persistent");
            print!("  {stream}: {count} events\r");
        }
        println!("  {stream}: events done");
    }

    // Migrate queues
    println!("Migrating queues...");
    let queues = source.list_queues().expect("Failed to list queues");
    for queue in &queues {
        let jobs = source.list_jobs(queue).expect("Failed to list jobs");
        if jobs.is_empty() {
            continue;
        }
        dest.push_jobs_batch(jobs)
            .expect("Failed to write jobs to persistent");
        println!("  {queue}: done");
    }

    // Migrate links (skip — links have auto-generated IDs that won't match)
    let link_count = source.count_links().unwrap_or(0);
    if link_count > 0 {
        println!("Note: {link_count} links were NOT migrated (link IDs are auto-generated).");
        println!("  Links created post-migration will work normally.");
    }

    println!("Migration complete.");
    println!("Source: {sqlite_path}");
    println!("Dest:   {persistent_path}");
    println!("");
    println!("Verify your data, then delete the old SQLite file.");
}

// We need SqliteThingStore for the migration
use thingd::SqliteThingStore;
