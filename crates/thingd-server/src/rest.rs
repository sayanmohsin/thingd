use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU64, Ordering},
};
use thingd::*;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use crate::error::AppError;
use crate::server::AppState;

const REPLICATION_STREAM: &str = "__thingd:system:replication";
const MAX_BATCH_ITEMS: usize = 1_000;
const MAX_BLOCKING_WORKERS: usize = 8;
const SNAPSHOT_PAGE_SIZE: u64 = 250;
const SNAPSHOT_MAX_LINE_BYTES: usize = 16 * 1024 * 1024;
static BLOCKING_GATE: OnceLock<Arc<Semaphore>> = OnceLock::new();
static BLOCKING_ACTIVE: AtomicU64 = AtomicU64::new(0);
static BLOCKING_REJECTED: AtomicU64 = AtomicU64::new(0);

fn blocking_permit() -> Result<OwnedSemaphorePermit, AppError> {
    BLOCKING_GATE
        .get_or_init(|| Arc::new(Semaphore::new(MAX_BLOCKING_WORKERS)))
        .clone()
        .try_acquire_owned()
        .inspect(|_| {
            BLOCKING_ACTIVE.fetch_add(1, Ordering::Relaxed);
        })
        .map_err(|_| {
            BLOCKING_REJECTED.fetch_add(1, Ordering::Relaxed);
            AppError::overloaded(format!(
                "blocking worker limit reached ({MAX_BLOCKING_WORKERS}); retry shortly"
            ))
        })
}

fn finish_blocking() {
    BLOCKING_ACTIVE.fetch_sub(1, Ordering::Relaxed);
}

fn replication_collection_allowed(state: &AppState, collection: &str) -> bool {
    !collection.starts_with("__thingd")
        && (state.sync_config.collections.is_empty()
            || state
                .sync_config
                .collections
                .iter()
                .any(|allowed| allowed == collection))
}

fn ensure_source_writable(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    if state.sync_config.role == crate::config::SyncRole::Replica {
        return Err(AppError::bad_request(
            "This Thingd instance is configured as a replication target",
        ));
    }
    ensure_storage_writable(state, headers)
}

fn ensure_storage_writable(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    let tenant_id = crate::auth::extract_tenant_id(headers, &state.tenant_config)?;
    let db_path = state.tenant_config.resolve_db_path(tenant_id.as_deref());
    let status = state.pool.storage_maintenance_status(&db_path);
    if status.state != "idle" {
        return Err(AppError::overloaded(format!(
            "storage maintenance is {}; writes are temporarily paused",
            status.state
        )));
    }
    Ok(())
}

fn ensure_replication_target_allowed(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), AppError> {
    if state.sync_config.provider == "thingd.cloud" && !state.sync_config.allow_cloud_target {
        return Err(AppError::conflict(
            "Cloud targets are protected by default; explicitly enable this target before applying replication changes",
        ));
    }
    for (header_name, expected, label) in [
        (
            "x-thingd-project-id",
            state.sync_config.project_id.as_str(),
            "project",
        ),
        (
            "x-thingd-instance-slug",
            state.sync_config.instance_slug.as_str(),
            "instance",
        ),
    ] {
        if !expected.is_empty()
            && headers
                .get(header_name)
                .and_then(|value| value.to_str().ok())
                != Some(expected)
        {
            return Err(AppError::conflict(format!(
                "Replication target {label} identity does not match configured instance"
            )));
        }
    }
    Ok(())
}

fn append_replication_change(
    engine: &mut dyn ThingStore,
    source_id: &str,
    operation: &str,
    collection: Option<&str>,
    id: Option<&str>,
    payload: Option<Value>,
) -> Result<(), AppError> {
    let change = json!({
        "sourceId": source_id,
        "operation": operation,
        "collection": collection,
        "id": id,
        "payload": payload,
    });
    ReplicationService::new(engine, ReplicationConfig::source(source_id.to_string()))
        .record_change(&change)
        .map_err(|e| AppError::internal(format!("Failed to append replication change: {e}")))
}

fn get_engine<'a>(
    state: &'a AppState,
    headers: &'a HeaderMap,
) -> Result<crate::engine::SharedEngine, AppError> {
    let tenant_id = crate::auth::extract_tenant_id(headers, &state.tenant_config)?;
    let db_path = state.tenant_config.resolve_db_path(tenant_id.as_deref());
    Ok(state.pool.get_reader(&db_path))
}

fn ok<T: serde::Serialize>(data: T) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({ "data": data })))
}

// ─── Health ─────────────────────────────────────────────────────

pub async fn health() -> Json<Value> {
    Json(json!({ "data": { "status": "ok" } }))
}

pub async fn ready(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let Ok(engine) = get_engine(&state, &headers) else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "data": { "status": "not_ready" }
            })),
        )
            .into_response();
    };
    let guard = engine.lock();
    let search = guard.search_rebuild_status();
    let maintenance = guard.storage_maintenance_status();
    if maintenance.state != "idle" {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "data": {
                    "status": "not_ready",
                    "maintenance": maintenance,
                    "degradedSearch": true,
                    "search": search,
                }
            })),
        )
            .into_response();
    }
    Json(json!({
        "data": {
            "status": "ready",
            "degradedSearch": search.is_some(),
            "search": search,
            "maintenance": maintenance,
        }
    }))
    .into_response()
}

pub async fn diagnostics(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let permit = blocking_permit()?;
    let diagnostics = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let g = e.lock();
        let result = g
            .storage_diagnostics()
            .map(|diagnostics| {
                (
                    diagnostics,
                    g.search_rebuild_status(),
                    g.storage_maintenance_status(),
                )
            })
            .map_err(|error| AppError::internal(error.to_string()));
        finish_blocking();
        result
    })
    .await
    .map_err(|error| AppError::internal(format!("diagnostics worker failed: {error}")))??;
    ok(json!({
        "storage": diagnostics.0,
        "search": diagnostics.1,
        "maintenance": diagnostics.2,
        "blockingWorkers": {
            "active": BLOCKING_ACTIVE.load(Ordering::Relaxed),
            "rejected": BLOCKING_REJECTED.load(Ordering::Relaxed),
            "capacity": MAX_BLOCKING_WORKERS,
        }
    }))
}

pub async fn retention(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let options: RetentionOptions = serde_json::from_value(body.clone())
        .map_err(|error| AppError::bad_request(format!("invalid retention options: {error}")))?;
    if !options.dry_run && body.get("confirm").and_then(Value::as_bool) != Some(true) {
        return Err(AppError::bad_request(
            "retention deletion requires confirm=true; use dryRun=true to preview",
        ));
    }
    let e = get_engine(&state, &headers)?;
    let permit = blocking_permit()?;
    let report = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut g = e.lock();
        let result = g
            .retain(options)
            .map_err(|error| AppError::internal(error.to_string()));
        finish_blocking();
        result
    })
    .await
    .map_err(|error| AppError::internal(format!("retention worker failed: {error}")))??;
    ok(report)
}

pub async fn clear_default_db(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    state
        .pool
        .clear_default_engine()
        .map_err(|e| AppError::internal(format!("Failed to clear default database: {e}")))?;
    Ok(Json(
        json!({ "ok": true, "message": "Default database cleared" }),
    ))
}

pub async fn metrics(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> axum::response::Response {
    let e = match get_engine(&state, &headers) {
        Ok(engine) => engine,
        Err(e) => return e.into_response(),
    };
    let g = e.lock();
    let objects = g.count_objects().unwrap_or(0);
    let events = g.count_events().unwrap_or(0);
    let links = g.count_links().unwrap_or(0);
    let queue_count = g.list_queues().unwrap_or_default().len() as u64;
    let active_jobs = g.count_active_jobs().unwrap_or(0);
    let dead_jobs = g.count_dead_jobs().unwrap_or(0);

    let body = format!(
        "# HELP thingd_objects_total Total number of objects.\n\
         # TYPE thingd_objects_total gauge\n\
         thingd_objects_total {objects}\n\
         # HELP thingd_events_total Total number of events.\n\
         # TYPE thingd_events_total gauge\n\
         thingd_events_total {events}\n\
         # HELP thingd_links_total Total number of links.\n\
         # TYPE thingd_links_total gauge\n\
         thingd_links_total {links}\n\
         # HELP thingd_queues_total Total number of queues.\n\
         # TYPE thingd_queues_total gauge\n\
         thingd_queues_total {queue_count}\n\
         # HELP thingd_active_jobs_total Total active queue jobs.\n\
         # TYPE thingd_active_jobs_total gauge\n\
         thingd_active_jobs_total {active_jobs}\n\
         # HELP thingd_dead_jobs_total Total dead-letter queue jobs.\n\
         # TYPE thingd_dead_jobs_total gauge\n\
         thingd_dead_jobs_total {dead_jobs}\n\
         # HELP thingd_blocking_workers_active Active bounded blocking workers.\n\
         # TYPE thingd_blocking_workers_active gauge\n\
         thingd_blocking_workers_active {blocking_active}\n\
         # HELP thingd_blocking_workers_rejected_total Requests rejected because blocking workers were saturated.\n\
         # TYPE thingd_blocking_workers_rejected_total counter\n\
         thingd_blocking_workers_rejected_total {blocking_rejected}\n",
        objects = objects,
        events = events,
        links = links,
        queue_count = queue_count,
        active_jobs = active_jobs,
        dead_jobs = dead_jobs,
        blocking_active = BLOCKING_ACTIVE.load(Ordering::Relaxed),
        blocking_rejected = BLOCKING_REJECTED.load(Ordering::Relaxed),
    );

    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response()
}

// ─── Counts ─────────────────────────────────────────────────────

pub async fn count_objects(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    ok(json!({ "count": g.count_objects().map_err(|e| AppError::internal(e.to_string()))? }))
}

pub async fn count_objects_in_collection(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(collection): Path<String>,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    ok(
        json!({ "count": g.count_objects_in_collection(&collection).map_err(|e| AppError::internal(e.to_string()))? }),
    )
}

pub async fn count_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    ok(json!({ "count": g.count_events().map_err(|e| AppError::internal(e.to_string()))? }))
}

pub async fn count_links(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    ok(json!({ "count": g.count_links().map_err(|e| AppError::internal(e.to_string()))? }))
}

// ─── Listings ───────────────────────────────────────────────────

pub async fn list_collections(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    ok(g.list_collections()
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn list_streams(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    ok(g.list_streams()
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn list_queues(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    ok(g.list_queues()
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn list_indexes(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    ok(g.list_indexes()
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn create_index(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let collection = body["collection"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("collection is required"))?;
    let field = body["field"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("field is required"))?;
    let unique = body["unique"].as_bool().unwrap_or(false);
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    g.create_index_definition(thingd::IndexDefinition {
        collection: collection.to_string(),
        field: field.to_string(),
        unique,
    })
    .map_err(|e| AppError::internal(e.to_string()))?;
    ok(json!({ "created": true }))
}

pub async fn delete_index(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let collection = body["collection"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("collection is required"))?;
    let field = body["field"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("field is required"))?;
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    let deleted = g
        .delete_index(collection, field)
        .map_err(|e| AppError::internal(e.to_string()))?;
    ok(json!({ "deleted": deleted }))
}

// ─── Objects ────────────────────────────────────────────────────

pub async fn list_objects(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, AppError> {
    let collection = params["collection"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing collection"))?
        .to_string();
    let mut filter = Vec::new();
    if let Some(obj) = params.as_object() {
        for (k, v) in obj {
            if let Some(fk) = k.strip_prefix("filter.") {
                filter.push((fk.to_string(), v.clone()));
            }
        }
    }
    let sort_by = params
        .get("sortBy")
        .and_then(|v| v.as_str())
        .map(|f| SortBy {
            field: f.to_string(),
            direction: if params.get("sortDir").and_then(|v| v.as_str()) == Some("desc") {
                SortDirection::Desc
            } else {
                SortDirection::Asc
            },
        });
    let opts = ListObjectsOptions {
        filter,
        sort_by,
        limit: params.get("limit").and_then(|v| v.as_u64()),
        offset: params.get("offset").and_then(|v| v.as_u64()),
    };

    let e = get_engine(&state, &headers)?;
    let permit = blocking_permit()?;
    let objects = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let g = e.lock();
        let collections = vec![collection];
        let result = g
            .list_objects(Some(&collections), &opts)
            .map_err(|error| AppError::internal(error.to_string()));
        finish_blocking();
        result
    })
    .await
    .map_err(|error| AppError::internal(format!("list operation failed: {error}")))??;
    let items: Vec<Value> = objects.iter().map(|obj| {
        let body: Value = serde_json::from_str(&obj.body).unwrap_or(Value::Null);
        json!({ "id": obj.key.id, "collection": obj.key.collection, "body": body, "version": obj.version, "createdAt": obj.created_at, "updatedAt": obj.updated_at })
    }).collect();
    ok(items)
}

pub async fn put_object(
    State(state): State<Arc<AppState>>,
    Path((collection, id)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let mut obj = MemoryObject::new(collection.clone(), id.clone(), body.to_string());
    if let Some(vector) = body.get("vector").and_then(|v| v.as_array()) {
        let vec: Vec<f32> = vector
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();
        if !vec.is_empty() {
            obj = obj.with_vector(vec);
        }
    }
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    let expected_version = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let r = if let Some(version) = expected_version {
        let opts = thingd::PutObjectOptions {
            expected_version: Some(version),
            ..Default::default()
        };
        g.put_object_with_options(obj, opts)
    } else {
        g.put_object(obj)
    }
    .map_err(AppError::from)?;
    if replication_collection_allowed(&state, &collection) {
        append_replication_change(
            &mut **g,
            &state.sync_config.source_id,
            "object.upsert",
            Some(&collection),
            Some(&id),
            Some(json!({
                "id": r.key.id,
                "collection": r.key.collection,
                "body": serde_json::from_str::<Value>(&r.body).unwrap_or(Value::Null),
                "version": r.version,
                "createdAt": r.created_at,
                "updatedAt": r.updated_at,
            })),
        )?;
    }
    ok(
        json!({ "id": r.key.id, "collection": r.key.collection, "version": r.version, "createdAt": r.created_at, "updatedAt": r.updated_at }),
    )
}

pub async fn get_object(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((collection, id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    match g
        .get_object(&collection, &id)
        .map_err(|e| AppError::internal(e.to_string()))?
    {
        Some(obj) => {
            let body: Value = serde_json::from_str(&obj.body).unwrap_or(Value::Null);
            ok(
                json!({ "id": obj.key.id, "collection": obj.key.collection, "body": body, "version": obj.version, "createdAt": obj.created_at, "updatedAt": obj.updated_at }),
            )
        },
        None => Err(AppError::not_found("Object not found")),
    }
}

pub async fn delete_object(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((collection, id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    let deleted = g
        .delete_object(&collection, &id)
        .map_err(|e| AppError::internal(e.to_string()))?;
    if deleted && replication_collection_allowed(&state, &collection) {
        append_replication_change(
            &mut **g,
            &state.sync_config.source_id,
            "object.delete",
            Some(&collection),
            Some(&id),
            None,
        )?;
    }
    ok(json!({ "deleted": deleted }))
}

pub async fn put_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<Value>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let collection = params["collection"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing collection"))?;
    let items = body.as_array().cloned().unwrap_or_default();
    if items.len() > MAX_BATCH_ITEMS {
        return Err(AppError::bad_request(format!(
            "object batch contains {} items; maximum is {MAX_BATCH_ITEMS}",
            items.len()
        )));
    }
    let body_only = params
        .get("bodyOnly")
        .and_then(|value| {
            value
                .as_bool()
                .or_else(|| value.as_str().map(|v| v == "true"))
        })
        .unwrap_or(false);
    let objects: Vec<MemoryObject> = items
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let stored_body = if body_only {
                let mut value = v.clone();
                if let Some(object) = value.as_object_mut() {
                    object.remove("id");
                }
                value.to_string()
            } else {
                v.to_string()
            };
            MemoryObject::new(
                collection.to_string(),
                v["id"].as_str().unwrap_or(&format!("b{}", i)),
                stored_body,
            )
        })
        .collect();
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    let stored = g
        .put_objects_batch(objects)
        .map_err(|e| AppError::internal(e.to_string()))?;
    if replication_collection_allowed(&state, collection) {
        for object in &stored {
            append_replication_change(
                &mut **g,
                &state.sync_config.source_id,
                "object.upsert",
                Some(collection),
                Some(&object.key.id),
                Some(json!({
                    "id": object.key.id,
                    "collection": object.key.collection,
                    "body": serde_json::from_str::<Value>(&object.body).unwrap_or(Value::Null),
                    "version": object.version,
                    "createdAt": object.created_at,
                    "updatedAt": object.updated_at,
                })),
            )?;
        }
    }
    ok(stored)
}

pub async fn get_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<Value>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let collection = params["collection"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing collection"))?;
    let ids: Vec<String> = body
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .or_else(|| {
            body["ids"].as_array().map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
        })
        .unwrap_or_default();
    if ids.len() > MAX_BATCH_ITEMS {
        return Err(AppError::bad_request(format!(
            "object batch contains {} ids; maximum is {MAX_BATCH_ITEMS}",
            ids.len()
        )));
    }
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    let results = g
        .get_objects_batch(collection, &ids)
        .map_err(|e| AppError::internal(e.to_string()))?;
    let items: Vec<Value> = results
        .into_iter()
        .map(|opt| {
            opt.map(|obj| {
                let body_val: Value = serde_json::from_str(&obj.body).unwrap_or(Value::Null);
                json!({
                    "id": obj.key.id,
                    "collection": obj.key.collection,
                    "body": body_val,
                    "version": obj.version,
                    "createdAt": obj.created_at,
                    "updatedAt": obj.updated_at,
                })
            })
            .unwrap_or(Value::Null)
        })
        .collect();
    ok(items)
}

pub async fn delete_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<Value>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let collection = params["collection"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing collection"))?;
    let ids: Vec<String> = body
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .or_else(|| {
            body["ids"].as_array().map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
        })
        .unwrap_or_default();
    if ids.len() > MAX_BATCH_ITEMS {
        return Err(AppError::bad_request(format!(
            "object batch contains {} ids; maximum is {MAX_BATCH_ITEMS}",
            ids.len()
        )));
    }
    let keys: Vec<(String, String)> = ids
        .into_iter()
        .map(|id| (collection.to_string(), id))
        .collect();
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    let deleted = g
        .delete_objects_batch(&keys)
        .map_err(|e| AppError::internal(e.to_string()))?;
    if replication_collection_allowed(&state, collection) {
        for (collection, id) in &keys {
            append_replication_change(
                &mut **g,
                &state.sync_config.source_id,
                "object.delete",
                Some(collection),
                Some(id),
                None,
            )?;
        }
    }
    ok(json!({ "deleted": deleted }))
}

// ─── Events ─────────────────────────────────────────────────────

pub async fn append_event(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stream): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let et = body["type"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing 'type'"))?;

    // Prevent direct writes to the protected audit stream
    if stream == "__thingd:mcp:audit" {
        return Err(AppError::bad_request(
            "Stream '__thingd:mcp:audit' is protected and cannot be written to directly",
        ));
    }

    let mut event = MemoryEvent::new(&stream, et, body.to_string());
    if let Some(idempotency_key) = body.get("idempotencyKey").and_then(Value::as_str) {
        event.idempotency_key = idempotency_key.to_string();
    }
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    let r = g
        .append_event(event)
        .map_err(|e| AppError::internal(e.to_string()))?;
    if stream != REPLICATION_STREAM && !stream.starts_with("__thingd") {
        append_replication_change(
            &mut **g,
            &state.sync_config.source_id,
            "event.append",
            None,
            None,
            Some(json!({
                "stream": r.stream,
                "type": r.event_type,
                "body": serde_json::from_str::<Value>(&r.body).unwrap_or(Value::Null),
                "idempotencyKey": r.idempotency_key,
            })),
        )?;
    }
    ok(
        json!({ "id": r.sequence.to_string(), "stream": r.stream, "type": r.event_type, "sequence": r.sequence, "createdAt": r.created_at }),
    )
}

pub async fn list_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, AppError> {
    let stream = params.get("stream").and_then(|v| v.as_str());
    let opts = ListEventsOptions {
        from_sequence: params.get("fromSequence").and_then(|v| v.as_u64()),
        limit: params.get("limit").and_then(|v| v.as_u64()),
        since: params
            .get("since")
            .and_then(|v| v.as_str())
            .map(String::from),
    };
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    let events = g
        .list_events(stream, opts)
        .map_err(|e| AppError::internal(e.to_string()))?;
    let items: Vec<Value> = events.iter().map(|r| json!({ "id": r.sequence.to_string(), "stream": r.stream, "type": r.event_type, "sequence": r.sequence, "createdAt": r.created_at })).collect();
    ok(items)
}

pub async fn replication_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, AppError> {
    let after = params.get("after").and_then(Value::as_u64).unwrap_or(0);
    let limit = params
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(500)
        .clamp(1, 1000);
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    let config = ReplicationConfig {
        source_id: state.sync_config.source_id.clone(),
        role: ReplicationRole::Source,
        collections: state.sync_config.collections.clone(),
    };
    let page = ReplicationService::new(&mut **g, config)
        .events(after, limit)
        .map_err(|error| match error {
            ThingdError::Conflict(message) => AppError::conflict(message),
            error => AppError::internal(error.to_string()),
        })?;
    ok(page)
}

pub async fn replication_apply(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_storage_writable(&state, &headers)?;
    if state.sync_config.role == crate::config::SyncRole::Source {
        return Err(AppError::bad_request(
            "Replication apply requires this instance to be configured as a replica",
        ));
    }
    ensure_replication_target_allowed(&state, &headers)?;
    let changes = body
        .get("changes")
        .cloned()
        .ok_or_else(|| AppError::bad_request("Missing changes array"))?;
    let changes: Vec<ReplicationChange> = serde_json::from_value(changes)
        .map_err(|error| AppError::bad_request(format!("Invalid changes array: {error}")))?;
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    let result = ReplicationService::new(
        &mut **g,
        ReplicationConfig {
            source_id: state.sync_config.source_id.clone(),
            role: ReplicationRole::Replica,
            collections: state.sync_config.collections.clone(),
        },
    )
    .apply(&changes)
    .map_err(|error| match error {
        ThingdError::Conflict(message) => AppError::conflict(message),
        ThingdError::InvalidInput(message) => AppError::bad_request(message),
        error => AppError::internal(error.to_string()),
    })?;
    ok(json!({
        "applied": result.applied,
        "skipped": result.skipped,
        "conflicts": result.conflicts,
        "lastAppliedCursor": result.last_applied_cursor,
    }))
}

pub async fn replication_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, AppError> {
    let requested_source_id = params
        .get("sourceId")
        .and_then(Value::as_str)
        .unwrap_or(&state.sync_config.source_id)
        .to_string();
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    let config = ReplicationConfig {
        source_id: requested_source_id,
        role: match state.sync_config.role {
            crate::config::SyncRole::Source => ReplicationRole::Source,
            crate::config::SyncRole::Replica => ReplicationRole::Replica,
        },
        collections: state.sync_config.collections.clone(),
    };
    let status = ReplicationService::new(&mut **g, config)
        .status()
        .map_err(|error| AppError::internal(error.to_string()))?;
    ok(json!({
        "sourceId": status.source_id,
        "provider": state.sync_config.provider,
        "projectId": state.sync_config.project_id,
        "instanceSlug": state.sync_config.instance_slug,
        "role": match status.role { ReplicationRole::Source => "source", ReplicationRole::Replica => "replica" },
        "latestCursor": status.latest_cursor,
        "changeCount": status.change_count,
        "lastAppliedCursor": status.last_applied_cursor,
        "quarantinedConflicts": status.quarantined_conflicts,
    }))
}

/// Inspect durable replication conflicts without exposing internal collections
/// through the general object-listing API.
pub async fn replication_conflicts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    let conflicts = ReplicationService::new(
        &mut **g,
        ReplicationConfig::replica(state.sync_config.source_id.clone()),
    )
    .conflicts()
    .map_err(|e| AppError::internal(e.to_string()))?;
    ok(json!({
        "sourceId": state.sync_config.source_id,
        "conflicts": conflicts,
    }))
}

/// Return a provider-neutral bootstrap snapshot for replicas whose cursor has
/// fallen out of the retained event window.
pub async fn replication_snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    let config = ReplicationConfig {
        source_id: state.sync_config.source_id.clone(),
        role: ReplicationRole::Source,
        collections: state.sync_config.collections.clone(),
    };
    let snapshot = ReplicationService::new(&mut **g, config)
        .snapshot()
        .map_err(|e| AppError::internal(e.to_string()))?;
    let objects = snapshot
        .objects
        .into_iter()
        .map(|object| {
            json!({
                "id": object.key.id,
                "collection": object.key.collection,
                "body": serde_json::from_str::<Value>(&object.body).unwrap_or(Value::Null),
                "version": object.version,
                "createdAt": object.created_at,
                "updatedAt": object.updated_at,
            })
        })
        .collect::<Vec<_>>();
    ok(json!({
        "sourceId": snapshot.source_id,
        "cursor": snapshot.cursor,
        "objects": objects,
        "events": snapshot.events,
    }))
}

pub async fn replication_snapshot_apply(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_storage_writable(&state, &headers)?;
    if state.sync_config.role == crate::config::SyncRole::Source {
        return Err(AppError::bad_request(
            "Snapshot apply requires this instance to be configured as a replica",
        ));
    }
    ensure_replication_target_allowed(&state, &headers)?;
    let source_id = body
        .get("sourceId")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("Snapshot is missing sourceId"))?;
    let snapshot = body.get("snapshot").unwrap_or(&body);
    let objects = snapshot
        .get("objects")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::bad_request("Snapshot is missing objects"))?;
    let cursor = snapshot.get("cursor").and_then(Value::as_u64).unwrap_or(0);
    let events = snapshot
        .get("events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let replace = body
        .get("replace")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    let mut snapshot_objects = Vec::with_capacity(objects.len());
    for value in objects {
        let object: MemoryObject = if value.get("key").is_some() {
            serde_json::from_value(value.clone())
                .map_err(|e| AppError::bad_request(format!("Invalid snapshot object: {e}")))?
        } else {
            let collection = value
                .get("collection")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("Snapshot object is missing collection"))?;
            let id = value
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("Snapshot object is missing id"))?;
            let mut object = MemoryObject::new(
                collection,
                id,
                value
                    .get("body")
                    .cloned()
                    .unwrap_or(Value::Null)
                    .to_string(),
            );
            object.version = value.get("version").and_then(Value::as_u64).unwrap_or(0);
            object.created_at = value
                .get("createdAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            object.updated_at = value
                .get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            object
        };
        if !replication_collection_allowed(&state, &object.key.collection) {
            continue;
        }
        snapshot_objects.push(object);
    }
    let mut snapshot_events = Vec::with_capacity(events.len());
    for value in events {
        let event: MemoryEvent = serde_json::from_value(value)
            .map_err(|e| AppError::bad_request(format!("Invalid snapshot event: {e}")))?;
        if event.stream.starts_with("__thingd") {
            continue;
        }
        snapshot_events.push(event);
    }
    let events_applied = snapshot_events.len() as u64;
    let result = ReplicationService::new(
        &mut **g,
        ReplicationConfig {
            source_id: source_id.to_string(),
            role: ReplicationRole::Replica,
            collections: state.sync_config.collections.clone(),
        },
    )
    .apply_snapshot(
        &ReplicationSnapshot {
            source_id: source_id.to_string(),
            cursor,
            objects: snapshot_objects,
            events: snapshot_events,
        },
        replace,
    )
    .map_err(|error| match error {
        ThingdError::Conflict(message) => AppError::conflict(message),
        error => AppError::internal(error.to_string()),
    })?;
    ok(json!({
        "sourceId": source_id,
        "applied": result.applied,
        "eventsApplied": events_applied,
        "lastAppliedCursor": result.last_applied_cursor,
        "verified": true,
    }))
}

// ─── Logical snapshots ────────────────────────────────────────

/// Stream a complete logical snapshot as newline-delimited JSON. Each record
/// is read in bounded pages so exporting a large store does not build one
/// giant JSON value or hold the engine lock while the client is receiving it.
pub async fn snapshot_export(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let e = match get_engine(&state, &headers) {
        Ok(engine) => engine,
        Err(error) => return error.into_response(),
    };
    let (sender, receiver) = mpsc::channel::<Result<Bytes, std::convert::Infallible>>(16);

    tokio::task::spawn_blocking(move || {
        let send_record = |record: Value| -> Result<(), ()> {
            let mut line = serde_json::to_vec(&record).map_err(|_| ())?;
            line.push(b'\n');
            sender.blocking_send(Ok(Bytes::from(line))).map_err(|_| ())
        };

        let result = (|| -> Result<(), String> {
            send_record(json!({
                "type": "thingd.snapshot",
                "version": "2.0.0",
                "format": "jsonl"
            }))
            .map_err(|_| "snapshot client disconnected".to_string())?;

            let collections = e
                .lock()
                .list_collections()
                .map_err(|error| error.to_string())?;
            for collection in collections {
                let mut offset = 0;
                loop {
                    let page = {
                        let guard = e.lock();
                        guard
                            .list_objects(
                                Some(std::slice::from_ref(&collection)),
                                &ListObjectsOptions {
                                    limit: Some(SNAPSHOT_PAGE_SIZE),
                                    offset: Some(offset),
                                    ..Default::default()
                                },
                            )
                            .map_err(|error| error.to_string())?
                    };
                    let page_len = page.len() as u64;
                    for object in page {
                        send_record(json!({ "type": "object", "object": object }))
                            .map_err(|_| "snapshot client disconnected".to_string())?;
                    }
                    if page_len < SNAPSHOT_PAGE_SIZE {
                        break;
                    }
                    offset += page_len;
                }
            }

            let mut from_sequence = 0;
            loop {
                let page = {
                    let guard = e.lock();
                    guard
                        .list_events(
                            None,
                            ListEventsOptions {
                                from_sequence: Some(from_sequence),
                                limit: Some(SNAPSHOT_PAGE_SIZE),
                                ..Default::default()
                            },
                        )
                        .map_err(|error| error.to_string())?
                };
                let page_len = page.len() as u64;
                for event in page {
                    from_sequence = event.sequence;
                    send_record(json!({ "type": "event", "event": event }))
                        .map_err(|_| "snapshot client disconnected".to_string())?;
                }
                if page_len < SNAPSHOT_PAGE_SIZE {
                    break;
                }
            }

            let queues = e.lock().list_queues().map_err(|error| error.to_string())?;
            for queue in queues {
                let jobs = {
                    let guard = e.lock();
                    let mut jobs = guard.list_jobs(&queue).map_err(|error| error.to_string())?;
                    jobs.extend(
                        guard
                            .list_dead_jobs(&queue)
                            .map_err(|error| error.to_string())?,
                    );
                    jobs
                };
                for job in jobs {
                    send_record(json!({ "type": "queue", "job": job }))
                        .map_err(|_| "snapshot client disconnected".to_string())?;
                }
            }
            Ok(())
        })();

        if let Err(error) = result {
            let _ = send_record(json!({ "type": "error", "error": error }));
        }
    });

    let stream = ReceiverStream::new(receiver);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

/// Import a logical JSONL snapshot incrementally. Objects and events are
/// idempotent by their stable keys; queue records are written with their full
/// stored state so completed and dead jobs remain terminal after restore.
pub async fn snapshot_import(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let e = get_engine(&state, &headers)?;
    let mut lines = body.into_data_stream();
    let mut buffer = Vec::new();
    let mut header_seen = false;
    let mut records = 0u64;
    let mut objects = 0u64;
    let mut events = 0u64;
    let mut queues = 0u64;

    while let Some(chunk) = lines.next().await {
        let chunk = chunk
            .map_err(|error| AppError::bad_request(format!("Invalid snapshot body: {error}")))?;
        buffer.extend_from_slice(&chunk);
        if buffer.len() > SNAPSHOT_MAX_LINE_BYTES && !buffer.contains(&b'\n') {
            return Err(AppError::bad_request(format!(
                "Snapshot record exceeds {SNAPSHOT_MAX_LINE_BYTES} bytes"
            )));
        }

        while let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
            let line = buffer.drain(..=newline).collect::<Vec<_>>();
            let line = &line[..line.len().saturating_sub(1)];
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            if line.len() > SNAPSHOT_MAX_LINE_BYTES {
                return Err(AppError::bad_request(format!(
                    "Snapshot record exceeds {SNAPSHOT_MAX_LINE_BYTES} bytes"
                )));
            }
            let record: Value = serde_json::from_slice(line).map_err(|error| {
                AppError::bad_request(format!("Invalid snapshot record {records}: {error}"))
            })?;
            let record_type = record["type"].as_str().unwrap_or_default().to_string();
            if !header_seen {
                if record_type != "thingd.snapshot" || record["version"] != "2.0.0" {
                    return Err(AppError::bad_request("Unsupported Thingd snapshot header"));
                }
                header_seen = true;
                continue;
            }
            if record_type == "error" {
                return Err(AppError::bad_request(
                    record["error"].as_str().unwrap_or("Source snapshot failed"),
                ));
            }

            let permit = blocking_permit()?;
            let engine = Arc::clone(&e);
            let record_type_for_worker = record_type.clone();
            let result = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let mut guard = engine.lock();
                let result: Result<(), String> = match record_type_for_worker.as_str() {
                    "object" => {
                        let object: MemoryObject = serde_json::from_value(record["object"].clone())
                            .map_err(|error| format!("invalid object: {error}"))?;
                        guard
                            .put_object(object)
                            .map(|_| ())
                            .map_err(|error| error.to_string())
                    },
                    "event" => {
                        let event: MemoryEvent = serde_json::from_value(record["event"].clone())
                            .map_err(|error| format!("invalid event: {error}"))?;
                        guard
                            .append_event(event)
                            .map(|_| ())
                            .map_err(|error| error.to_string())
                    },
                    "queue" => {
                        let job: QueueJob = serde_json::from_value(record["job"].clone())
                            .map_err(|error| format!("invalid queue job: {error}"))?;
                        guard
                            .push_job(job)
                            .map(|_| ())
                            .map_err(|error| error.to_string())
                    },
                    other => Err(format!("unsupported snapshot record type '{other}'")),
                };
                finish_blocking();
                result
            })
            .await
            .map_err(|error| {
                AppError::internal(format!("snapshot import worker failed: {error}"))
            })?;
            result.map_err(|error| {
                AppError::bad_request(format!("Snapshot record {records} failed: {error}"))
            })?;
            records += 1;
            match record_type.as_str() {
                "object" => objects += 1,
                "event" => events += 1,
                "queue" => queues += 1,
                _ => {},
            }
        }
    }

    if !buffer.iter().all(u8::is_ascii_whitespace) {
        return Err(AppError::bad_request(
            "Snapshot ended with an incomplete JSONL record",
        ));
    }
    if !header_seen {
        return Err(AppError::bad_request("Snapshot is missing its header"));
    }
    Ok(Json(
        json!({ "data": { "records": records, "objects": objects, "events": events, "queues": queues } }),
    ))
}

// ─── Queues ─────────────────────────────────────────────────────

pub async fn push_job(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(queue): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let payload = body.get("payload").cloned().unwrap_or(body.clone());
    let max_at = body
        .get("maxAttempts")
        .and_then(|v| v.as_u64())
        .unwrap_or(3) as u32;
    let job = QueueJob::new(
        &queue,
        uuid::Uuid::new_v4().to_string(),
        payload.to_string(),
        max_at,
    );
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    ok(g.push_job(job)
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn claim_job(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(queue): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let opts = QueueClaimOptions {
        lease_ms: body
            .get("leaseMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30000),
    };
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    match g
        .claim_job_with_options(&queue, opts)
        .map_err(|e| AppError::internal(e.to_string()))?
    {
        Some(job) => ok(job),
        None => Ok(Json(json!({ "data": null }))),
    }
}

pub async fn ack_job(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(queue): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let job_id = body["jobId"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing jobId"))?;
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    match g
        .ack_job(&queue, job_id)
        .map_err(|e| AppError::internal(e.to_string()))?
    {
        Some(job) => ok(job),
        None => Err(AppError::bad_request("Ack failed")),
    }
}

pub async fn nack_job(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(queue): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let job_id = body["jobId"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing jobId"))?;
    let opts = QueueNackOptions {
        delay_ms: body.get("delayMs").and_then(|v| v.as_u64()).unwrap_or(0),
        error: body
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    };
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    match g
        .nack_job_with_options(&queue, job_id, opts)
        .map_err(|e| AppError::internal(e.to_string()))?
    {
        Some(job) => ok(job),
        None => Err(AppError::bad_request("Nack failed")),
    }
}

pub async fn list_jobs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(queue): Path<String>,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    ok(g.list_jobs(&queue)
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn list_dead_jobs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(queue): Path<String>,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    ok(g.list_dead_jobs(&queue)
        .map_err(|e| AppError::internal(e.to_string()))?)
}

// ─── Links ──────────────────────────────────────────────────────

pub async fn create_link(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let from_ref = body["fromRef"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing fromRef"))?;
    let link_type = body["linkType"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing linkType"))?;
    let to_ref = body["toRef"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing toRef"))?;
    let mut link = Link::new(from_ref, link_type, to_ref);
    if let Some(w) = body.get("weight").and_then(|v| v.as_f64()) {
        link.weight = Some(w);
    }
    if let Some(m) = body.get("metadataJson").and_then(|v| v.as_str()) {
        link.metadata_json = m.to_string();
    }
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    ok(g.create_link(link)
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn get_links(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, AppError> {
    if let Some(reference) = params.get("reference").and_then(|v| v.as_str()) {
        let d = params
            .get("direction")
            .and_then(|v| v.as_str())
            .unwrap_or("Both");
        let dir = match d {
            "Outgoing" => LinkDirection::Outgoing,
            "Incoming" => LinkDirection::Incoming,
            _ => LinkDirection::Both,
        };
        let opts = LinkQueryOptions {
            link_type: params
                .get("linkType")
                .and_then(|v| v.as_str())
                .map(String::from),
            limit: params
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize),
        };
        let e = get_engine(&state, &headers)?;
        let g = e.lock();
        return ok(g
            .get_neighbors(reference, dir, opts)
            .map_err(|e| AppError::internal(e.to_string()))?);
    }
    Err(AppError::bad_request("Missing 'reference' or 'id'"))
}

pub async fn get_link_by_id(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    match g
        .get_link(&id)
        .map_err(|e| AppError::internal(e.to_string()))?
    {
        Some(l) => ok(l),
        None => Err(AppError::not_found("Link not found")),
    }
}

pub async fn delete_link(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    ok(json!({ "deleted": g.delete_link(&id).map_err(|e| AppError::internal(e.to_string()))? }))
}

// ─── Connectors ─────────────────────────────────────────────────

pub async fn list_connectors() -> Result<Json<Value>, AppError> {
    Ok(Json(json!({ "data": ["file", "postgres", "mysql"] })))
}

fn build_connector_auth(body: &Value) -> Option<ConnectorAuth> {
    let auth = body.get("auth")?;
    Some(ConnectorAuth {
        username: auth["username"].as_str().unwrap_or("").to_string(),
        password: auth["password"].as_str().unwrap_or("").to_string(),
        host: auth["host"].as_str().unwrap_or("").to_string(),
        port: auth["port"].as_u64().unwrap_or(5432) as u16,
        database: auth["database"].as_str().unwrap_or("").to_string(),
        ssl_mode: match auth.get("sslMode").and_then(|v| v.as_str()) {
            Some("disable") => SslMode::Disable,
            Some("require") => SslMode::Require,
            _ => SslMode::Prefer,
        },
    })
}

fn build_connector_config(connector_type: &str, body: &Value) -> ConnectorConfig {
    ConnectorConfig {
        connector_type: connector_type.to_string(),
        source: body
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        collection: body
            .get("collection")
            .and_then(|v| v.as_str())
            .unwrap_or("imported")
            .to_string(),
        sync_strategy: match body.get("syncStrategy").and_then(|v| v.as_str()) {
            Some("incremental") => SyncStrategy::Incremental {
                cursor_column: String::new(),
            },
            _ => SyncStrategy::Full,
        },
        query: body
            .get("query")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        column_mapping: body
            .get("columnMapping")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or(k).to_string()))
                    .collect()
            }),
        auth: build_connector_auth(body),
        batch_size: body
            .get("batchSize")
            .and_then(|v| v.as_u64())
            .unwrap_or(1000) as usize,
    }
}

fn validate_connector_access(
    connector_type: &str,
    config: &ConnectorConfig,
    hardening: &crate::config::HardeningConfig,
) -> Result<(), AppError> {
    match connector_type {
        "file" => {
            let root = hardening
                .connector_file_root
                .as_deref()
                .ok_or_else(|| AppError::forbidden("File connectors are disabled"))?;
            let root = std::fs::canonicalize(root)
                .map_err(|_| AppError::bad_request("Configured connector file root is invalid"))?;
            let path = std::fs::canonicalize(&config.source)
                .map_err(|_| AppError::bad_request("Connector file does not exist"))?;
            if !path.starts_with(&root) || !path.is_file() {
                return Err(AppError::forbidden(
                    "Connector file is outside the allowed root",
                ));
            }
            let size = std::fs::metadata(&path)
                .map_err(|_| AppError::bad_request("Unable to inspect connector file"))?
                .len();
            if size > hardening.max_connector_file_bytes {
                return Err(AppError::bad_request(
                    "Connector file exceeds the configured size limit",
                ));
            }
        },
        "postgres" | "mysql" => {
            let auth = config
                .auth
                .as_ref()
                .ok_or_else(|| AppError::bad_request("Connector auth is required"))?;
            if !hardening
                .connector_allowed_hosts
                .iter()
                .any(|host| host == "*" || host == &auth.host)
            {
                return Err(AppError::forbidden("Connector host is not allowlisted"));
            }
            if hardening.connector_require_tls && auth.ssl_mode == SslMode::Disable {
                return Err(AppError::bad_request(
                    "Encrypted connector transport is required",
                ));
            }
        },
        _ => {},
    }
    Ok(())
}

fn get_connector(connector_type: &str) -> Result<Box<dyn Connector>, AppError> {
    match connector_type {
        "postgres" => Ok(Box::new(PostgresConnector::new())),
        "mysql" => Ok(Box::new(MysqlConnector::new())),
        "file" => Ok(Box::new(FileConnector)),
        _ => Err(AppError::bad_request(format!(
            "Unknown connector type: {connector_type}"
        ))),
    }
}

pub async fn discover_schema(
    State(state): State<Arc<AppState>>,
    Path(connector_type): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let connector = get_connector(&connector_type)?;
    let config = build_connector_config(&connector_type, &body);
    validate_connector_access(&connector_type, &config, &state.hardening_config)?;
    let schema = tokio::task::spawn_blocking(move || connector.discover_schema(&config))
        .await
        .map_err(|e| AppError::internal(format!("Connector task failed: {e}")))?
        .map_err(|e| AppError::internal(e.to_string()))?;

    let columns: Vec<Value> = schema
        .columns
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "dataType": match c.data_type {
                    ColumnType::Text => "text",
                    ColumnType::Integer => "integer",
                    ColumnType::Float => "float",
                    ColumnType::Boolean => "boolean",
                    ColumnType::Timestamp => "timestamp",
                    ColumnType::Json => "json",
                    ColumnType::Unknown => "unknown",
                },
                "nullable": c.nullable,
                "sampleValues": c.sample_values,
            })
        })
        .collect();

    ok(json!({
        "name": schema.name,
        "columns": columns,
        "estimatedRows": schema.estimated_rows,
    }))
}

pub async fn list_connector_tables(
    State(state): State<Arc<AppState>>,
    Path(connector_type): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let connector = get_connector(&connector_type)?;
    let config = build_connector_config(&connector_type, &body);
    validate_connector_access(&connector_type, &config, &state.hardening_config)?;
    let tables = tokio::task::spawn_blocking(move || connector.list_tables(&config))
        .await
        .map_err(|e| AppError::internal(format!("Connector task failed: {e}")))?
        .map_err(|e| AppError::internal(e.to_string()))?;

    ok(json!({ "tables": tables }))
}

pub async fn ping_connector(
    State(state): State<Arc<AppState>>,
    Path(connector_type): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let connector = get_connector(&connector_type)?;
    let config = build_connector_config(&connector_type, &body);
    validate_connector_access(&connector_type, &config, &state.hardening_config)?;
    let result = tokio::task::spawn_blocking(move || connector.list_tables(&config))
        .await
        .map_err(|e| AppError::internal(format!("Connector task failed: {e}")))?;
    match result {
        Ok(_tables) => ok(json!({ "ok": true, "connector": connector_type })),
        Err(e) => Err(AppError::bad_request(format!("Connection failed: {e}"))),
    }
}

pub async fn pull_data(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(connector_type): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let connector = get_connector(&connector_type)?;
    let config = build_connector_config(&connector_type, &body);
    validate_connector_access(&connector_type, &config, &state.hardening_config)?;
    let collection = config.collection.clone();
    let batch_size = config.batch_size;
    let return_objects = body
        .get("returnObjects")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let stream = tokio::task::spawn_blocking(move || connector.pull(&config))
        .await
        .map_err(|e| AppError::internal(format!("Connector task failed: {e}")))?
        .map_err(|e| AppError::internal(e.to_string()))?;

    let mut imported = 0u64;
    let mut batch: Vec<MemoryObject> = Vec::new();
    let mut returned_objects: Vec<Value> = Vec::new();

    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();

    for row in stream {
        let row = row.map_err(|e| AppError::internal(e.to_string()))?;
        let id = row
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let body_str =
            serde_json::to_string(&row).map_err(|e| AppError::internal(e.to_string()))?;
        let obj = MemoryObject::new(&collection, &id, &body_str);
        if return_objects {
            returned_objects.push(json!({ "id": id, "collection": collection, "body": row }));
        }
        batch.push(obj);

        if batch.len() >= batch_size {
            g.put_objects_batch(std::mem::take(&mut batch))
                .map_err(|e| AppError::internal(e.to_string()))?;
            imported += batch_size as u64;
        }
    }

    if !batch.is_empty() {
        let count = batch.len();
        g.put_objects_batch(batch)
            .map_err(|e| AppError::internal(e.to_string()))?;
        imported += count as u64;
    }

    let mut response = json!({ "imported": imported, "collection": collection });
    if return_objects {
        response["objects"] = Value::Array(returned_objects);
    }
    ok(response)
}

// ─── Search ─────────────────────────────────────────────────────

pub async fn search(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let query = body["query"].as_str().unwrap_or("").to_string();
    if query.is_empty() {
        return ok(Vec::<Value>::new());
    }
    let opts = SearchOptions {
        collections: body.get("collections").and_then(|v| v.as_array()).map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        }),
        limit: body
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize),
        filter: body.get("filter").cloned(),
    };
    let e = get_engine(&state, &headers)?;
    let permit = blocking_permit()?;
    let hits = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let g = e.lock();
        let result = g
            .search(&query, opts)
            .map_err(|error| AppError::internal(error.to_string()));
        finish_blocking();
        result
    })
    .await
    .map_err(|error| AppError::internal(format!("search operation failed: {error}")))??;
    ok(hits)
}
// ─── Vector Search ─────────────────────────────────────────────

pub async fn vector_search(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let collection = body["collection"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing 'collection'"))?;
    let query_vector: Vec<f32> = body["vector"]
        .as_array()
        .ok_or_else(|| AppError::bad_request("Missing or invalid 'vector'"))?
        .iter()
        .map(|v| {
            v.as_f64()
                .map(|f| f as f32)
                .ok_or_else(|| AppError::bad_request("'vector' must contain only numbers"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let opts = VectorSearchOptions {
        top_k: body
            .get("topK")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize),
        filter: body.get("filter").cloned(),
    };

    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    ok(g.vector_search(collection, &query_vector, opts)
        .map_err(AppError::from)?)
}

// ─── NLQ ───────────────────────────────────────────────────────

pub async fn nlq_query(
    State(state): State<Arc<AppState>>,
    _headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    if !state.nlq_config.enabled {
        return Err(AppError::bad_request(
            "NLQ is not enabled. Set nlq.enabled=true or THINGD_NLQ_ENABLED=true.",
        ));
    }

    let question = body["question"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing 'question'"))?
        .to_string();

    let collection = body
        .get("collection")
        .and_then(|v| v.as_str())
        .map(String::from);

    let result = tokio::runtime::Handle::current().block_on(crate::nlq::execute_nlq(
        &state.pool,
        &state.nlq_config,
        &question,
        collection.as_deref(),
    ));

    match result {
        Ok(r) => ok(r),
        Err(e) => Err(AppError::bad_request(e)),
    }
}

// ─── Aggregate ──────────────────────────────────────────────────

pub async fn aggregate(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let collection = body["collection"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing 'collection'"))?;
    let function_str = body["function"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing 'function'"))?;
    let function = match function_str {
        "sum" => AggregateFunction::Sum,
        "avg" => AggregateFunction::Avg,
        "min" => AggregateFunction::Min,
        "max" => AggregateFunction::Max,
        _ => AggregateFunction::Count,
    };
    let field = body.get("field").and_then(|v| v.as_str()).map(String::from);
    let group_by = body
        .get("groupBy")
        .and_then(|v| v.as_str())
        .map(String::from);
    let filter = body
        .get("filter")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let opts = AggregateOptions {
        function,
        field,
        filter,
        group_by,
    };

    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    ok(g.aggregate(collection, &opts)
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn timeseries(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let collection = body["collection"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing 'collection'"))?;
    let function_str = body["function"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing 'function'"))?;
    let function = match function_str {
        "sum" => AggregateFunction::Sum,
        "avg" => AggregateFunction::Avg,
        "min" => AggregateFunction::Min,
        "max" => AggregateFunction::Max,
        _ => AggregateFunction::Count,
    };
    let bucket_str = body["bucket"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing 'bucket'"))?;
    let bucket = match bucket_str {
        "hour" => TimeBucket::Hour,
        "week" => TimeBucket::Week,
        "month" => TimeBucket::Month,
        _ => TimeBucket::Day,
    };
    let field = body.get("field").and_then(|v| v.as_str()).map(String::from);
    let filter = body
        .get("filter")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let from = body.get("from").and_then(|v| v.as_str()).map(String::from);
    let to = body.get("to").and_then(|v| v.as_str()).map(String::from);

    let opts = TimeSeriesOptions {
        function,
        bucket,
        field,
        filter,
        from,
        to,
    };

    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    ok(g.timeseries(collection, &opts)
        .map_err(|e| AppError::internal(e.to_string()))?)
}

// ─── Schema ────────────────────────────────────────────────────

pub async fn list_schemas(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    use thingd::SchemaOptions;
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    let schemas = g
        .schema(None, &SchemaOptions::default())
        .map_err(|e| AppError::internal(e.to_string()))?;
    ok(schemas)
}

pub async fn get_schema(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(collection): Path<String>,
) -> Result<Json<Value>, AppError> {
    use thingd::SchemaOptions;
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    let schemas = g
        .schema(Some(&collection), &SchemaOptions::default())
        .map_err(|e| AppError::internal(e.to_string()))?;
    match schemas.into_iter().next() {
        Some(schema) => ok(schema),
        None => Err(AppError::not_found(format!(
            "Collection '{collection}' not found or has no objects"
        ))),
    }
}

/// Parse and validate a schema source document without changing the engine.
pub async fn validate_schema(Json(body): Json<Value>) -> Result<Json<Value>, AppError> {
    let source = body
        .get("source")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("Missing source"))?;
    if source.trim().is_empty() {
        return Err(AppError::bad_request("Schema source must not be empty"));
    }
    let schema =
        thingd_schema::parse(source).map_err(|error| AppError::bad_request(error.to_string()))?;
    let hash = schema
        .hash()
        .map_err(|error| AppError::internal(error.to_string()))?;
    ok(json!({ "schema": schema, "hash": hash }))
}

/// Return the last applied canonical schema document.
pub async fn current_schema(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    ok(g.get_schema_document()
        .map_err(|error| AppError::internal(error.to_string()))?)
}

/// Persist canonical schema metadata after an explicit migration approval.
pub async fn put_schema_document(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let schema_json = body
        .get("schemaJson")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("Missing schemaJson"))?;
    let hash = body
        .get("hash")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("Missing hash"))?;
    let updated_at = body
        .get("updatedAt")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("Missing updatedAt"))?;
    serde_json::from_str::<Value>(schema_json)
        .map_err(|error| AppError::bad_request(format!("Invalid schemaJson: {error}")))?;
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    g.put_schema_document(StoredSchema {
        schema_json: schema_json.to_string(),
        hash: hash.to_string(),
        updated_at: updated_at.to_string(),
    })
    .map_err(|error| AppError::internal(error.to_string()))?;
    ok(json!({ "stored": true, "hash": hash }))
}

/// Return durable migration records in application order.
pub async fn list_migrations(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let e = get_engine(&state, &headers)?;
    let g = e.lock();
    ok(g.list_migrations()
        .map_err(|error| AppError::internal(error.to_string()))?)
}

/// Record an explicitly approved schema migration.
pub async fn record_migration(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_source_writable(&state, &headers)?;
    let id = body
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("Missing migration id"))?;
    let hash = body
        .get("hash")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("Missing migration hash"))?;
    let applied_at = body
        .get("appliedAt")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("Missing migration timestamp"))?;
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    g.record_migration(MigrationRecord {
        id: id.to_string(),
        hash: hash.to_string(),
        applied_at: applied_at.to_string(),
    })
    .map_err(|error| AppError::internal(error.to_string()))?;
    ok(json!({ "recorded": true, "id": id, "hash": hash }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::config::Config;
    use crate::config::HardeningConfig;
    use crate::engine::EnginePool;

    fn state_for_config(config: &Config) -> Arc<AppState> {
        Arc::new(AppState {
            pool: Arc::new(EnginePool::new(":memory:".to_string())),
            tenant_config: config.tenant.clone(),
            mcp_config: config.mcp.clone(),
            auth_token: config.auth.token.clone(),
            tenant_tokens: config.auth.tenant_tokens.clone(),
            auth_verifier: None,
            allow_unauthenticated: config.auth.allow_unauthenticated,
            cluster_config: config.cluster.clone(),
            nlq_config: config.nlq.clone(),
            sync_config: config.sync.clone(),
            hardening_config: config.hardening.clone(),
        })
    }

    fn test_state_and_config() -> (Arc<AppState>, Config) {
        let config = Config::default();
        (state_for_config(&config), config)
    }

    fn test_state_and_config_with_config(config: Config) -> (Arc<AppState>, Config) {
        (state_for_config(&config), config)
    }

    #[tokio::test]
    async fn test_health() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_ready_reports_primary_storage_ready() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["data"]["status"], "ready");
    }

    #[test]
    fn saturated_worker_response_includes_retry_after() {
        let response = crate::error::AppError::overloaded("busy").into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::RETRY_AFTER], "1");
    }

    #[tokio::test]
    async fn schema_validation_returns_canonical_hash() {
        let response = validate_schema(Json(json!({
            "source": "version 1\ncollection users {\n id: string @id\n}\n"
        })))
        .await
        .expect("schema should validate");
        assert!(
            response.0["data"]["hash"]
                .as_str()
                .unwrap_or_default()
                .starts_with("sha256:")
        );
        assert_eq!(response.0["data"]["schema"]["version"], 1);
    }

    #[tokio::test]
    async fn schema_validation_rejects_empty_source() {
        let error = validate_schema(Json(json!({ "source": "  " })))
            .await
            .expect_err("empty schema should fail");
        assert!(error.detail.contains("must not be empty"));
    }

    #[tokio::test]
    async fn schema_metadata_can_be_stored_and_migration_recorded() {
        let (state, _config) = test_state_and_config();
        let headers = HeaderMap::new();
        let _ = put_schema_document(
            State(Arc::clone(&state)),
            headers.clone(),
            Json(json!({
                "schemaJson": "{\"version\":1,\"collections\":[]}",
                "hash": "sha256:test",
                "updatedAt": "2026-08-09T00:00:00Z"
            })),
        )
        .await
        .expect("schema metadata should be stored");
        let _ = record_migration(
            State(Arc::clone(&state)),
            headers,
            Json(json!({
                "id": "001_initial",
                "hash": "sha256:test",
                "appliedAt": "2026-08-09T00:00:00Z"
            })),
        )
        .await
        .expect("migration should be recorded");
        let current = current_schema(State(state.clone()), HeaderMap::new())
            .await
            .expect("schema should be readable");
        assert_eq!(current.0["data"]["hash"], "sha256:test");
        let migrations = list_migrations(State(state), HeaderMap::new())
            .await
            .expect("migrations should be readable");
        assert_eq!(migrations.0["data"][0]["id"], "001_initial");
    }

    #[tokio::test]
    async fn multi_tenant_auth_binds_token_to_tenant() {
        let root =
            std::env::temp_dir().join(format!("thingd-tenant-auth-{}", uuid::Uuid::new_v4()));
        let mut config = Config::default();
        config.tenant.mode = crate::config::TenantMode::MultiTenant;
        config.tenant.database_prefix = format!("{}/", root.display());
        config.auth.token.clear();
        config.auth.tenant_tokens.insert(
            "tenant-a".to_string(),
            "tenant-a-token-that-is-long-enough".to_string(),
        );
        let state = state_for_config(&config);
        let app = crate::server::build_router(Arc::clone(&state), &config);

        let valid = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/collections")
                    .header("x-tenant-id", "tenant-a")
                    .header("authorization", "Bearer tenant-a-token-that-is-long-enough")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(valid.status(), StatusCode::OK);

        let invalid = app
            .oneshot(
                Request::builder()
                    .uri("/v1/collections")
                    .header("x-tenant-id", "tenant-a")
                    .header("authorization", "Bearer another-tenant-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::UNAUTHORIZED);

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn cluster_topology_requires_authentication() {
        let mut config = Config::default();
        config.auth.token = "server-token-that-is-long-enough".to_string();
        let state = state_for_config(&config);
        let app = crate::server::build_router(Arc::clone(&state), &config);

        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/cluster/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let authorized = app
            .oneshot(
                Request::builder()
                    .uri("/cluster/status")
                    .header("authorization", "Bearer server-token-that-is-long-enough")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authorized.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_metrics() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::default())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("thingd_objects_total"));
        assert!(text.contains("thingd_events_total"));
        assert!(text.contains("thingd_queues_total"));
    }

    #[tokio::test]
    async fn test_v1_health() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_put_and_get_object() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/v1/objects/test/obj1")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"name":"Alice","val":42}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/objects/test/obj1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        let data = json.get("data").unwrap();
        assert_eq!(data["id"], "obj1");
        assert_eq!(data["collection"], "test");
        assert_eq!(data["body"]["name"], "Alice");
        assert_eq!(data["body"]["val"], 42);
        assert!(data.get("version").is_some());
        assert!(data.get("createdAt").is_some());
        assert!(data.get("updatedAt").is_some());
    }

    #[tokio::test]
    async fn test_get_object_not_found() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/objects/test/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_object() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        let body = r#"{"id":"del1"}"#;
        app.clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/v1/objects/test/del1")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/objects/test/del1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_objects() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/objects?collection=test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_objects_missing_collection() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/objects")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_search() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        let body = r#"{"query":"test"}"#;
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/search")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_search_missing_query() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/search")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_vector_search() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        // Put an object with a vector via REST
        let body = r#"{"id":"doc1","text":"alpha","vector":[1.0,0.0,0.0]}"#;
        app.clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/v1/objects/v/doc1")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Put another
        let body = r#"{"id":"doc2","text":"beta","vector":[0.0,1.0,0.0]}"#;
        app.clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/v1/objects/v/doc2")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Vector search
        let search_body = r#"{"collection":"v","vector":[0.9,0.1,0.0],"topK":5}"#;
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/search/vector")
                    .header("content-type", "application/json")
                    .body(Body::from(search_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let collected = response.into_body().collect().await.unwrap_or_default();
        let bytes = collected.to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        let hits = json["data"].as_array().unwrap();
        assert!(!hits.is_empty(), "expected at least one vector search hit");
        assert_eq!(hits[0]["id"], "doc1");
        assert!(hits[0]["score"].as_f64().unwrap() > hits[1]["score"].as_f64().unwrap());
    }

    #[tokio::test]
    async fn test_vector_search_rejects_dimension_mismatch() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/v1/objects/v/doc1")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"id":"doc1","vector":[1.0,0.0]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/search/vector")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"collection":"v","vector":[1.0,0.0,0.0]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_append_event() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        let body = r#"{"type":"test.event","data":"hello"}"#;
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/events/mystream")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_append_event_idempotency_key_is_replayable() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let body = r#"{"type":"imported","value":1,"idempotencyKey":"snapshot-event-1"}"#;

        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/events/import")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        let first_body = first.into_body().collect().await.unwrap().to_bytes();
        let first_value: Value = serde_json::from_slice(&first_body).unwrap();

        let second = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/events/import")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        let second_body = second.into_body().collect().await.unwrap().to_bytes();
        let second_value: Value = serde_json::from_slice(&second_body).unwrap();

        assert_eq!(
            first_value["data"]["sequence"],
            second_value["data"]["sequence"]
        );
    }

    #[tokio::test]
    async fn test_replication_feed_and_apply() {
        let (source_state, source_config) = test_state_and_config();
        let source_app = crate::server::build_router(Arc::clone(&source_state), &source_config);
        source_app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/v1/objects/notes/n1")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"text":"hello"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        let feed_response = source_app
            .oneshot(
                Request::builder()
                    .uri("/v1/replication/events?after=0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(feed_response.status(), StatusCode::OK);
        let feed_bytes = feed_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let feed: Value = serde_json::from_slice(&feed_bytes).unwrap();
        assert_eq!(feed["data"]["changes"].as_array().unwrap().len(), 1);

        let mut replica_config = Config::default();
        replica_config.sync.role = crate::config::SyncRole::Replica;
        let (replica_state, _) = test_state_and_config_with_config(replica_config.clone());
        let replica_app = crate::server::build_router(replica_state, &replica_config);
        let apply_response = replica_app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/replication/apply")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "changes": feed["data"]["changes"] }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(apply_response.status(), StatusCode::OK);

        let retry_response = replica_app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/replication/apply")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "changes": feed["data"]["changes"] }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let retry_bytes = retry_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let retry: Value = serde_json::from_slice(&retry_bytes).unwrap();
        assert_eq!(retry["data"]["skipped"], 1);

        let object_response = replica_app
            .oneshot(
                Request::builder()
                    .uri("/v1/objects/notes/n1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(object_response.status(), StatusCode::OK);
        let object_bytes = object_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let object: Value = serde_json::from_slice(&object_bytes).unwrap();
        assert_eq!(object["data"]["body"]["text"], "hello");
    }

    #[tokio::test]
    async fn test_replica_rejects_direct_object_writes() {
        let mut config = Config::default();
        config.sync.role = crate::config::SyncRole::Replica;
        let (state, _) = test_state_and_config_with_config(config.clone());
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/v1/objects/notes/n1")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"text":"blocked"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_replication_preserves_source_metadata_and_tombstone() {
        let mut config = Config::default();
        config.sync.role = crate::config::SyncRole::Replica;
        let (state, _) = test_state_and_config_with_config(config.clone());
        let app = crate::server::build_router(state, &config);
        let apply = serde_json::json!({
            "changes": [{
                "sourceId": "source-a",
                "cursor": 1,
                "idempotencyKey": "source-a:1",
                "change": {
                    "operation": "object.upsert",
                    "collection": "notes",
                    "id": "n1",
                    "payload": {
                        "body": {"text": "hello"},
                        "version": 7,
                        "createdAt": "2025-01-01T00:00:00.000Z",
                        "updatedAt": "2025-01-02T00:00:00.000Z"
                    }
                }
            }]
        });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/replication/apply")
                    .header("content-type", "application/json")
                    .body(Body::from(apply.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let object = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/objects/notes/n1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let object: Value = serde_json::from_slice(&object).unwrap();
        assert_eq!(object["data"]["version"], 7);
        assert_eq!(object["data"]["createdAt"], "2025-01-01T00:00:00.000Z");
        assert_eq!(object["data"]["updatedAt"], "2025-01-02T00:00:00.000Z");

        let delete = serde_json::json!({
            "changes": [{
                "sourceId": "source-a",
                "cursor": 2,
                "idempotencyKey": "source-a:2",
                "change": {"operation": "object.delete", "collection": "notes", "id": "n1"}
            }]
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/replication/apply")
                    .header("content-type", "application/json")
                    .body(Body::from(delete.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_cloud_replication_target_requires_explicit_opt_in() {
        let mut config = Config::default();
        config.sync.role = crate::config::SyncRole::Replica;
        config.sync.provider = "thingd.cloud".to_string();
        let (state, _) = test_state_and_config_with_config(config.clone());
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/replication/apply")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"changes":[]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_append_event_missing_type() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/events/mystream")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"data":"hello"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_list_events() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/events?stream=mystream")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_queue_push_claim_ack() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        let body = r#"{"payload":"test-job"}"#;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/queues/myqueue/push")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/queues/myqueue/claim")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"leaseMs":30000}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/queues/myqueue/jobs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_create_link() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        let body = r#"{"fromRef":"obj1","linkType":"references","toRef":"obj2"}"#;
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/links")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_create_link_missing_fields() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/links")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_count_objects() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/counts/objects")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["data"]["count"].is_number());
    }

    #[tokio::test]
    async fn test_count_events() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/counts/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["data"]["count"].is_number());
    }

    #[tokio::test]
    async fn test_count_links() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/counts/links")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["data"]["count"].is_number());
    }

    #[tokio::test]
    async fn test_list_collections() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/collections")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["data"].is_array());
    }

    #[tokio::test]
    async fn test_list_streams() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/streams")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["data"].is_array());
    }

    #[tokio::test]
    async fn test_list_queues() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/queues")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["data"].is_array());
    }

    #[tokio::test]
    async fn test_batch_upsert() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let body = r#"[{"id":"b1","name":"Bob"},{"id":"b2","name":"Alice"}]"#;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/v1/objects/batch?collection=users")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        let arr = json["data"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[tokio::test]
    async fn test_batch_upsert_body_only_does_not_store_id_in_body() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/v1/objects/batch?collection=users&bodyOnly=true")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"[{"id":"batch-1","name":"Zoe"}]"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/objects/users/batch-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["data"]["id"], "batch-1");
        assert!(json["data"]["body"]["id"].is_null());
    }

    #[tokio::test]
    async fn test_snapshot_export_and_import_round_trip() {
        let (source_state, source_config) = test_state_and_config();
        let source_app = crate::server::build_router(Arc::clone(&source_state), &source_config);
        source_app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/v1/objects/notes/n1")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"text":"hello"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        source_app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/events/import")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"type":"note.created","value":1,"idempotencyKey":"event-1"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let snapshot = source_app
            .oneshot(
                Request::builder()
                    .uri("/v1/snapshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(snapshot.status(), StatusCode::OK);
        assert_eq!(
            snapshot.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/x-ndjson"
        );
        let snapshot_bytes = snapshot.into_body().collect().await.unwrap().to_bytes();
        let snapshot_text = String::from_utf8(snapshot_bytes.to_vec()).unwrap();
        assert!(
            snapshot_text
                .lines()
                .any(|line| line.contains("thingd.snapshot"))
        );
        assert!(
            snapshot_text
                .lines()
                .any(|line| line.contains("note.created"))
        );

        let (target_state, target_config) = test_state_and_config();
        let target_app = crate::server::build_router(target_state, &target_config);
        let response = target_app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/snapshot")
                    .header("content-type", "application/x-ndjson")
                    .body(Body::from(snapshot_text))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = target_app
            .oneshot(
                Request::builder()
                    .uri("/v1/objects/notes/n1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["data"]["body"]["text"], "hello");
    }

    #[tokio::test]
    async fn test_batch_delete() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        // First create objects
        app.clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/v1/objects/deltest/d1")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"id":"d1"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        app.clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/v1/objects/deltest/d2")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"id":"d2"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = r#"["d1","d2"]"#;
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/objects/batch?collection=deltest")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["data"]["deleted"], 2);
    }

    #[tokio::test]
    async fn test_queue_nack() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        // Push a job
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/queues/nackq/push")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"payload":"job"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Claim it
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/queues/nackq/claim")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        let job_id = json["data"]["id"].as_str().unwrap();

        // Nack it
        let body = format!(r#"{{"jobId":"{}","error":"failed"}}"#, job_id);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/queues/nackq/nack")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_queue_dead() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/queues/deadq/dead")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["data"].is_array());
    }

    #[tokio::test]
    async fn test_get_link_by_id() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        // Create a link first
        let body = r#"{"fromRef":"a","linkType":"refs","toRef":"b"}"#;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/links")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        let link_id = json["data"]["id"].as_str().unwrap();

        // Get by ID
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/links/{}", link_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_delete_link() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        // Create a link
        let body = r#"{"fromRef":"x","linkType":"dep","toRef":"y"}"#;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/links")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        let link_id = json["data"]["id"].as_str().unwrap();

        // Delete it
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/links/{}", link_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_neighbors() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        // Create links
        let body1 = r#"{"fromRef":"user/alice","linkType":"authored","toRef":"post/1"}"#;
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/links")
                    .header("content-type", "application/json")
                    .body(Body::from(body1))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body2 = r#"{"fromRef":"user/alice","linkType":"authored","toRef":"post/2"}"#;
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/links")
                    .header("content-type", "application/json")
                    .body(Body::from(body2))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Get neighbors
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/links?reference=user/alice&direction=Outgoing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        let arr = json["data"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[tokio::test]
    async fn test_list_connectors() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/connectors")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        let arr = json["data"].as_array().unwrap();
        assert!(arr.contains(&Value::String("file".to_string())));
    }

    #[tokio::test]
    async fn test_connector_tables_rejects_unknown_type() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/connectors/unknown/tables")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn connector_access_is_deny_by_default() {
        let config = ConnectorConfig {
            connector_type: "file".to_string(),
            source: "/etc/passwd.csv".to_string(),
            ..Default::default()
        };
        assert!(validate_connector_access("file", &config, &HardeningConfig::default()).is_err());

        let config = ConnectorConfig {
            connector_type: "postgres".to_string(),
            auth: Some(ConnectorAuth {
                username: "u".to_string(),
                password: "p".to_string(),
                host: "127.0.0.1".to_string(),
                port: 5432,
                database: "db".to_string(),
                ssl_mode: SslMode::Require,
            }),
            ..Default::default()
        };
        assert!(
            validate_connector_access("postgres", &config, &HardeningConfig::default()).is_err()
        );
    }

    #[tokio::test]
    async fn test_aggregate_count() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        // Seed some objects
        for i in 0..5 {
            let body = format!(r#"{{"id":"obj{i}","value":{i}}}"#);
            app.clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri(format!("/v1/objects/test/obj{i}"))
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
        }

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/aggregate")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"collection":"test","function":"count"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["data"]["total"], 5.0);
    }

    #[tokio::test]
    async fn test_aggregate_sum() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        for i in 0..5 {
            let body = format!(r#"{{"id":"obj{i}","value":{i}}}"#);
            app.clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri(format!("/v1/objects/test/obj{i}"))
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
        }

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/aggregate")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"collection":"test","function":"sum","field":"value"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["data"]["total"], 10.0);
    }

    #[tokio::test]
    async fn test_aggregate_timeseries() {
        let (state, config) = test_state_and_config();
        let app = crate::server::build_router(state, &config);

        // Seed objects with different timestamps
        let objects = vec![
            r#"{"id":"obj1","value":1,"createdAt":"2026-01-01T12:00:00Z"}"#,
            r#"{"id":"obj2","value":2,"createdAt":"2026-01-02T12:00:00Z"}"#,
            r#"{"id":"obj3","value":3,"createdAt":"2026-01-03T12:00:00Z"}"#,
        ];
        for obj in objects {
            app.clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/v1/objects/test/obj")
                        .header("content-type", "application/json")
                        .body(Body::from(obj))
                        .unwrap(),
                )
                .await
                .unwrap();
        }

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/aggregate/timeseries")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"collection":"test","function":"count","bucket":"day"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
