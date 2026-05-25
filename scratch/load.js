import sqlite3 from "sqlite3";

const db = new sqlite3.Database("scratch/test.db");
db.serialize(() => {
  db.run(
    "CREATE TABLE objects (collection TEXT NOT NULL, id TEXT NOT NULL, body TEXT NOT NULL, version INTEGER NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY (collection, id))",
  );
  db.run("BEGIN TRANSACTION");
  for (let i = 0; i < 500000; i++) {
    db.run("INSERT INTO objects VALUES ('col1', 'id' || ?, '{}', 1, 'now', 'now')", [i]);
  }
  db.run("COMMIT");
});
