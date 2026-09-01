# thingd-server

A single static binary that serves thingd over HTTP — **MCP**, **REST**, and **cluster** in one process.

Use it instead of the Node.js SDK when you want a standalone server with no runtime dependencies.

The server embeds its durable storage directly. It does not connect to a
separate database container or require a database service.

RocksDB is the default durable backend. The experimental Rust-native ThingDB
backend can be selected with `THINGD_STORAGE_BACKEND=thingdb`; it uses a
separate format and is not a direct RocksDB file-compatible replacement. See
the public [storage backend guide](../../docs/storage-backends.md) and
[benchmark methodology](../../docs/benchmarks.md).

## Quick start

```bash
# from source
cargo run -p thingd-server

# or pull the Docker image
docker pull ghcr.io/sayanmohsin/thingd-server
docker run -p 7377:7377 -v ./data:/data ghcr.io/sayanmohsin/thingd-server
```

Starts on `http://0.0.0.0:7377` by default. Point your MCP client at `http://localhost:7377/mcp`.

## What it serves

| Protocol | Endpoint | Purpose |
|---|---|---|
| **MCP** | `POST /mcp` | Model Context Protocol — 39 core tools (search, put, get, delete, events, queues, links, aggregate, timeseries, schema, NLQ, indexes) |
| **REST** | `GET /healthz` | Health check (unauthenticated) |
| **REST** | `GET /ready` | Readiness and search rebuild status (unauthenticated) |
| **REST** | `GET /v1/health` | Same as `/healthz` |
| **REST** | `GET /v1/counts/objects` | Object count |
| **REST** | `GET /v1/counts/events` | Event count |
| **REST** | `GET /v1/counts/links` | Link count |
| **REST** | `GET /v1/collections` | List collection names |
| **REST** | `GET /v1/streams` | List stream names |
| **REST** | `GET /v1/queues` | List queue names |
| **REST** | `GET /v1/objects` | List objects |
| **REST** | `PUT /v1/objects/batch` | Batch put/delete objects |
| **REST** | `GET/PUT/DELETE /v1/objects/{collection}/{id}` | Single object CRUD |
| **REST** | `POST /v1/search` | Full-text search |
| **REST** | `POST/GET /v1/events/{stream}` | Event stream append/list |
| **REST** | `POST /v1/events` | List events across streams |
| **REST** | Queue endpoints | `push`, `claim`, `ack`, `nack`, `list`, `dead` under `/v1/queues/{queue}/` |
| **REST** | Link endpoints | CRUD under `/v1/links` |
| **REST** | `POST /v1/aggregate` | Aggregate queries (count, sum, avg, min, max) |
| **REST** | `POST /v1/aggregate/timeseries` | Time-bucketed aggregation |
| **REST** | `POST /v1/nlq` | Natural language queries |
| **REST** | `GET /v1/collections/schema` | Reflect collection schemas |
| **REST** | `GET /v1/connectors` | List available connectors |
| **REST** | `POST /v1/connectors/{type}/ping` | Test connector connection |
| **REST** | `POST /v1/connectors/{type}/tables` | List source tables or worksheets |
| **REST** | `POST /v1/connectors/{type}/schema` | Discover external schema |
| **REST** | `POST /v1/connectors/{type}/preview` | Preview source rows |
| **REST** | `POST /v1/connectors/{type}/pull` | Import data from external source |
| **REST** | `GET /metrics` | Prometheus metrics |
| **Cluster** | `GET /cluster/status` | Cluster node status |
| **Cluster** | `GET /cluster/peers` | Cluster peer list |

## Configuration

Configure via environment variables or a YAML config file pointed at by `THINGD_CONFIG`.

| Variable | Default | Description |
|---|---|---|
| `THINGD_HOST` | `0.0.0.0` | Bind address |
| `THINGD_PORT` | `7377` | HTTP port |
| `THINGD_DATABASE` | `thingd.db` | persistent database directory |
| `THINGD_STORAGE_BACKEND` | `rocksdb` | durable backend: `rocksdb` or experimental `thingdb` |
| `THINGD_ENCRYPTION_KEY` | — | optional 64-character hexadecimal key for encrypted persistent storage |
| `THINGD_SEARCH_MODE` | `persistent` | `persistent`, `persistent-async`, `persistent-no-rebuild`, or `disabled` |
| `THINGD_SEARCH_COMMIT_INTERVAL_MS` | `250` | Maximum debounce before coalesced Tantivy commits |
| `THINGD_SEARCH_COMMIT_BATCH_SIZE` | `32` | Maximum mutations per Tantivy commit |
| `THINGD_SEARCH_QUEUE_MAX_KEYS` | `10000` | Bounded distinct search keys before fallback/rebuild |
| `THINGD_JOURNAL_MAX_BYTES` | `33554432` | Soft journal threshold before recovery backpressure |
| `THINGD_RECOVERY_BATCH_SIZE` | `32` | Maximum records per recovery batch |
| `THINGD_RECOVERY_PAUSE_MS` | `50` | Yield interval between recovery batches |
| `THINGD_RECOVERY_MAX_RETRIES` | `3` | Maximum automatic search-rebuild retries |
| `THINGD_RECOVERY_MEMORY_LIMIT_BYTES` | — | Optional resident-memory ceiling |
| `THINGD_AUTH_TOKEN` | — | Bearer token for authenticated requests |
| `THINGD_ALLOW_UNAUTHENTICATED` | `false` | Skip auth entirely |
| `THINGD_MCP_MAX_OBJECT_SIZE` | `1 MB` | Max object size for MCP puts |
| `THINGD_RATE_LIMIT_ENABLED` | `false` | Enable rate limiting |
| `THINGD_RATE_LIMIT_REQUESTS` | `60` | Requests per minute when rate limited |
| `THINGD_REQUEST_TIMEOUT` | `30` | Request timeout in seconds |
| `THINGD_MAX_PAYLOAD_BYTES` | `5 MB` | Max request body size |
| `THINGD_CORS_ORIGINS` | `*` | Comma-separated allowed origins |
| `THINGD_CLUSTER_MODE` | `standalone` | `standalone` or `cluster` |
| `THINGD_CLUSTER_PEERS` | — | Comma-separated peer URLs |

Set `THINGD_ENCRYPTION_KEY` before startup to open encrypted persistent
storage. Missing or incorrect keys fail startup; the server never falls back
to memory for an encrypted database. Filesystem backups remain encrypted,
while JSON exports are decrypted logical data and must be protected separately.

Validate a database before starting the server. This check is lock-free and
does not open or mutate the database:

```bash
thingd-server --check /data/thingd.db
```

For a production cutover where a fresh empty directory must never be accepted,
require a migrated RocksDB manifest:

```bash
thingd-server --check /data/thingd.db --require-migrated
```

`GET /ready` returns `ready` only after storage recovery completes. During an
asynchronous search rebuild or primary journal compaction it returns `503` with
`Retry-After: 1`; search uses the bounded fallback scan and writes are paused.
Mutation clients should retry the response with bounded backoff rather than
restarting the server, especially on hosts with around 1 GB RAM.

Startup is staged: primary storage is recovered and compacted before Tantivy
rebuild begins. `/ready` remains unavailable until both phases finish. Reads
can use fallback scanning while writes receive `503` and must be retried after
the advertised delay. If bounded recovery fails, stop writers and run an
offline maintenance command:

```bash
thingd-server --compact /data/thingd.db
thingd-server --repack /data/thingd.db --destination /data/thingd-repacked.db
```

`--compact` requires exclusive access and preserves the native store. `--repack`
is an explicit logical migration into a fresh destination; it is not automatic
native-format conversion and never overwrites the source or destination. The
destination backend follows `THINGD_STORAGE_BACKEND` (RocksDB by default); use
`--source-backend thingdb` when the source is a ThingDB directory.

Current releases do not open legacy native directories. Recover those stores
with the archived compatibility release that created them or from a logical
export before switching to the current runtime.

For hosts with less than 2 GB RAM, prefer a separate standalone server over
HTTP rather than embedding the native store in the application process.

## Architecture

```
thingd-server binary
  ├── main.rs          — entry point, signal handling
  ├── config.rs        — YAML + env config loader
  ├── server.rs        — axum router, middleware stack (CORS, timeout, auth, rate limit)
  ├── rest.rs          — REST API handlers
  ├── mcp.rs           — MCP handler (39 core tools via registry dispatch)
  ├── engine.rs        — EnginePool: shared database connections
  ├── auth.rs          — Bearer token auth middleware
  ├── rate_limit.rs    — Token bucket rate limiter
  ├── cluster.rs       — Cluster status and peer discovery
  └── error.rs         — Error types and production-mode sanitization
```

The binary embeds the `thingd` crate directly (no FFI, no subprocess). All MCP and REST handlers use the same engine — there is no network hop between the HTTP layer and the database.

When `THINGD_ENCRYPTION_KEY` is configured, the sidecar opens the encrypted
database before starting its HTTP, REST, or MCP handlers. The key is never
part of MCP requests or REST headers. Authentication still protects the
network endpoint independently. Encrypted search rebuilds in memory and does
not create a persistent `search` directory; expect higher startup cost and
memory use.

## Differences from the Node.js SDK

| | thingd-server | @thingd/sdk |
|---|---|---|
| Runtime | Standalone binary | Node.js (embedded or subprocess) |
| Size | Platform-dependent static binary | Node.js with optional native addon |
| MCP | 39 core tools native | 49 tools via TypeScript, including scheduler tools |
| REST | Yes (axum) | Yes (Express) |
| Cluster | Status + peer discovery | Full leader election + forwarding |
| Write approval | Not implemented | Available |
| Cloud driver | Not implemented | Available (thingd-cloud) |
| Startup | Instant (compiled) | ~300ms (JIT warmup) |
