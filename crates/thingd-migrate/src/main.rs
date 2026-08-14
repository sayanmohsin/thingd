//! Offline migration from a legacy Fjall Thingd directory to `RocksDB`.
//!
//! The utility is intentionally separate from `thingd-server` and the native
//! addon. Fjall is linked only here for a one-time migration and is never a
//! runtime dependency of the new engine.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use fjall::{Database, KeyspaceCreateOptions};
use rocksdb::{ColumnFamilyDescriptor, DB, Options, WriteBatch, WriteOptions};

const KEYSPACES: &[&str] = &[
    "objects",
    "events",
    "event_meta",
    "queue_jobs",
    "ready_jobs",
    "links_by_id",
    "links_from",
    "links_to",
    "schemas",
    "migrations",
    "indexes",
    "vectors",
];

fn main() {
    if let Err(error) = run() {
        eprintln!("ERROR: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
        return Ok(());
    };
    if command != "fjall-to-rocksdb" {
        return Err(format!("unknown command: {command}").into());
    }
    let source = required_path(&mut args, "--source")?;
    let destination = required_path(&mut args, "--destination")?;
    let encryption_key = optional_value(&mut args, "--encryption-key")?;
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    migrate(&source, &destination, encryption_key.as_deref())
}

fn usage() {
    println!(
        "Usage: thingd-migrate fjall-to-rocksdb --source <path> --destination <path> [--encryption-key <64-hex>]"
    );
}

fn required_path(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    optional_value(args, flag)?
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing {flag}").into())
}

fn optional_value(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<Option<String>, Box<dyn Error>> {
    let Some(value) = args.next() else {
        return Ok(None);
    };
    if value == flag {
        return args
            .next()
            .map(Some)
            .ok_or_else(|| format!("missing value for {flag}").into());
    }
    if let Some(value) = value.strip_prefix(&format!("{flag}=")) {
        return Ok(Some(value.to_string()));
    }
    Err(format!("expected {flag}, found {value}").into())
}

fn migrate(
    source: &Path,
    destination: &Path,
    encryption_key: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    validate_paths(source, destination)?;
    let source_db = Database::builder(source).open()?;
    let partial = partial_destination(destination)?;
    fs::create_dir_all(&partial)?;

    let result = (|| -> Result<(), Box<dyn Error>> {
        let mut options = Options::default();
        options.create_if_missing(true);
        options.create_missing_column_families(true);
        let descriptors =
            std::iter::once(ColumnFamilyDescriptor::new("default", Options::default()))
                .chain(
                    KEYSPACES
                        .iter()
                        .map(|name| ColumnFamilyDescriptor::new(*name, Options::default())),
                )
                .collect::<Vec<_>>();
        let destination_db = DB::open_cf_descriptors(&options, &partial, descriptors)?;
        let mut batch = WriteBatch::default();
        let mut records = 0u64;

        for name in KEYSPACES {
            let source_keyspace = source_db.keyspace(name, KeyspaceCreateOptions::default)?;
            let Some(column_family) = destination_db.cf_handle(name) else {
                return Err(format!("destination is missing column family {name}").into());
            };
            for guard in source_keyspace.iter() {
                let (key, value) = guard.into_inner()?;
                batch.put_cf(column_family, key.as_ref(), value.as_ref());
                records += 1;
            }
        }
        let mut write_options = WriteOptions::default();
        write_options.set_sync(true);
        destination_db.write_opt(batch, &write_options)?;
        destination_db.flush_wal(true)?;
        destination_db.flush()?;
        drop(destination_db);
        copy_marker_files(source, &partial)?;
        write_manifest(&partial)?;
        validate_destination(&partial, encryption_key)?;
        println!("validated {records} records");
        Ok(())
    })();

    match result {
        Ok(()) => {
            fs::rename(&partial, destination)?;
            println!(
                "OK: migrated source={} destination={}",
                source.display(),
                destination.display()
            );
            Ok(())
        },
        Err(error) => {
            let _ = fs::remove_dir_all(&partial);
            Err(error)
        },
    }
}

fn validate_paths(source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    if !source.is_dir() {
        return Err(format!("source is not a directory: {}", source.display()).into());
    }
    if destination.exists() {
        return Err(format!("destination already exists: {}", destination.display()).into());
    }
    let source = fs::canonicalize(source)?;
    let parent = destination
        .parent()
        .ok_or("destination has no parent directory")?;
    fs::create_dir_all(parent)?;
    let parent = fs::canonicalize(parent)?;
    if parent.starts_with(&source) {
        return Err("destination must not be inside the source directory".into());
    }
    Ok(())
}

fn partial_destination(destination: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let name = destination
        .file_name()
        .ok_or("destination must have a directory name")?
        .to_string_lossy();
    Ok(destination.with_file_name(format!(".{name}.partial-{}", std::process::id())))
}

fn copy_marker_files(source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    for name in [".thingd-encryption", ".thingd-encryption-check"] {
        let source_file = source.join(name);
        if source_file.is_file() {
            fs::copy(source_file, destination.join(name))?;
        }
    }
    Ok(())
}

fn write_manifest(destination: &Path) -> Result<(), Box<dyn Error>> {
    let manifest = serde_json::json!({
        "format_version": 1,
        "contract": "rocksdb-tantivy-v1",
        "keyspaces": KEYSPACES,
        "search_schema_version": 1
    });
    fs::write(
        destination.join(".thingd-storage.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}

fn validate_destination(path: &Path, encryption_key: Option<&str>) -> Result<(), Box<dyn Error>> {
    let encryption = encryption_key.map(parse_key).transpose()?;
    let options = thingd::PersistentOpenOptions {
        encryption,
        search_mode: thingd::PersistentSearchMode::Disabled,
        ..thingd::PersistentOpenOptions::default()
    };
    let engine = thingd::PersistentEngine::open_with_options(path, options)?;
    engine.checkpoint()?;
    Ok(())
}

fn parse_key(value: &str) -> Result<thingd::EncryptionConfig, Box<dyn Error>> {
    if value.len() != 64 {
        return Err("encryption key must contain 64 hexadecimal characters".into());
    }
    let bytes = (0..64)
        .step_by(2)
        .map(|index| Ok(u8::from_str_radix(&value[index..index + 2], 16)?))
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    Ok(thingd::EncryptionConfig::from_key(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fjall::PersistMode;
    use thingd::{MemoryObject, ObjectStore};

    #[test]
    fn migrates_legacy_plaintext_records_and_reopens() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("legacy");
        let destination = temp.path().join("new");
        let source_db = Database::builder(&source).open().unwrap();
        let objects = source_db
            .keyspace("objects", KeyspaceCreateOptions::default)
            .unwrap();
        let object = MemoryObject::new("notes", "one", r#"{"text":"hello"}"#);
        let key = b"notes\0one";
        objects
            .insert(key, serde_json::to_vec(&object).unwrap())
            .unwrap();
        for name in KEYSPACES.iter().copied().filter(|name| *name != "objects") {
            source_db
                .keyspace(name, KeyspaceCreateOptions::default)
                .unwrap();
        }
        source_db.persist(PersistMode::SyncAll).unwrap();
        drop(objects);
        drop(source_db);

        migrate(&source, &destination, None).unwrap();
        let engine = thingd::PersistentEngine::open_with_options(
            &destination,
            thingd::PersistentOpenOptions {
                search_mode: thingd::PersistentSearchMode::Disabled,
                ..thingd::PersistentOpenOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            engine.get_object("notes", "one").unwrap(),
            Some(object.clone())
        );
        drop(engine);

        let options = Options::default();
        let destination_db = DB::open_cf(&options, &destination, KEYSPACES).unwrap();
        for name in KEYSPACES {
            assert!(
                destination_db.cf_handle(name).is_some(),
                "missing {name} CF"
            );
        }
        let objects_cf = destination_db.cf_handle("objects").unwrap();
        assert_eq!(
            destination_db.get_cf(objects_cf, b"notes\0one").unwrap(),
            Some(serde_json::to_vec(&object).unwrap())
        );
    }
}
