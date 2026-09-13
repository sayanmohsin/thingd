#![allow(unused_crate_dependencies)]

//! Small RAM-only example for users of the standalone `thingdb` crate.

use thingdb::{Database, KeyspaceCreateOptions};

fn main() -> thingdb::Result<()> {
    let db = Database::builder(":memory:").open()?;
    let users = db.keyspace("users", KeyspaceCreateOptions::default)?;

    users.insert(b"alice", br#"{"active":true}"#)?;
    db.batch()
        .put(&users, b"bob", br#"{"active":false}"#)
        .delete(&users, b"alice")
        .commit()?;

    for entry in users.prefix(b"b") {
        println!(
            "{} = {}",
            String::from_utf8_lossy(&entry.key),
            String::from_utf8_lossy(&entry.value)
        );
    }

    let snapshot = db.snapshot()?;
    assert_eq!(
        snapshot.keyspace("users").get(b"bob"),
        Some(br#"{"active":false}"#.to_vec())
    );
    Ok(())
}
