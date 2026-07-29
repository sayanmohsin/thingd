use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, header},
    response::IntoResponse,
};
use serde_json::{Value, json};
use std::sync::Arc;
use thingd::*;

use crate::error::AppError;
use crate::server::AppState;

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

pub async fn clear_default_db(State(state): State<Arc<AppState>>) -> Result<Json<Value>, AppError> {
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
         thingd_dead_jobs_total {dead_jobs}\n",
        objects = objects,
        events = events,
        links = links,
        queue_count = queue_count,
        active_jobs = active_jobs,
        dead_jobs = dead_jobs,
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
    let collection = body["collection"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("collection is required"))?;
    let field = body["field"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("field is required"))?;
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    g.create_index(collection, field)
        .map_err(|e| AppError::internal(e.to_string()))?;
    ok(json!({ "created": true }))
}

// ─── Objects ────────────────────────────────────────────────────

pub async fn list_objects(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, AppError> {
    let collection = params["collection"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing collection"))?;
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
    let g = e.lock();
    let objects = g
        .list_objects(Some(&[collection.to_string()]), &opts)
        .map_err(|e| AppError::internal(e.to_string()))?;
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
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    ok(
        json!({ "deleted": g.delete_object(&collection, &id).map_err(|e| AppError::internal(e.to_string()))? }),
    )
}

pub async fn put_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<Value>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let collection = params["collection"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing collection"))?;
    let items = body.as_array().cloned().unwrap_or_default();
    let objects: Vec<MemoryObject> = items
        .iter()
        .enumerate()
        .map(|(i, v)| {
            MemoryObject::new(
                collection.to_string(),
                v["id"].as_str().unwrap_or(&format!("b{}", i)),
                v.to_string(),
            )
        })
        .collect();
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    ok(g.put_objects_batch(objects)
        .map_err(|e| AppError::internal(e.to_string()))?)
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
    let keys: Vec<(String, String)> = ids
        .into_iter()
        .map(|id| (collection.to_string(), id))
        .collect();
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    ok(
        json!({ "deleted": g.delete_objects_batch(&keys).map_err(|e| AppError::internal(e.to_string()))? }),
    )
}

// ─── Events ─────────────────────────────────────────────────────

pub async fn append_event(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stream): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let et = body["type"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing 'type'"))?;

    // Prevent direct writes to the protected audit stream
    if stream == "__thingd:mcp:audit" {
        return Err(AppError::bad_request(
            "Stream '__thingd:mcp:audit' is protected and cannot be written to directly",
        ));
    }

    let event = MemoryEvent::new(stream, et, body.to_string());
    let e = get_engine(&state, &headers)?;
    let mut g = e.lock();
    let r = g
        .append_event(event)
        .map_err(|e| AppError::internal(e.to_string()))?;
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

// ─── Queues ─────────────────────────────────────────────────────

pub async fn push_job(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(queue): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
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
    Path(connector_type): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let connector = get_connector(&connector_type)?;
    let config = build_connector_config(&connector_type, &body);
    let schema = connector
        .discover_schema(&config)
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
    Path(connector_type): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let connector = get_connector(&connector_type)?;
    let config = build_connector_config(&connector_type, &body);
    let tables = connector
        .list_tables(&config)
        .map_err(|e| AppError::internal(e.to_string()))?;

    ok(json!({ "tables": tables }))
}

pub async fn ping_connector(
    Path(connector_type): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let connector = get_connector(&connector_type)?;
    let mut config = build_connector_config(&connector_type, &body);
    config.query = Some("SELECT 1".to_string());
    let result = connector.discover_schema(&config);
    match result {
        Ok(_schema) => ok(json!({ "ok": true, "connector": connector_type })),
        Err(e) => Err(AppError::bad_request(format!("Connection failed: {e}"))),
    }
}

pub async fn pull_data(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(connector_type): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let connector = get_connector(&connector_type)?;
    let config = build_connector_config(&connector_type, &body);
    let collection = config.collection.clone();

    let stream = connector
        .pull(&config)
        .map_err(|e| AppError::internal(e.to_string()))?;

    let mut imported = 0u64;
    let mut batch: Vec<MemoryObject> = Vec::new();

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
        batch.push(obj);

        if batch.len() >= config.batch_size {
            g.put_objects_batch(std::mem::take(&mut batch))
                .map_err(|e| AppError::internal(e.to_string()))?;
            imported += config.batch_size as u64;
        }
    }

    if !batch.is_empty() {
        let count = batch.len();
        g.put_objects_batch(batch)
            .map_err(|e| AppError::internal(e.to_string()))?;
        imported += count as u64;
    }

    ok(json!({ "imported": imported, "collection": collection }))
}

// ─── Search ─────────────────────────────────────────────────────

pub async fn search(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let query = body["query"].as_str().unwrap_or("");
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
    let g = e.lock();
    ok(g.search(query, opts)
        .map_err(|e| AppError::internal(e.to_string()))?)
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
        .filter_map(|v| v.as_f64().map(|f| f as f32))
        .collect();

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
        .map_err(|e| AppError::internal(e.to_string()))?)
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::config::Config;
    use crate::engine::EnginePool;

    fn test_state_and_config() -> (Arc<AppState>, Config) {
        let config = Config::default();
        let state = Arc::new(AppState {
            pool: Arc::new(EnginePool::new(":memory:".to_string())),
            tenant_config: config.tenant.clone(),
            mcp_config: config.mcp.clone(),
            auth_token: config.auth.token.clone(),
            allow_unauthenticated: config.auth.allow_unauthenticated,
            cluster_config: config.cluster.clone(),
            nlq_config: config.nlq.clone(),
        });
        (state, config)
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
