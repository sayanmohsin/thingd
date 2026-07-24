use crate::auth::extract_tenant_id;
use crate::error::{self, AppError};
use crate::server::AppState;
use axum::{Json, extract::State, http::HeaderMap};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::LazyLock;

fn mcp_success(id: Option<&Value>, content: Value) -> Json<Value> {
    Json(json!({ "jsonrpc": "2.0", "result": content, "id": id }))
}

fn mcp_error(id: Option<&Value>, code: i32, message: &str) -> Json<Value> {
    Json(json!({ "jsonrpc": "2.0", "error": { "code": code, "message": message }, "id": id }))
}

fn mcp_error_result(id: Option<&Value>, text: &str) -> Json<Value> {
    Json(
        json!({ "jsonrpc": "2.0", "result": { "content": [{ "type": "text", "text": text }], "isError": true }, "id": id }),
    )
}

fn sanitize_detail(e: &AppError) -> String {
    if error::is_production_mode() && e.status.as_u16() == 500 {
        "Internal server error".to_string()
    } else {
        e.detail.clone()
    }
}

const MAX_STRING_LEN: usize = 256;
const MAX_QUERY_LEN: usize = 4096;

fn arg_str(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| {
            let cap = if key == "query" || key == "text" || key == "payload" {
                MAX_QUERY_LEN
            } else {
                MAX_STRING_LEN
            };
            let mut truncated = s.chars().take(cap).collect::<String>();
            truncated.shrink_to_fit();
            truncated
        })
        .unwrap_or_default()
}

fn arg_u64(args: &Value, key: &str) -> Option<u64> {
    args.get(key)
        .and_then(|v| v.as_u64())
        .map(|v| v.min(10_000))
}

fn arg_usize(args: &Value, key: &str) -> Option<usize> {
    args.get(key)
        .and_then(|v| v.as_u64())
        .map(|v| v.min(10_000) as usize)
}

fn arg_f64(args: &Value, key: &str) -> Option<f64> {
    args.get(key).and_then(|v| v.as_f64())
}

fn tool_def(entry: &ToolEntry) -> Value {
    json!({
        "name": entry.name,
        "description": entry.description,
        "inputSchema": {
            "type": "object",
            "properties": entry.properties,
            "required": entry.required,
        },
        "annotations": {
            "readOnlyHint": !entry.is_write,
            "destructiveHint": entry.destructive,
            "idempotentHint": !entry.is_write || entry.destructive,
            "openWorldHint": false,
        }
    })
}

fn str_prop(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn int_prop(description: &str) -> Value {
    json!({ "type": "integer", "description": description })
}

fn obj_prop(description: &str) -> Value {
    json!({ "type": "object", "description": description })
}

fn arr_prop(description: &str) -> Value {
    json!({ "type": "array", "description": description })
}

fn num_prop(description: &str) -> Value {
    json!({ "type": "number", "description": description })
}

fn emit_audit_event(
    state: &AppState,
    g: &mut Box<dyn thingd::ThingStore + Send>,
    tool_name: &str,
    args: &Value,
    result: &str,
) {
    if !state.mcp_config.audit {
        return;
    }
    let actor = args
        .get("actor")
        .and_then(|v| v.as_str())
        .unwrap_or(&state.mcp_config.audit_actor);
    let source = args
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or(&state.mcp_config.audit_source);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let body = json!({
        "tool": tool_name,
        "actor": actor,
        "source": source,
        "timestamp": now,
        "result": result,
    });
    if let Err(e) = g.append_event(thingd::MemoryEvent::new(
        &state.mcp_config.audit_stream,
        "audit",
        body.to_string(),
    )) {
        tracing::warn!("Failed to emit audit event: {e}");
    }
}

// ─── Object tools ────────────────────────────────────────────────

fn handle_thing_search(
    state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    // Allowlist check for thing_search's collections array
    if !state.mcp_config.collection_allowlist.is_empty()
        && let Some(collections) = args.get("collections").and_then(|v| v.as_array())
    {
        for c in collections {
            if let Some(name) = c.as_str()
                && !state
                    .mcp_config
                    .collection_allowlist
                    .contains(&name.to_string())
            {
                return Err(AppError::bad_request(format!(
                    "Collection '{name}' is not in the allowlist"
                )));
            }
        }
    }
    let e = state.pool.get_reader(db_path);
    let g = e.lock();
    let query = arg_str(args, "query");
    let limit = arg_usize(args, "limit").map(|v| v.min(100));
    let opts = thingd::SearchOptions {
        collections: args.get("collections").and_then(|v| v.as_array()).map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        }),
        limit,
        filter: args.get("filter").cloned(),
    };
    let hits = g.search(&query, opts)?;
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&hits).unwrap_or_default() }] }),
    )
}

fn handle_thing_get(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let collection = arg_str(args, "collection");
    let id = arg_str(args, "id");
    match g.get_object(&collection, &id)? {
        Some(obj) => {
            let body_val: Value = serde_json::from_str(&obj.body).unwrap_or(Value::Null);
            Ok(
                json!({ "content": [{ "type": "text", "text": serde_json::to_string(&body_val).unwrap_or_default() }] }),
            )
        },
        None => Ok(
            json!({ "content": [{ "type": "text", "text": "Object not found" }], "isError": true }),
        ),
    }
}

fn handle_thing_put(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_writer(db_path);
    let mut g = e.lock();
    let collection = arg_str(args, "collection");
    let obj = args.get("object").cloned().unwrap_or(json!({}));
    let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("new");
    let mut memory_obj = thingd::MemoryObject::new(collection, id.to_string(), obj.to_string());
    if let Some(vector) = obj.get("vector").and_then(|v| v.as_array()) {
        let vec: Vec<f32> = vector
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();
        if !vec.is_empty() {
            memory_obj = memory_obj.with_vector(vec);
        }
    }
    let expected_version = args.get("expectedVersion").and_then(|v| v.as_u64());
    let r = if let Some(version) = expected_version {
        let opts = thingd::PutObjectOptions {
            expected_version: Some(version),
            ..Default::default()
        };
        g.put_object_with_options(memory_obj, opts)?
    } else {
        g.put_object(memory_obj)?
    };
    emit_audit_event(_state, &mut g, _tool_name, args, "success");
    Ok(
        json!({ "content": [{ "type": "text", "text": format!("Created/updated: {}/{}", r.key.collection, r.key.id) }] }),
    )
}

fn handle_thing_delete(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let mut g = e.lock();
    let collection = arg_str(args, "collection");
    let id = arg_str(args, "id");
    let deleted = g.delete_object(&collection, &id)?;
    emit_audit_event(_state, &mut g, _tool_name, args, "success");
    Ok(json!({ "content": [{ "type": "text", "text": format!("Deleted: {deleted}") }] }))
}

fn handle_thing_objects_list(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let collection = arg_str(args, "collection");
    let opts = thingd::ListObjectsOptions {
        filter: args
            .get("filter")
            .and_then(|v| v.as_object())
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default(),
        sort_by: args.get("sortBy").map(|sb| thingd::SortBy {
            field: sb
                .get("field")
                .and_then(|v| v.as_str())
                .unwrap_or("created_at")
                .to_string(),
            direction: match sb.get("direction").and_then(|v| v.as_str()) {
                Some("desc") => thingd::SortDirection::Desc,
                _ => thingd::SortDirection::Asc,
            },
        }),
        limit: arg_u64(args, "limit"),
        offset: arg_u64(args, "offset"),
    };
    let objects = g.list_objects(Some(&[collection]), &opts)?;
    let items: Vec<Value> = objects
        .iter()
        .map(|obj| json!({ "id": obj.key.id, "collection": obj.key.collection }))
        .collect();
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&items).unwrap_or_default() }] }),
    )
}

fn handle_thing_objects_put_batch(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let mut g = e.lock();
    let collection = arg_str(args, "collection");
    let objects = args
        .get("objects")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if objects.is_empty() || objects.len() > 1000 {
        return Err(AppError::bad_request(
            "objects must contain between 1 and 1000 items",
        ));
    }
    let memory_objects: Vec<thingd::MemoryObject> = objects
        .iter()
        .map(|obj| {
            let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("new");
            thingd::MemoryObject::new(collection.clone(), id.to_string(), obj.to_string())
        })
        .collect();
    let results = g.put_objects_batch(memory_objects)?;
    emit_audit_event(_state, &mut g, _tool_name, args, "success");
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&results).unwrap_or_default() }] }),
    )
}

fn handle_thing_objects_delete_batch(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let mut g = e.lock();
    let collection = arg_str(args, "collection");
    let ids = args
        .get("ids")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if ids.is_empty() || ids.len() > 1000 {
        return Err(AppError::bad_request(
            "ids must contain between 1 and 1000 items",
        ));
    }
    let keys: Vec<(String, String)> = ids
        .iter()
        .filter_map(|v| v.as_str().map(|s| (collection.clone(), s.to_string())))
        .collect();
    let deleted = g.delete_objects_batch(&keys)?;
    emit_audit_event(_state, &mut g, _tool_name, args, "success");
    Ok(json!({ "content": [{ "type": "text", "text": format!("{deleted}") }] }))
}

fn handle_thing_objects_get_batch(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let collection = arg_str(args, "collection");
    let ids = args
        .get("ids")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if ids.is_empty() || ids.len() > 1000 {
        return Err(AppError::bad_request(
            "ids must contain between 1 and 1000 items",
        ));
    }
    let id_strings: Vec<String> = ids
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    let results = g.get_objects_batch(&collection, &id_strings)?;
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&results).unwrap_or_default() }] }),
    )
}

// ─── Event tools ─────────────────────────────────────────────────

fn handle_thing_events_append(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let mut g = e.lock();
    let stream = arg_str(args, "stream");

    // Prevent direct writes to the protected audit stream
    if g.is_protected_stream(&stream) {
        return Err(AppError::bad_request(format!(
            "Stream '{stream}' is protected and cannot be written to directly"
        )));
    }

    let event = args.get("event").cloned().unwrap_or(json!({}));
    let event_type = event
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("event");
    let idempotency_key = event
        .get("idempotencyKey")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let mut memory_event = thingd::MemoryEvent::new(stream, event_type, event.to_string());
    if !idempotency_key.is_empty() {
        memory_event.idempotency_key = idempotency_key.to_string();
    }
    let r = g.append_event(memory_event)?;
    emit_audit_event(_state, &mut g, _tool_name, args, "success");
    Ok(
        json!({ "content": [{ "type": "text", "text": format!("Event appended: {} seq={}", r.event_type, r.sequence) }] }),
    )
}

fn handle_thing_events_list(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let stream = arg_str(args, "stream");
    let stream_opt = if stream.is_empty() {
        None
    } else {
        Some(stream.as_str())
    };
    let opts = thingd::ListEventsOptions {
        from_sequence: arg_u64(args, "fromSequence"),
        limit: arg_u64(args, "limit"),
        since: args.get("since").and_then(|v| v.as_str()).map(String::from),
    };
    let events = g.list_events(stream_opt, opts)?;
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&events).unwrap_or_default() }] }),
    )
}

// ─── Queue tools ─────────────────────────────────────────────────

fn handle_thing_queue_push(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let mut g = e.lock();
    let queue = arg_str(args, "queue");
    let payload = args.get("payload").cloned().unwrap_or(json!({}));
    let max_attempts = args
        .get("maxAttempts")
        .and_then(|v| v.as_u64())
        .map(|v| v.min(100))
        .unwrap_or(3) as u32;
    let job_id = arg_str(args, "idempotencyKey");
    let job_id = if job_id.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        job_id
    };
    let mut job = thingd::QueueJob::new(&queue, job_id, payload.to_string(), max_attempts);
    if let Some(delay) = args.get("delayMs").and_then(|v| v.as_u64())
        && delay > 0
    {
        job = job.delay_by_ms(delay);
    }
    if let Some(priority) = args.get("priority").and_then(|v| v.as_i64()) {
        job = job.with_priority(i32::try_from(priority).unwrap_or(i32::MAX));
    }
    let result = g.push_job(job)?;
    emit_audit_event(_state, &mut g, _tool_name, args, "success");
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&result).unwrap_or_default() }] }),
    )
}

fn handle_thing_queue_claim(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let mut g = e.lock();
    let queue = arg_str(args, "queue");
    let opts = thingd::QueueClaimOptions {
        lease_ms: args
            .get("leaseMs")
            .and_then(|v| v.as_u64())
            .map(|v| v.min(86_400_000))
            .unwrap_or(30000),
    };
    match g.claim_job_with_options(&queue, opts)? {
        Some(job) => {
            emit_audit_event(_state, &mut g, _tool_name, args, "success");
            Ok(
                json!({ "content": [{ "type": "text", "text": serde_json::to_string(&job).unwrap_or_default() }] }),
            )
        },
        None => Ok(json!({ "content": [{ "type": "text", "text": "No job available" }] })),
    }
}

fn handle_thing_queue_ack(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let mut g = e.lock();
    let queue = arg_str(args, "queue");
    let id = arg_str(args, "id");
    match g.ack_job(&queue, &id)? {
        Some(job) => {
            emit_audit_event(_state, &mut g, _tool_name, args, "success");
            Ok(
                json!({ "content": [{ "type": "text", "text": serde_json::to_string(&job).unwrap_or_default() }] }),
            )
        },
        None => {
            Ok(json!({ "content": [{ "type": "text", "text": "Ack failed" }], "isError": true }))
        },
    }
}

fn handle_thing_queue_nack(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let mut g = e.lock();
    let queue = arg_str(args, "queue");
    let id = arg_str(args, "id");
    let opts = thingd::QueueNackOptions {
        delay_ms: args.get("delayMs").and_then(|v| v.as_u64()).unwrap_or(0),
        error: arg_str(args, "error"),
    };
    match g.nack_job_with_options(&queue, &id, opts)? {
        Some(job) => {
            emit_audit_event(_state, &mut g, _tool_name, args, "success");
            Ok(
                json!({ "content": [{ "type": "text", "text": serde_json::to_string(&job).unwrap_or_default() }] }),
            )
        },
        None => {
            Ok(json!({ "content": [{ "type": "text", "text": "Nack failed" }], "isError": true }))
        },
    }
}

fn handle_thing_queue_list(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let queue = arg_str(args, "queue");
    let jobs = g.list_jobs(&queue)?;
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&jobs).unwrap_or_default() }] }),
    )
}

fn handle_thing_queue_dead(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let queue = arg_str(args, "queue");
    let jobs = g.list_dead_jobs(&queue)?;
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&jobs).unwrap_or_default() }] }),
    )
}

// ─── Count tools ─────────────────────────────────────────────────

fn handle_thing_count_objects(
    _state: &AppState,
    _tool_name: &str,
    _args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let count = g.count_objects()?;
    Ok(json!({ "content": [{ "type": "text", "text": count.to_string() }] }))
}

fn handle_thing_count_objects_in_collection(
    state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let collection = args["collection"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("collection is required"))?;
    let e = state.pool.get_reader(db_path);
    let g = e.lock();
    let count = g.count_objects_in_collection(collection)?;
    Ok(json!({ "content": [{ "type": "text", "text": count.to_string() }] }))
}

fn handle_thing_count_events(
    _state: &AppState,
    _tool_name: &str,
    _args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let count = g.count_events()?;
    Ok(json!({ "content": [{ "type": "text", "text": count.to_string() }] }))
}

fn handle_thing_count_active_jobs(
    _state: &AppState,
    _tool_name: &str,
    _args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let queues = g.list_queues()?;
    let mut count: u64 = 0;
    for q in &queues {
        let jobs = g.list_jobs(q)?;
        count += jobs.len() as u64;
    }
    Ok(json!({ "content": [{ "type": "text", "text": count.to_string() }] }))
}

fn handle_thing_count_dead_jobs(
    _state: &AppState,
    _tool_name: &str,
    _args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let queues = g.list_queues()?;
    let mut count: u64 = 0;
    for q in &queues {
        let jobs = g.list_dead_jobs(q)?;
        count += jobs.len() as u64;
    }
    Ok(json!({ "content": [{ "type": "text", "text": count.to_string() }] }))
}

// ─── Discovery tools ─────────────────────────────────────────────

fn handle_thing_list_collections(
    _state: &AppState,
    _tool_name: &str,
    _args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let collections = g.list_collections()?;
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&collections).unwrap_or_default() }] }),
    )
}

fn handle_thing_list_streams(
    _state: &AppState,
    _tool_name: &str,
    _args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let streams = g.list_streams()?;
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&streams).unwrap_or_default() }] }),
    )
}

fn handle_thing_list_queues(
    _state: &AppState,
    _tool_name: &str,
    _args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let queues = g.list_queues()?;
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&queues).unwrap_or_default() }] }),
    )
}

fn handle_thing_create_index(
    state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let collection = args["collection"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("collection is required"))?;
    let field = args["field"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("field is required"))?;
    let e = state.pool.get_writer(db_path);
    let mut g = e.lock();
    g.create_index(collection, field)?;
    Ok(json!({ "content": [{ "type": "text", "text": "{\"created\":true}" }] }))
}

fn handle_thing_list_indexes(
    state: &AppState,
    _tool_name: &str,
    _args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = state.pool.get_writer(db_path);
    let g = e.lock();
    let indexes = g.list_indexes()?;
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&indexes).unwrap_or_default() }] }),
    )
}

// ─── Link tools ──────────────────────────────────────────────────

fn handle_thing_link_create(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let mut g = e.lock();
    let from_ref = arg_str(args, "fromRef");
    let link_type = arg_str(args, "linkType");
    let to_ref = arg_str(args, "toRef");
    let mut link = thingd::Link::new(&from_ref, &link_type, &to_ref);
    if let Some(w) = arg_f64(args, "weight") {
        link.weight = Some(w);
    }
    link.metadata_json = arg_str(args, "metadataJson");
    let r = g.create_link(link)?;
    emit_audit_event(_state, &mut g, _tool_name, args, "success");
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&r).unwrap_or_default() }] }),
    )
}

fn handle_thing_link_get(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let id = arg_str(args, "id");
    match g.get_link(&id)? {
        Some(l) => Ok(
            json!({ "content": [{ "type": "text", "text": serde_json::to_string(&l).unwrap_or_default() }] }),
        ),
        None => Ok(
            json!({ "content": [{ "type": "text", "text": "Link not found" }], "isError": true }),
        ),
    }
}

fn handle_thing_link_delete(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let mut g = e.lock();
    let id = arg_str(args, "id");
    let deleted = g.delete_link(&id)?;
    emit_audit_event(_state, &mut g, _tool_name, args, "success");
    Ok(json!({ "content": [{ "type": "text", "text": format!("Deleted: {deleted}") }] }))
}

fn handle_thing_link_neighbors(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let reference = arg_str(args, "reference");
    let dir_str = args
        .get("direction")
        .and_then(|v| v.as_str())
        .unwrap_or("Both");
    let direction = match dir_str {
        "Outgoing" => thingd::LinkDirection::Outgoing,
        "Incoming" => thingd::LinkDirection::Incoming,
        _ => thingd::LinkDirection::Both,
    };
    let opts = thingd::LinkQueryOptions {
        link_type: args
            .get("linkType")
            .and_then(|v| v.as_str())
            .map(String::from),
        limit: arg_usize(args, "limit"),
    };
    let links = g.get_neighbors(&reference, direction, opts)?;
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&links).unwrap_or_default() }] }),
    )
}

fn handle_thing_link_count(
    _state: &AppState,
    _tool_name: &str,
    _args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let count = g.count_links()?;
    Ok(json!({ "content": [{ "type": "text", "text": count.to_string() }] }))
}

fn handle_thing_schema(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let collection = args.get("collection").and_then(|v| v.as_str());
    let schemas = g
        .schema(collection, &thingd::SchemaOptions::default())
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&schemas).unwrap_or_default() }] }),
    )
}

fn handle_thing_nlq(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    _db_path: &str,
) -> Result<Value, AppError> {
    if !_state.nlq_config.enabled {
        return Ok(json!({
            "content": [{ "type": "text", "text": "NLQ is not enabled. Set --nlq-model to enable." }],
            "isError": true
        }));
    }

    let question = args.get("question").and_then(|v| v.as_str()).unwrap_or("");

    if question.is_empty() {
        return Ok(json!({
            "content": [{ "type": "text", "text": "Missing 'question' parameter" }],
            "isError": true
        }));
    }

    let collection = args.get("collection").and_then(|v| v.as_str());

    let result = tokio::runtime::Handle::current().block_on(crate::nlq::execute_nlq(
        &_state.pool,
        &_state.nlq_config,
        question,
        collection,
    ));

    match result {
        Ok(nlq_result) => Ok(json!({
            "content": [{ "type": "text", "text": serde_json::to_string(&nlq_result).unwrap_or_default() }]
        })),
        Err(e) => Ok(json!({
            "content": [{ "type": "text", "text": e }],
            "isError": true
        })),
    }
}

fn handle_thing_aggregate(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let collection = arg_str(args, "collection");
    let function_str = arg_str(args, "function");
    let function = match function_str.as_str() {
        "sum" => thingd::AggregateFunction::Sum,
        "avg" => thingd::AggregateFunction::Avg,
        "min" => thingd::AggregateFunction::Min,
        "max" => thingd::AggregateFunction::Max,
        _ => thingd::AggregateFunction::Count,
    };
    let field = args.get("field").and_then(|v| v.as_str()).map(String::from);
    let group_by = args
        .get("groupBy")
        .and_then(|v| v.as_str())
        .map(String::from);
    let filter = args
        .get("filter")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let opts = thingd::AggregateOptions {
        function,
        field,
        filter,
        group_by,
    };

    let result = g.aggregate(&collection, &opts)?;
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&result).unwrap_or_default() }] }),
    )
}

fn handle_thing_timeseries(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let collection = arg_str(args, "collection");
    let function_str = arg_str(args, "function");
    let function = match function_str.as_str() {
        "sum" => thingd::AggregateFunction::Sum,
        "avg" => thingd::AggregateFunction::Avg,
        "min" => thingd::AggregateFunction::Min,
        "max" => thingd::AggregateFunction::Max,
        _ => thingd::AggregateFunction::Count,
    };
    let bucket_str = arg_str(args, "bucket");
    let bucket = match bucket_str.as_str() {
        "hour" => thingd::TimeBucket::Hour,
        "week" => thingd::TimeBucket::Week,
        "month" => thingd::TimeBucket::Month,
        _ => thingd::TimeBucket::Day,
    };
    let field = args.get("field").and_then(|v| v.as_str()).map(String::from);
    let filter = args
        .get("filter")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let from = args.get("from").and_then(|v| v.as_str()).map(String::from);
    let to = args.get("to").and_then(|v| v.as_str()).map(String::from);

    let opts = thingd::TimeSeriesOptions {
        function,
        bucket,
        field,
        filter,
        from,
        to,
    };

    let result = g.timeseries(&collection, &opts)?;
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&result).unwrap_or_default() }] }),
    )
}

// ─── Vector Search ─────────────────────────────────────────────

fn handle_thing_vector_search(
    _state: &AppState,
    _tool_name: &str,
    args: &Value,
    db_path: &str,
) -> Result<Value, AppError> {
    let e = _state.pool.get_reader(db_path);
    let g = e.lock();
    let collection = arg_str(args, "collection");

    let query_vector: Vec<f32> = args
        .get("vector")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect()
        })
        .ok_or_else(|| {
            AppError::bad_request("Missing or invalid 'vector' (expected array of numbers)")
        })?;

    let top_k = arg_usize(args, "topK");
    let filter = args.get("filter").cloned();

    let opts = thingd::VectorSearchOptions { top_k, filter };
    let hits = g.vector_search(&collection, &query_vector, opts)?;
    Ok(
        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&hits).unwrap_or_default() }] }),
    )
}

type ToolHandler = fn(&AppState, &str, &Value, &str) -> Result<Value, AppError>;

struct ToolEntry {
    name: &'static str,
    description: &'static str,
    properties: Value,
    required: &'static [&'static str],
    handler: ToolHandler,
    is_write: bool,
    destructive: bool,
    needs_collection: bool,
}

fn all_tool_defs() -> Vec<Value> {
    ALL_TOOLS.iter().map(tool_def).collect()
}

static ALL_TOOLS: LazyLock<Vec<ToolEntry>> = LazyLock::new(|| {
    vec![
        // Object tools (7)
        ToolEntry {
            name: "thing_search",
            description: "Search objects and events using FTS5",
            properties: json!({ "query": str_prop("Search query"), "collections": arr_prop("Optional collection filter"), "limit": int_prop("Max results (max 100, default 10)"), "filter": obj_prop("Metadata filter") }),
            required: &["query"],
            handler: handle_thing_search,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_get",
            description: "Get an object by collection and ID",
            properties: json!({ "collection": str_prop("Collection name"), "id": str_prop("Object ID") }),
            required: &["collection", "id"],
            handler: handle_thing_get,
            is_write: false,
            destructive: false,
            needs_collection: true,
        },
        ToolEntry {
            name: "thing_put",
            description: "Create or update an object",
            properties: json!({ "collection": str_prop("Collection name"), "object": obj_prop("Object data, must include 'id' field"), "expectedVersion": int_prop("Optional expected version for optimistic locking (CAS)"), "actor": str_prop("Who performed the action"), "source": str_prop("Where the action originated") }),
            required: &["collection", "object"],
            handler: handle_thing_put,
            is_write: true,
            destructive: false,
            needs_collection: true,
        },
        ToolEntry {
            name: "thing_delete",
            description: "Delete an object",
            properties: json!({ "collection": str_prop("Collection name"), "id": str_prop("Object ID"), "actor": str_prop("Who performed the action"), "source": str_prop("Where the action originated") }),
            required: &["collection", "id"],
            handler: handle_thing_delete,
            is_write: true,
            destructive: true,
            needs_collection: true,
        },
        ToolEntry {
            name: "thing_objects_list",
            description: "List objects in a collection",
            properties: json!({ "collection": str_prop("Collection name"), "filter": obj_prop("Metadata filter"), "sortBy": obj_prop("Sort options: { field, direction }"), "limit": int_prop("Max results"), "offset": int_prop("Result offset") }),
            required: &["collection"],
            handler: handle_thing_objects_list,
            is_write: false,
            destructive: false,
            needs_collection: true,
        },
        ToolEntry {
            name: "thing_objects_put_batch",
            description: "Batch create or update up to 1000 objects",
            properties: json!({ "collection": str_prop("Collection name"), "objects": arr_prop("Array of objects (1-1000)") }),
            required: &["collection", "objects"],
            handler: handle_thing_objects_put_batch,
            is_write: true,
            destructive: false,
            needs_collection: true,
        },
        ToolEntry {
            name: "thing_objects_delete_batch",
            description: "Batch delete up to 1000 objects by ID",
            properties: json!({ "collection": str_prop("Collection name"), "ids": arr_prop("Array of object IDs (1-1000)") }),
            required: &["collection", "ids"],
            handler: handle_thing_objects_delete_batch,
            is_write: true,
            destructive: true,
            needs_collection: true,
        },
        ToolEntry {
            name: "thing_objects_get_batch",
            description: "Batch read up to 1000 objects by ID",
            properties: json!({ "collection": str_prop("Collection name"), "ids": arr_prop("Array of object IDs (1-1000)") }),
            required: &["collection", "ids"],
            handler: handle_thing_objects_get_batch,
            is_write: false,
            destructive: false,
            needs_collection: true,
        },
        // Event tools (2)
        ToolEntry {
            name: "thing_events_append",
            description: "Append an event to a stream",
            properties: json!({ "stream": str_prop("Stream name"), "event": obj_prop("Event data with 'type' field"), "actor": str_prop("Who performed the action"), "source": str_prop("Where the action originated") }),
            required: &["stream", "event"],
            handler: handle_thing_events_append,
            is_write: true,
            destructive: false,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_events_list",
            description: "List events in a stream",
            properties: json!({ "stream": str_prop("Optional stream name (lists all if omitted)"), "fromSequence": int_prop("Starting sequence"), "limit": int_prop("Max results"), "since": str_prop("ISO 8601 timestamp to filter from (e.g. 2026-07-17T00:00:00.000Z)") }),
            required: &[],
            handler: handle_thing_events_list,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        // Queue tools (6)
        ToolEntry {
            name: "thing_queue_push",
            description: "Push a job to a queue",
            properties: json!({ "queue": str_prop("Queue name"), "payload": obj_prop("Job payload"), "idempotencyKey": str_prop("Idempotency key"), "maxAttempts": int_prop("Max retry attempts (default 3, max 100)"), "delayMs": int_prop("Delay in ms before job is available"), "priority": int_prop("Priority for claim ordering (higher = claimed sooner)"), "actor": str_prop("Who performed the action"), "source": str_prop("Where the action originated") }),
            required: &["queue", "payload"],
            handler: handle_thing_queue_push,
            is_write: true,
            destructive: false,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_queue_claim",
            description: "Claim a job from a queue",
            properties: json!({ "queue": str_prop("Queue name"), "leaseMs": int_prop("Lease duration in ms (default 30000)"), "actor": str_prop("Who performed the action"), "source": str_prop("Where the action originated") }),
            required: &["queue"],
            handler: handle_thing_queue_claim,
            is_write: true,
            destructive: false,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_queue_ack",
            description: "Acknowledge a job as completed",
            properties: json!({ "queue": str_prop("Queue name"), "id": str_prop("Job ID"), "actor": str_prop("Who performed the action"), "source": str_prop("Where the action originated") }),
            required: &["queue", "id"],
            handler: handle_thing_queue_ack,
            is_write: true,
            destructive: false,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_queue_nack",
            description: "Mark a job as failed (retry or dead letter)",
            properties: json!({ "queue": str_prop("Queue name"), "id": str_prop("Job ID"), "delayMs": int_prop("Retry delay in ms"), "error": str_prop("Error description"), "actor": str_prop("Who performed the action"), "source": str_prop("Where the action originated") }),
            required: &["queue", "id"],
            handler: handle_thing_queue_nack,
            is_write: true,
            destructive: false,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_queue_list",
            description: "List active jobs in a queue",
            properties: json!({ "queue": str_prop("Queue name") }),
            required: &["queue"],
            handler: handle_thing_queue_list,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_queue_dead",
            description: "List dead-letter jobs in a queue",
            properties: json!({ "queue": str_prop("Queue name") }),
            required: &["queue"],
            handler: handle_thing_queue_dead,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        // Count tools (5)
        ToolEntry {
            name: "thing_count_objects",
            description: "Count all objects",
            properties: json!({}),
            required: &[],
            handler: handle_thing_count_objects,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_count_objects_in_collection",
            description: "Count objects in a specific collection",
            properties: json!({ "collection": str_prop("Collection name") }),
            required: &["collection"],
            handler: handle_thing_count_objects_in_collection,
            is_write: false,
            destructive: false,
            needs_collection: true,
        },
        ToolEntry {
            name: "thing_count_events",
            description: "Count all events",
            properties: json!({}),
            required: &[],
            handler: handle_thing_count_events,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_count_active_jobs",
            description: "Count all active (non-dead) queue jobs",
            properties: json!({}),
            required: &[],
            handler: handle_thing_count_active_jobs,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_count_dead_jobs",
            description: "Count all dead-letter queue jobs",
            properties: json!({}),
            required: &[],
            handler: handle_thing_count_dead_jobs,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        // Discovery tools (3)
        ToolEntry {
            name: "thing_list_collections",
            description: "List all collection names",
            properties: json!({}),
            required: &[],
            handler: handle_thing_list_collections,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_list_streams",
            description: "List all stream names",
            properties: json!({}),
            required: &[],
            handler: handle_thing_list_streams,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_list_queues",
            description: "List all queue names",
            properties: json!({}),
            required: &[],
            handler: handle_thing_list_queues,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_create_index",
            description: "Create a functional index on a JSON body field for a collection",
            properties: json!({ "collection": str_prop("Collection name"), "field": str_prop("JSON body field name to index") }),
            required: &["collection", "field"],
            handler: handle_thing_create_index,
            is_write: true,
            destructive: false,
            needs_collection: true,
        },
        ToolEntry {
            name: "thing_list_indexes",
            description: "List all custom functional indexes",
            properties: json!({}),
            required: &[],
            handler: handle_thing_list_indexes,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        // Link tools (5)
        ToolEntry {
            name: "thing_link_create",
            description: "Create a directed link between two references",
            properties: json!({ "fromRef": str_prop("Source reference"), "linkType": str_prop("Link type label"), "toRef": str_prop("Target reference"), "weight": num_prop("Optional weight 0.0-1.0"), "metadataJson": str_prop("Optional metadata JSON string") }),
            required: &["fromRef", "linkType", "toRef"],
            handler: handle_thing_link_create,
            is_write: true,
            destructive: false,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_link_get",
            description: "Get a link by ID",
            properties: json!({ "id": str_prop("Link ID") }),
            required: &["id"],
            handler: handle_thing_link_get,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_link_delete",
            description: "Delete a link by ID",
            properties: json!({ "id": str_prop("Link ID") }),
            required: &["id"],
            handler: handle_thing_link_delete,
            is_write: true,
            destructive: true,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_link_neighbors",
            description: "Get neighbor links for a reference",
            properties: json!({ "reference": str_prop("Reference to find neighbors for"), "direction": str_prop("Direction: Outgoing, Incoming, or Both (default)"), "linkType": str_prop("Optional link type filter"), "limit": int_prop("Max results") }),
            required: &["reference"],
            handler: handle_thing_link_neighbors,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        ToolEntry {
            name: "thing_link_count",
            description: "Count all links",
            properties: json!({}),
            required: &[],
            handler: handle_thing_link_count,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        // Schema tool (1)
        ToolEntry {
            name: "thing_schema",
            description: "Reflect the schema of one or all collections by sampling stored objects. Returns inferred field names, types, and sample values.",
            properties: json!({ "collection": str_prop("Optional collection name (omit for all)") }),
            required: &[],
            handler: handle_thing_schema,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        // NLQ tool (1)
        ToolEntry {
            name: "thing_nlq",
            description: "Ask a natural language question about your data. Infers schema, calls an LLM, and returns structured results.",
            properties: json!({ "question": str_prop("Natural language question about your data"), "collection": str_prop("Optional collection name to scope the query") }),
            required: &["question"],
            handler: handle_thing_nlq,
            is_write: false,
            destructive: false,
            needs_collection: false,
        },
        // Aggregate tools (2)
        ToolEntry {
            name: "thing_aggregate",
            description: "Run aggregate queries on objects in a collection",
            properties: json!({ "collection": str_prop("Collection name"), "function": str_prop("Aggregate function: count, sum, avg, min, or max"), "field": str_prop("Field name for sum/avg/min/max"), "groupBy": str_prop("Optional field to group results by"), "filter": obj_prop("Metadata filter"), "actor": str_prop("Who performed the action"), "source": str_prop("Where the action originated") }),
            required: &["collection", "function"],
            handler: handle_thing_aggregate,
            is_write: false,
            destructive: false,
            needs_collection: true,
        },
        ToolEntry {
            name: "thing_timeseries",
            description: "Run time-series aggregation on objects",
            properties: json!({ "collection": str_prop("Collection name"), "function": str_prop("Aggregate function: count, sum, avg, min, or max"), "bucket": str_prop("Time bucket: hour, day, week, or month"), "field": str_prop("Field name for sum/avg/min/max"), "filter": obj_prop("Metadata filter"), "from": str_prop("Start timestamp (ISO 8601)"), "to": str_prop("End timestamp (ISO 8601)"), "actor": str_prop("Who performed the action"), "source": str_prop("Where the action originated") }),
            required: &["collection", "function", "bucket"],
            handler: handle_thing_timeseries,
            is_write: false,
            destructive: false,
            needs_collection: true,
        },
        // Vector search tool (1)
        ToolEntry {
            name: "thing_vector_search",
            description: "Search objects by vector similarity (cosine similarity)",
            properties: json!({ "collection": str_prop("Collection name"), "vector": arr_prop("Query vector as array of floats (e.g. [0.1, 0.2, 0.3])"), "topK": int_prop("Max results"), "filter": obj_prop("Metadata filter") }),
            required: &["collection", "vector"],
            handler: handle_thing_vector_search,
            is_write: false,
            destructive: false,
            needs_collection: true,
        },
    ]
});

pub async fn handle_mcp_request(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let tenant_id = extract_tenant_id(&headers, &state.tenant_config)?;
    let db_path = state.tenant_config.resolve_db_path(tenant_id.as_deref());
    let method = body.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let id = body.get("id");

    match method {
        "initialize" => Ok(Json(json!({
            "jsonrpc": "2.0",
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {}, "resources": {} },
                "serverInfo": { "name": "thingd-server", "version": env!("CARGO_PKG_VERSION") }
            },
            "id": id
        }))),

        "tools/list" => {
            let tools = all_tool_defs();
            Ok(Json(json!({
                "jsonrpc": "2.0",
                "result": { "tools": tools },
                "id": id
            })))
        },

        "tools/call" => {
            let params = body.get("params").cloned().unwrap_or(json!({}));
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

            let tool = ALL_TOOLS.iter().find(|t| t.name == tool_name);
            let tool = match tool {
                Some(t) => t,
                None => {
                    return Ok(mcp_error_result(id, &format!("Unknown tool: {tool_name}")));
                },
            };

            // Read-only check for write tools
            if tool.is_write && state.mcp_config.read_only {
                return Ok(mcp_error(id, -32603, "Server is in read-only mode"));
            }

            // Collection allowlist check
            if tool.needs_collection
                && let Some(collection) = arguments.get("collection").and_then(|v| v.as_str())
                && !state.mcp_config.collection_allowlist.is_empty()
                && !state
                    .mcp_config
                    .collection_allowlist
                    .contains(&collection.to_string())
            {
                return Ok(mcp_error(
                    id,
                    -32603,
                    &format!("Collection '{collection}' is not in the allowlist"),
                ));
            }

            let result = (tool.handler)(&state, tool_name, &arguments, &db_path);
            match result {
                Ok(content) => Ok(mcp_success(id, content)),
                Err(e) => Ok(mcp_error(id, -32603, &sanitize_detail(&e))),
            }
        },

        "ping" => Ok(Json(json!({
            "jsonrpc": "2.0", "result": {}, "id": id
        }))),

        _ => Ok(Json(json!({
            "jsonrpc": "2.0",
            "error": { "code": -32601, "message": "Method not found" },
            "id": id
        }))),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;
    use crate::engine::EnginePool;
    use crate::server::{self, AppState};

    fn test_state() -> Arc<AppState> {
        let config = Config::default();
        Arc::new(AppState {
            pool: Arc::new(EnginePool::new(":memory:".to_string())),
            tenant_config: config.tenant,
            mcp_config: config.mcp,
            auth_token: config.auth.token,
            allow_unauthenticated: config.auth.allow_unauthenticated,
            cluster_config: config.cluster,
            nlq_config: config.nlq,
        })
    }

    async fn call_mcp_with(state: &Arc<AppState>, body: Value) -> (StatusCode, Value) {
        let app = server::build_router(Arc::clone(state), &Config::default());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let collected = response.into_body().collect().await.unwrap_or_default();
        let bytes = collected.to_bytes();
        let value: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
        (status, value)
    }

    #[tokio::test]
    async fn test_mcp_initialize() {
        let state = test_state();
        let (_status, result) = call_mcp_with(
            &state,
            json!({ "jsonrpc": "2.0", "method": "initialize", "id": 1 }),
        )
        .await;
        assert_eq!(result["jsonrpc"], "2.0");
        assert!(result["result"]["protocolVersion"].is_string());
        assert_eq!(result["id"], 1);
    }

    #[tokio::test]
    async fn test_mcp_tools_list() {
        let state = test_state();
        let (_status, result) = call_mcp_with(
            &state,
            json!({ "jsonrpc": "2.0", "method": "tools/list", "id": 1 }),
        )
        .await;
        let tools = result["result"]["tools"].as_array().unwrap();
        assert_eq!(
            tools.len(),
            36,
            "expected 36 MCP tools, got {}",
            tools.len()
        );
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        for expected in &[
            "thing_search",
            "thing_get",
            "thing_put",
            "thing_delete",
            "thing_objects_list",
            "thing_objects_put_batch",
            "thing_objects_delete_batch",
            "thing_objects_get_batch",
            "thing_events_append",
            "thing_events_list",
            "thing_queue_push",
            "thing_queue_claim",
            "thing_queue_ack",
            "thing_queue_nack",
            "thing_queue_list",
            "thing_queue_dead",
            "thing_count_objects",
            "thing_count_objects_in_collection",
            "thing_count_events",
            "thing_count_active_jobs",
            "thing_count_dead_jobs",
            "thing_list_collections",
            "thing_list_streams",
            "thing_list_queues",
            "thing_create_index",
            "thing_list_indexes",
            "thing_link_create",
            "thing_link_get",
            "thing_link_delete",
            "thing_link_neighbors",
            "thing_link_count",
            "thing_schema",
            "thing_nlq",
            "thing_aggregate",
            "thing_timeseries",
            "thing_vector_search",
        ] {
            assert!(names.contains(expected), "missing tool: {expected}");
        }
    }

    #[tokio::test]
    async fn test_mcp_tools_call_thing_get_not_found() {
        let state = test_state();
        let (_status, result) = call_mcp_with(
            &state,
            json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_get", "arguments": { "collection": "test", "id": "nonexistent" } }, "id": 1 }),
        ).await;
        assert_eq!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_tools_call_thing_put_and_get() {
        let state = test_state();
        let (_status, result) = call_mcp_with(
            &state,
            json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_put", "arguments": { "collection": "test", "object": { "id": "mcp1", "val": 42 } } }, "id": 1 }),
        ).await;
        assert!(
            result["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Created/updated")
        );

        let (_status, result) = call_mcp_with(
            &state,
            json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_get", "arguments": { "collection": "test", "id": "mcp1" } }, "id": 2 }),
        ).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_tools_call_thing_search() {
        let state = test_state();
        call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_put", "arguments": { "collection": "search_test", "object": { "id": "s1", "text": "hello world" } } }, "id": 1 })).await;
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_search", "arguments": { "query": "hello" } }, "id": 2 })).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_tools_call_thing_delete() {
        let state = test_state();
        call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_put", "arguments": { "collection": "test", "object": { "id": "delete_me" } } }, "id": 1 })).await;
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_delete", "arguments": { "collection": "test", "id": "delete_me" } }, "id": 2 })).await;
        assert!(
            result["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Deleted")
        );
    }

    #[tokio::test]
    async fn test_mcp_tools_call_thing_objects_list() {
        let state = test_state();
        for i in 0..3 {
            call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_put", "arguments": { "collection": "list_test", "object": { "id": format!("l{i}") } } }, "id": i })).await;
        }
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_objects_list", "arguments": { "collection": "list_test" } }, "id": 10 })).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_tools_call_thing_events_append_and_list() {
        let state = test_state();
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_events_append", "arguments": { "stream": "test_stream", "event": { "type": "test.event", "data": "hello" } } }, "id": 1 })).await;
        assert_ne!(result["result"]["isError"], true);

        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_events_list", "arguments": { "stream": "test_stream" } }, "id": 2 })).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_tools_call_thing_queue_push_and_claim() {
        let state = test_state();
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_queue_push", "arguments": { "queue": "test_queue", "payload": { "task": "test" } } }, "id": 1 })).await;
        assert_ne!(result["result"]["isError"], true);

        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_queue_claim", "arguments": { "queue": "test_queue" } }, "id": 2 })).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_tools_call_thing_count_tools() {
        let state = test_state();
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_count_objects", "arguments": {} }, "id": 1 })).await;
        assert_ne!(result["result"]["isError"], true);

        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_list_collections", "arguments": {} }, "id": 2 })).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_tools_call_thing_link_tools() {
        let state = test_state();
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_link_create", "arguments": { "fromRef": "a", "linkType": "refs", "toRef": "b" } }, "id": 1 })).await;
        assert_ne!(result["result"]["isError"], true);

        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_link_count", "arguments": {} }, "id": 2 })).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_thing_objects_put_batch() {
        let state = test_state();
        let (_status, result) = call_mcp_with(
            &state,
            json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_objects_put_batch", "arguments": { "collection": "batch_test", "objects": [{"id":"b1","v":1},{"id":"b2","v":2}] } }, "id": 1 }),
        ).await;
        assert_ne!(result["result"]["isError"], true);
        let text = result["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("2"));
    }

    #[tokio::test]
    async fn test_mcp_thing_objects_delete_batch() {
        let state = test_state();
        call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_objects_put_batch", "arguments": { "collection": "del_batch", "objects": [{"id":"d1"},{"id":"d2"}] } }, "id": 1 })).await;
        let (_status, result) = call_mcp_with(
            &state,
            json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_objects_delete_batch", "arguments": { "collection": "del_batch", "ids": ["d1","d2"] } }, "id": 2 }),
        ).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_thing_queue_ack_and_nack() {
        let state = test_state();
        // Push
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_queue_push", "arguments": { "queue": "ackq", "payload": {"t":"j"} } }, "id": 1 })).await;
        assert_ne!(result["result"]["isError"], true);
        // Claim
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_queue_claim", "arguments": { "queue": "ackq" } }, "id": 2 })).await;
        assert_ne!(result["result"]["isError"], true);
        let text = result["result"]["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        let job_id = parsed["id"].as_str().unwrap();
        // Ack (MCP uses "id" not "jobId")
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_queue_ack", "arguments": { "queue": "ackq", "id": job_id } }, "id": 3 })).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_thing_queue_nack() {
        let state = test_state();
        call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_queue_push", "arguments": { "queue": "nackq", "payload": {"t":"j"} } }, "id": 1 })).await;
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_queue_claim", "arguments": { "queue": "nackq" } }, "id": 2 })).await;
        let text = result["result"]["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        let job_id = parsed["id"].as_str().unwrap();
        // Nack (MCP uses "id" not "jobId")
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_queue_nack", "arguments": { "queue": "nackq", "id": job_id, "error": "oops" } }, "id": 3 })).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_thing_queue_list() {
        let state = test_state();
        call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_queue_push", "arguments": { "queue": "listq", "payload": {"k":"v"} } }, "id": 1 })).await;
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_queue_list", "arguments": { "queue": "listq" } }, "id": 2 })).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_thing_queue_dead() {
        let state = test_state();
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_queue_dead", "arguments": { "queue": "deadq" } }, "id": 1 })).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_thing_count_events() {
        let state = test_state();
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_count_events", "arguments": {} }, "id": 1 })).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_thing_count_active_jobs() {
        let state = test_state();
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_count_active_jobs", "arguments": { "queue": "q" } }, "id": 1 })).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_thing_count_dead_jobs() {
        let state = test_state();
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_count_dead_jobs", "arguments": { "queue": "q" } }, "id": 1 })).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_thing_list_streams() {
        let state = test_state();
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_list_streams", "arguments": {} }, "id": 1 })).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_thing_list_queues() {
        let state = test_state();
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_list_queues", "arguments": {} }, "id": 1 })).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_thing_link_get_and_delete_and_neighbors() {
        let state = test_state();
        // Create
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_link_create", "arguments": { "fromRef": "u1", "linkType": "knows", "toRef": "u2" } }, "id": 1 })).await;
        assert_ne!(result["result"]["isError"], true);
        let text = result["result"]["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        let link_id = parsed["id"].as_str().unwrap();
        // Get
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_link_get", "arguments": { "id": link_id } }, "id": 2 })).await;
        assert_ne!(result["result"]["isError"], true);
        // Neighbors
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_link_neighbors", "arguments": { "reference": "u1", "direction": "Outgoing" } }, "id": 3 })).await;
        assert_ne!(result["result"]["isError"], true);
        // Delete
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_link_delete", "arguments": { "id": link_id } }, "id": 4 })).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_unknown_tool() {
        let state = test_state();
        let (_status, result) = call_mcp_with(&state, json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "nonexistent_tool", "arguments": {} }, "id": 1 })).await;
        assert_eq!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_ping() {
        let state = test_state();
        let (_status, result) = call_mcp_with(
            &state,
            json!({ "jsonrpc": "2.0", "method": "ping", "id": 1 }),
        )
        .await;
        assert_eq!(result["result"], json!({}));
    }

    #[tokio::test]
    async fn test_mcp_method_not_found() {
        let state = test_state();
        let (_status, result) = call_mcp_with(
            &state,
            json!({ "jsonrpc": "2.0", "method": "unknown_method", "id": 1 }),
        )
        .await;
        assert_eq!(result["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn test_mcp_thing_aggregate() {
        let state = test_state();
        // Seed objects
        for i in 0..5 {
            let (_status, _result) = call_mcp_with(
                &state,
                json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_put", "arguments": { "collection": "test", "object": { "id": format!("obj{i}"), "value": i } } }, "id": 1 }),
            ).await;
        }
        let (_status, result) = call_mcp_with(
            &state,
            json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_aggregate", "arguments": { "collection": "test", "function": "count" } }, "id": 1 }),
        ).await;
        assert_ne!(result["result"]["isError"], true);
        let text = result["result"]["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["total"], 5.0);
    }

    #[tokio::test]
    async fn test_mcp_thing_aggregate_sum() {
        let state = test_state();
        for i in 0..5 {
            let (_status, _result) = call_mcp_with(
                &state,
                json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_put", "arguments": { "collection": "t", "object": { "id": format!("obj{i}"), "value": i } } }, "id": 1 }),
            ).await;
        }
        let (_status, result) = call_mcp_with(
            &state,
            json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_aggregate", "arguments": { "collection": "t", "function": "sum", "field": "value" } }, "id": 1 }),
        ).await;
        assert_ne!(result["result"]["isError"], true);
        let text = result["result"]["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["total"], 10.0);
    }

    #[tokio::test]
    async fn test_mcp_thing_timeseries() {
        let state = test_state();
        let (_status, _result) = call_mcp_with(
            &state,
            json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_put", "arguments": { "collection": "ts", "object": { "id": "obj1", "value": 1 } } }, "id": 1 }),
        ).await;
        let (_status, result) = call_mcp_with(
            &state,
            json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_timeseries", "arguments": { "collection": "ts", "function": "count", "bucket": "day" } }, "id": 1 }),
        ).await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_thing_vector_search() {
        let state = test_state();
        // Put an object with a vector
        let (_status, _result) = call_mcp_with(
            &state,
            json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_put", "arguments": { "collection": "v", "object": { "id": "doc1", "text": "alpha", "vector": [1.0, 0.0, 0.0] } } }, "id": 1 }),
        ).await;
        // Put another with a different vector
        let (_status, _result) = call_mcp_with(
            &state,
            json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_put", "arguments": { "collection": "v", "object": { "id": "doc2", "text": "beta", "vector": [0.0, 1.0, 0.0] } } }, "id": 2 }),
        ).await;
        // Search with vector similar to doc1
        let (_status, result) = call_mcp_with(
            &state,
            json!({ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "thing_vector_search", "arguments": { "collection": "v", "vector": [0.9, 0.1, 0.0], "topK": 5 } }, "id": 3 }),
        ).await;
        assert_ne!(result["result"]["isError"], true);
        let text = result["result"]["content"][0]["text"].as_str().unwrap();
        let hits: Vec<Value> = serde_json::from_str(text).unwrap_or_default();
        assert!(!hits.is_empty(), "expected at least one vector search hit");
        assert_eq!(hits[0]["id"], "doc1", "most similar should be doc1");
        assert!(
            hits[0]["score"].as_f64().unwrap() > hits[1]["score"].as_f64().unwrap(),
            "doc1 should score higher than doc2"
        );
    }
}
