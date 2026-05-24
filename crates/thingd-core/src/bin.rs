use thingd_core::SqliteThingStore;

fn main() {
    println!("Opening db...");
    let store = SqliteThingStore::open("/Users/sayanmohsin/Downloads/data.db").unwrap();
    println!("Opened.");
}
