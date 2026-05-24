use rusqlite::Connection;

fn main() {
    println!("Opening db...");
    let conn = Connection::open("/Users/sayanmohsin/Downloads/data.db").unwrap();
    println!("Opened connection.");
    
    conn.execute_batch(
        r"
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;
        "
    ).unwrap();
    println!("Executed pragma.");
}
