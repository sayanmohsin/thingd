#![allow(unused_crate_dependencies)]

//! Durable `ThingDB` example. Set `THINGDB_EXAMPLE_PATH` to choose the path.

use std::env;
use std::path::PathBuf;

use thingdb::{Database, KeyspaceCreateOptions, PersistMode};

fn main() -> thingdb::Result<()> {
    let path = env::var_os("THINGDB_EXAMPLE_PATH").map_or_else(
        || env::temp_dir().join("thingdb-durable-example"),
        PathBuf::from,
    );
    let db = Database::open(&path)?;
    let events = db.keyspace("events", KeyspaceCreateOptions::default)?;
    events.insert(b"event-1", b"created")?;
    db.persist(PersistMode::SyncAll)?;
    println!(
        "stored {}",
        String::from_utf8_lossy(&events.get(b"event-1")?.unwrap_or_default())
    );
    Ok(())
}
