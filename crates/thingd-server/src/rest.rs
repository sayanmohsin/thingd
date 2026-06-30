use axum::{
    Json,
    extract::{Path, Query, State},
    http::header,
    response::IntoResponse,
};
use serde_json::{Value, json};
use std::sync::Arc;
use thingd::*;

use crate::error::AppError;
use crate::server::AppState;

fn ok<T: serde::Serialize>(data: T) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({ "data": data })))
}

// ─── Health ─────────────────────────────────────────────────────

pub async fn health() -> Json<Value> {
    Json(json!({ "data": { "status": "ok" } }))
}

pub async fn metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let e = state.pool.get("");
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

    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body)
}

// ─── Counts ─────────────────────────────────────────────────────

pub async fn count_objects(State(state): State<Arc<AppState>>) -> Result<Json<Value>, AppError> {
    let e = state.pool.get("");
    let g = e.lock();
    ok(json!({ "count": g.count_objects().map_err(|e| AppError::internal(e.to_string()))? }))
}

pub async fn count_events(State(state): State<Arc<AppState>>) -> Result<Json<Value>, AppError> {
    let e = state.pool.get("");
    let g = e.lock();
    ok(json!({ "count": g.count_events().map_err(|e| AppError::internal(e.to_string()))? }))
}

pub async fn count_links(State(state): State<Arc<AppState>>) -> Result<Json<Value>, AppError> {
    let e = state.pool.get("");
    let g = e.lock();
    ok(json!({ "count": g.count_links().map_err(|e| AppError::internal(e.to_string()))? }))
}

// ─── Listings ───────────────────────────────────────────────────

pub async fn list_collections(State(state): State<Arc<AppState>>) -> Result<Json<Value>, AppError> {
    let e = state.pool.get("");
    let g = e.lock();
    ok(g.list_collections()
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn list_streams(State(state): State<Arc<AppState>>) -> Result<Json<Value>, AppError> {
    let e = state.pool.get("");
    let g = e.lock();
    ok(g.list_streams()
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn list_queues(State(state): State<Arc<AppState>>) -> Result<Json<Value>, AppError> {
    let e = state.pool.get("");
    let g = e.lock();
    ok(g.list_queues()
        .map_err(|e| AppError::internal(e.to_string()))?)
}

// ─── Objects ────────────────────────────────────────────────────

pub async fn list_objects(
    State(state): State<Arc<AppState>>,
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

    let e = state.pool.get("");
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
    let obj = MemoryObject::new(collection.clone(), id.clone(), body.to_string());
    let e = state.pool.get("");
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
    Path((collection, id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let e = state.pool.get("");
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
    Path((collection, id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let e = state.pool.get("");
    let mut g = e.lock();
    ok(
        json!({ "deleted": g.delete_object(&collection, &id).map_err(|e| AppError::internal(e.to_string()))? }),
    )
}

pub async fn put_batch(
    State(state): State<Arc<AppState>>,
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
    let e = state.pool.get("");
    let mut g = e.lock();
    ok(g.put_objects_batch(objects)
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn delete_batch(
    State(state): State<Arc<AppState>>,
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
    let e = state.pool.get("");
    let mut g = e.lock();
    ok(
        json!({ "deleted": g.delete_objects_batch(&keys).map_err(|e| AppError::internal(e.to_string()))? }),
    )
}

// ─── Events ─────────────────────────────────────────────────────

pub async fn append_event(
    State(state): State<Arc<AppState>>,
    Path(stream): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let et = body["type"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing 'type'"))?;
    let event = MemoryEvent::new(stream, et, body.to_string());
    let e = state.pool.get("");
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
    Query(params): Query<Value>,
) -> Result<Json<Value>, AppError> {
    let stream = params.get("stream").and_then(|v| v.as_str());
    let opts = ListEventsOptions {
        from_sequence: params.get("fromSequence").and_then(|v| v.as_u64()),
        limit: params.get("limit").and_then(|v| v.as_u64()),
    };
    let e = state.pool.get("");
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
    let e = state.pool.get("");
    let mut g = e.lock();
    ok(g.push_job(job)
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn claim_job(
    State(state): State<Arc<AppState>>,
    Path(queue): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let opts = QueueClaimOptions {
        lease_ms: body
            .get("leaseMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30000),
    };
    let e = state.pool.get("");
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
    Path(queue): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let job_id = body["jobId"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("Missing jobId"))?;
    let e = state.pool.get("");
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
    let e = state.pool.get("");
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
    Path(queue): Path<String>,
) -> Result<Json<Value>, AppError> {
    let e = state.pool.get("");
    let g = e.lock();
    ok(g.list_jobs(&queue)
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn list_dead_jobs(
    State(state): State<Arc<AppState>>,
    Path(queue): Path<String>,
) -> Result<Json<Value>, AppError> {
    let e = state.pool.get("");
    let g = e.lock();
    ok(g.list_dead_jobs(&queue)
        .map_err(|e| AppError::internal(e.to_string()))?)
}

// ─── Links ──────────────────────────────────────────────────────

pub async fn create_link(
    State(state): State<Arc<AppState>>,
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
    let e = state.pool.get("");
    let mut g = e.lock();
    ok(g.create_link(link)
        .map_err(|e| AppError::internal(e.to_string()))?)
}

pub async fn get_links(
    State(state): State<Arc<AppState>>,
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
        let e = state.pool.get("");
        let g = e.lock();
        return ok(g
            .get_neighbors(reference, dir, opts)
            .map_err(|e| AppError::internal(e.to_string()))?);
    }
    Err(AppError::bad_request("Missing 'reference' or 'id'"))
}

pub async fn get_link_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let e = state.pool.get("");
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
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let e = state.pool.get("");
    let mut g = e.lock();
    ok(json!({ "deleted": g.delete_link(&id).map_err(|e| AppError::internal(e.to_string()))? }))
}

// ─── Search ─────────────────────────────────────────────────────

pub async fn search(
    State(state): State<Arc<AppState>>,
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
    let e = state.pool.get("");
    let g = e.lock();
    ok(g.search(query, opts)
        .map_err(|e| AppError::internal(e.to_string()))?)
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
            pool: EnginePool::new(":memory:".to_string()),
            mcp_config: config.mcp.clone(),
            auth_token: config.auth.token.clone(),
            allow_unauthenticated: config.auth.allow_unauthenticated,
            cluster_config: config.cluster.clone(),
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
}
