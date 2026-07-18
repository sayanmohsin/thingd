# thingd API Specification

This is the language-agnostic API contract for thingd. Any SDK (Node.js, Go, Rust, Flutter) implements this spec in its own language.

thingd is a fast, object-first data engine for applications and AI agents. It supports in-memory, file-based (SQLite), Docker, and hosted HTTP instances.

## Sections

- [Data Model](data-model.md) — entity definitions: objects, events, queues, links
- [REST API](rest-api.md) — HTTP endpoints for app SDKs
- [MCP Tools](mcp-tools.md) — tool schemas for AI agents
- [Search](search.md) — FTS5 query syntax, filters, scoring
- [Errors](errors.md) — error codes, HTTP mapping, MCP error format

## Architecture

```
┌─────────────────────────────────────────────────────┐
│              thingd (Rust engine)                    │
│  ObjectStore, EventLog, QueueStore, Searcher,       │
│  LinkStore — in-memory + SQLite implementations     │
└──────────────────────┬──────────────────────────────┘
                       │
          ┌────────────┼────────────┬────────────┬────────────┐
          │            │            │            │            │
    ┌─────▼────┐ ┌─────▼────┐ ┌────▼─────┐ ┌───▼──────┐ ┌───▼──────┐
    │ thingd   │ │ thingd-  │ │ thingd-  │ │ thingd-  │ │ thingd-  │
    │ (Node)   │ │ client   │ │ go       │ │ rust     │ │ flutter  │
    │ napi-rs  │ │ fetch    │ │ cgo FFI  │ │ direct   │ │ dart FFI │
    └─────┬────┘ └─────┬────┘ └────┬─────┘ └───┬──────┘ └───┬──────┘
          │            │            │            │            │
          ▼            ▼            ▼            ▼            ▼
    ┌─────────────────────────────────────────────────────────────┐
    │           Protocol Adapters (per-language)                   │
    │  REST API (/v1/*)  │  MCP Server (32 tools)                 │
    └─────────────────────────┬───────────────────────────────────┘
                              │
                  ┌───────────┼───────────┐
                  │           │           │
           ┌──────▼──┐  ┌─────▼─────┐ ┌──▼──────┐
           │ CLI     │  │ Cloud     │ │ Web UI  │
           │ + TUI   │  │ Hosted    │ │ Local + │
           │ + MCP   │  │ Auth+Billing│ │ Cloud  │
           └─────────┘  └───────────┘ └─────────┘
```

## Response Format

All REST API responses follow a consistent format:

**Success:**
```json
{ "data": <payload> }
```

**List success:**
```json
{ "data": [<items>], "total": <number> }
```

**Error:**
```json
{ "error": { "code": "<error_code>", "message": "<description>" } }
```

## Authentication

REST API endpoints require a Bearer token when `THINGD_AUTH_TOKEN` is set:

```
Authorization: Bearer <token>
```

MCP server endpoints use the same Bearer token scheme.

## Quick Reference

| Protocol | Endpoint | Purpose |
|----------|----------|---------|
| REST | `GET /v1/health` | Health check + counts |
| REST | `GET /v1/objects/:collection/:id` | Read object |
| REST | `PUT /v1/objects/:collection/:id` | Create/replace object |
| REST | `DELETE /v1/objects/:collection/:id` | Delete object |
| REST | `GET /v1/objects?collection=` | List objects |
| REST | `PUT /v1/objects/batch?collection=` | Batch create |
| REST | `GET /v1/objects/batch?collection=` | Batch read |
| REST | `POST /v1/search` | Full-text search |
| REST | `POST /v1/events/:stream` | Append event |
| REST | `GET /v1/events?stream=` | List events |
| REST | `POST /v1/queues/:queue/push` | Push job |
| REST | `POST /v1/queues/:queue/claim` | Claim job |
| REST | `POST /v1/queues/:queue/ack` | Ack job |
| REST | `POST /v1/queues/:queue/nack` | Nack job |
| REST | `GET /v1/queues/:queue/jobs` | List jobs |
| REST | `GET /v1/queues/:queue/dead` | Dead-letter jobs |
| REST | `POST /v1/links` | Create link |
| REST | `GET /v1/links?id=` | Get link |
| REST | `GET /v1/links?reference=` | Get neighbors |
| REST | `DELETE /v1/links/:id` | Delete link |
| MCP | `thing_search` | Search objects + events |
| MCP | `thing_get` | Get object |
| MCP | `thing_put` | Put object |
| MCP | `thing_delete` | Delete object |
| MCP | `thing_events_append` | Append event |
| MCP | `thing_events_list` | List events |
| MCP | `thing_queue_push` | Push job |
| MCP | `thing_queue_claim` | Claim job |
| MCP | `thing_queue_ack` | Ack job |
| MCP | `thing_queue_nack` | Nack job |
| MCP | `thing_queue_list` | List jobs |
| MCP | `thing_queue_dead` | Dead-letter jobs |
| MCP | `thing_objects_list` | List objects |
| MCP | `thing_objects_put_batch` | Batch put |
| MCP | `thing_objects_delete_batch` | Batch delete |
| MCP | `thing_objects_get_batch` | Batch read |
| MCP | `thing_link_create` | Create link |
| MCP | `thing_link_delete` | Delete link |
| MCP | `thing_link_get` | Get link |
| MCP | `thing_link_neighbors` | Get neighbors |
| MCP | `thing_link_count` | Count links |
| MCP | `thing_count_objects` | Count objects |
| MCP | `thing_count_events` | Count events |
| MCP | `thing_count_active_jobs` | Count active jobs |
| MCP | `thing_count_dead_jobs` | Count dead jobs |
| MCP | `thing_list_collections` | List collections |
| MCP | `thing_list_streams` | List streams |
| MCP | `thing_list_queues` | List queues |
