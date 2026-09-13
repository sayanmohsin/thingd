#![allow(unused_crate_dependencies)]

//! Bounded process-local cache example.

use thingdb::{CacheOptions, MemoryCache};

fn main() -> thingdb::Result<()> {
    let cache = MemoryCache::new(CacheOptions {
        max_entries: 128,
        max_bytes: 1024 * 1024,
        ..CacheOptions::default()
    })?;
    cache.insert(b"greeting", b"hello")?;
    assert_eq!(cache.get(b"greeting")?, Some(b"hello".to_vec()));
    println!("{} entries", cache.stats()?.current_entries);
    Ok(())
}
