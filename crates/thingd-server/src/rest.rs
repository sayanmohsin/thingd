use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde_json::{Value, json};
use std::sync::Arc;
use thingd::*;
use tokio::sync::Mutex;

use crate::engine::EnginePool;
use crate::error::AppError;

fn engine(pool: &EnginePool) -> std::sync::Arc<Mutex<Box<dyn ThingStore + Send>>> {
    pool.get("")
}

fn ok<T: serde::Serialize>(data: T) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({ "data": data })))
}

// ─── Health ─────────────────────────────────────────────────────

pub async fn health() -> Json<Value> {
    Json(json!({ "data": { "status": "ok" } }))
}

// ─── Counts ─────────────────────────────────────────────────────

#[axum::debug_handler]
pub async fn count_objects(State(pool): State<Arc<EnginePool>>) -> Result<Json<Value>, AppError> {
    let e = engine(&pool);
    let g = e.lock().await;
    ok(json!({ "count": g.count_objects().map_err(|e| AppError::internal(e.to_string()))? }))
}

pub async fn count_events(State(pool): State<Arc<EnginePool>>) -> Result<Json<Value>, AppError> {
    let e = engine(&pool);
    let g = e.lock().await;
    ok(json!({ "count": g.count_events().map_err(|e| AppError::internal(e.to_string()))? }))
}

pub async fn count_links(State(pool): State<Arc<EnginePool>>) -> Result<Json<Value>, AppError> {
    let e = engine(&pool);
    let g = e.lock().await;
    ok(json!({ "count": g.count_links().map_err(|e| AppError::internal(e.to_string()))? }))
}

// ─── Listings ───────────────────────────────────────────────────

pub async fn list_collections(
    State(pool): State<Arc<EnginePool>>,
) -> Result<Json<Value>, AppError> {
    let e = engine(&pool);
    let g = e.lock().await;
    ok(g.list_collections()
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn list_streams(State(pool): State<Arc<EnginePool>>) -> Result<Json<Value>, AppError> {
    let e = engine(&pool);
    let g = e.lock().await;
    ok(g.list_streams()
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn list_queues(State(pool): State<Arc<EnginePool>>) -> Result<Json<Value>, AppError> {
    let e = engine(&pool);
    let g = e.lock().await;
    ok(g.list_queues()
        .map_err(|e| AppError::internal(e.to_string()))?)
}

// ─── Objects ────────────────────────────────────────────────────

pub async fn list_objects(
    State(pool): State<Arc<EnginePool>>,
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

    let e = engine(&pool);
    let g = e.lock().await;
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
    State(pool): State<Arc<EnginePool>>,
    Path((collection, id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let obj = MemoryObject::new(collection, id, body.to_string());
    let e = engine(&pool);
    let mut g = e.lock().await;
    let r = g
        .put_object(obj)
        .map_err(|e| AppError::internal(e.to_string()))?;
    ok(
        json!({ "id": r.key.id, "collection": r.key.collection, "version": r.version, "createdAt": r.created_at, "updatedAt": r.updated_at }),
    )
}

pub async fn get_object(
    State(pool): State<Arc<EnginePool>>,
    Path((collection, id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let e = engine(&pool);
    let g = e.lock().await;
    match g
        .get_object(&collection, &id)
        .map_err(|e| AppError::internal(e.to_string()))?
    {
        Some(obj) => ok(
            json!({ "id": obj.key.id, "collection": obj.key.collection, "version": obj.version, "createdAt": obj.created_at, "updatedAt": obj.updated_at }),
        ),
        None => Err(AppError::not_found("Object not found")),
    }
}

pub async fn delete_object(
    State(pool): State<Arc<EnginePool>>,
    Path((collection, id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let e = engine(&pool);
    let mut g = e.lock().await;
    ok(
        json!({ "deleted": g.delete_object(&collection, &id).map_err(|e| AppError::internal(e.to_string()))? }),
    )
}

pub async fn put_batch(
    State(pool): State<Arc<EnginePool>>,
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
    let e = engine(&pool);
    let mut g = e.lock().await;
    ok(g.put_objects_batch(objects)
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn delete_batch(
    State(pool): State<Arc<EnginePool>>,
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
    let e = engine(&pool);
    let mut g = e.lock().await;
    ok(
        json!({ "deleted": g.delete_objects_batch(&keys).map_err(|e| AppError::internal(e.to_string()))? }),
    )
}

// ─── Events ─────────────────────────────────────────────────────

pub async fn append_event(
    State(pool): State<Arc<EnginePool>>,
    Path(stream): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let et = body["type"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing 'type'"))?;
    let event = MemoryEvent::new(stream, et, body.to_string());
    let e = engine(&pool);
    let mut g = e.lock().await;
    let r = g
        .append_event(event)
        .map_err(|e| AppError::internal(e.to_string()))?;
    ok(
        json!({ "id": r.sequence.to_string(), "stream": r.stream, "type": r.event_type, "sequence": r.sequence, "createdAt": r.created_at }),
    )
}

pub async fn list_events(
    State(pool): State<Arc<EnginePool>>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, AppError> {
    let stream = params.get("stream").and_then(|v| v.as_str());
    let opts = ListEventsOptions {
        from_sequence: params.get("fromSequence").and_then(|v| v.as_u64()),
        limit: params.get("limit").and_then(|v| v.as_u64()),
    };
    let e = engine(&pool);
    let g = e.lock().await;
    let events = g
        .list_events(stream, opts)
        .map_err(|e| AppError::internal(e.to_string()))?;
    let items: Vec<Value> = events.iter().map(|r| json!({ "id": r.sequence.to_string(), "stream": r.stream, "type": r.event_type, "sequence": r.sequence, "createdAt": r.created_at })).collect();
    ok(items)
}

// ─── Queues ─────────────────────────────────────────────────────

pub async fn push_job(
    State(pool): State<Arc<EnginePool>>,
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
    let e = engine(&pool);
    let mut g = e.lock().await;
    ok(g.push_job(job)
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn claim_job(
    State(pool): State<Arc<EnginePool>>,
    Path(queue): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let opts = QueueClaimOptions {
        lease_ms: body
            .get("leaseMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30000),
    };
    let e = engine(&pool);
    let mut g = e.lock().await;
    match g
        .claim_job_with_options(&queue, opts)
        .map_err(|e| AppError::internal(e.to_string()))?
    {
        Some(job) => ok(job),
        None => Ok(Json(json!({ "data": null }))),
    }
}

pub async fn ack_job(
    State(pool): State<Arc<EnginePool>>,
    Path(queue): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let job_id = body["jobId"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing jobId"))?;
    let e = engine(&pool);
    let mut g = e.lock().await;
    match g
        .ack_job(&queue, job_id)
        .map_err(|e| AppError::internal(e.to_string()))?
    {
        Some(job) => ok(job),
        None => Err(AppError::bad_request("Ack failed")),
    }
}

pub async fn nack_job(
    State(pool): State<Arc<EnginePool>>,
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
    let e = engine(&pool);
    let mut g = e.lock().await;
    match g
        .nack_job_with_options(&queue, job_id, opts)
        .map_err(|e| AppError::internal(e.to_string()))?
    {
        Some(job) => ok(job),
        None => Err(AppError::bad_request("Nack failed")),
    }
}

pub async fn list_jobs(
    State(pool): State<Arc<EnginePool>>,
    Path(queue): Path<String>,
) -> Result<Json<Value>, AppError> {
    let e = engine(&pool);
    let g = e.lock().await;
    ok(g.list_jobs(&queue)
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn list_dead_jobs(
    State(pool): State<Arc<EnginePool>>,
    Path(queue): Path<String>,
) -> Result<Json<Value>, AppError> {
    let e = engine(&pool);
    let g = e.lock().await;
    ok(g.list_dead_jobs(&queue)
        .map_err(|e| AppError::internal(e.to_string()))?)
}

// ─── Links ──────────────────────────────────────────────────────

pub async fn create_link(
    State(pool): State<Arc<EnginePool>>,
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
    let e = engine(&pool);
    let mut g = e.lock().await;
    ok(g.create_link(link)
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn get_links(
    State(pool): State<Arc<EnginePool>>,
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
        let e = engine(&pool);
        let g = e.lock().await;
        return ok(g
            .get_neighbors(reference, dir, opts)
            .map_err(|e| AppError::internal(e.to_string()))?);
    }
    Err(AppError::bad_request("Missing 'reference' or 'id'"))
}

pub async fn get_link_by_id(
    State(pool): State<Arc<EnginePool>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let e = engine(&pool);
    let g = e.lock().await;
    match g
        .get_link(&id)
        .map_err(|e| AppError::internal(e.to_string()))?
    {
        Some(l) => ok(l),
        None => Err(AppError::not_found("Link not found")),
    }
}

pub async fn delete_link(
    State(pool): State<Arc<EnginePool>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let e = engine(&pool);
    let mut g = e.lock().await;
    ok(json!({ "deleted": g.delete_link(&id).map_err(|e| AppError::internal(e.to_string()))? }))
}

// ─── Search ─────────────────────────────────────────────────────

pub async fn search(
    State(pool): State<Arc<EnginePool>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let query = body["query"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing 'query'"))?;
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
    let e = engine(&pool);
    let g = e.lock().await;
    ok(g.search(query, opts)
        .map_err(|e| AppError::internal(e.to_string()))?)
}
