#!/usr/bin/env bash
set -euo pipefail

server_binary="${1:?usage: test-thingdb-backend.sh <thingd-server-binary>}"
root="$(mktemp -d "${TMPDIR:-/tmp}/thingd-thingdb-smoke.XXXXXX")"
trap 'rm -rf "$root"' EXIT

rocks_source="$root/rocksdb-source"
thingdb_store="$root/thingdb-store"
rocks_destination="$root/rocksdb-destination"

# Open and compact a fresh default RocksDB store.
THINGD_STORAGE_BACKEND=rocksdb "$server_binary" --compact "$rocks_source"
THINGD_STORAGE_BACKEND=rocksdb "$server_binary" --check "$rocks_source"

# Repack RocksDB into ThingDB and validate the new format.
THINGD_STORAGE_BACKEND=thingdb "$server_binary" --repack "$rocks_source" \
  --destination "$thingdb_store"
THINGD_STORAGE_BACKEND=thingdb "$server_binary" --check "$thingdb_store"
THINGD_STORAGE_BACKEND=thingdb "$server_binary" --compact "$thingdb_store"

# Repack back without modifying either source directory.
THINGD_STORAGE_BACKEND=rocksdb "$server_binary" --repack "$thingdb_store" \
  --source-backend thingdb --destination "$rocks_destination"
THINGD_STORAGE_BACKEND=rocksdb "$server_binary" --check "$rocks_destination"

test -f "$rocks_source/.thingd-storage.json"
test -f "$thingdb_store/MANIFEST.json"
test -f "$rocks_destination/.thingd-storage.json"
echo "ThingDB backend smoke test passed."
