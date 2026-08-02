use std::env;
use thingd::PersistentEngine;

fn main() {
    let path = env::var("THINGD_PATH").unwrap_or_else(|_| "data.db".to_string());
    println!("Opening {path}...");
    let store = PersistentEngine::open(&path).unwrap_or_else(|e| {
        eprintln!("Failed to open database: {e}");
        std::process::exit(1);
    });
    println!("Opened.");
    drop(store);
}
