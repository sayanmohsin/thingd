use axum::{Json, extract::State};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::engine::EnginePool;
use crate::error::AppError;

pub async fn handle_mcp_request(
    State(pool): State<Arc<EnginePool>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let method = body.get("method").and_then(|v| v.as_str()).unwrap_or("");

    match method {
        "initialize" => Ok(Json(json!({
            "jsonrpc": "2.0",
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {},
                    "resources": {}
                },
                "serverInfo": {
                    "name": "thingd-server",
                    "version": env!("CARGO_PKG_VERSION")
                }
            },
            "id": body.get("id")
        }))),
        "tools/list" => Ok(Json(json!({
            "jsonrpc": "2.0",
            "result": {
                "tools": [
                    { "name": "thing_search", "description": "Search objects and events", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"] } },
                    { "name": "thing_get", "description": "Get object by ID", "inputSchema": { "type": "object", "properties": { "collection": { "type": "string" }, "id": { "type": "string" } }, "required": ["collection", "id"] } },
                    { "name": "thing_put", "description": "Create or update object", "inputSchema": { "type": "object", "properties": { "collection": { "type": "string" }, "object": { "type": "object" } }, "required": ["collection", "object"] } },
                    { "name": "thing_delete", "description": "Delete object", "inputSchema": { "type": "object", "properties": { "collection": { "type": "string" }, "id": { "type": "string" } }, "required": ["collection", "id"] } },
                    { "name": "thing_objects_list", "description": "List objects", "inputSchema": { "type": "object", "properties": { "collection": { "type": "string" } }, "required": ["collection"] } },
                ]
            },
            "id": body.get("id")
        }))),
        "tools/call" => {
            let params = body.get("params").cloned().unwrap_or(json!({}));
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

            let engine = pool.get("");
            let mut store = engine.lock().await;

            let result = match tool_name {
                "thing_search" => {
                    let query = arguments
                        .get("query")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let opts = thingd::SearchOptions {
                        collections: arguments.get("collections").and_then(|v| v.as_array()).map(
                            |a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            },
                        ),
                        limit: arguments
                            .get("limit")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as usize),
                        filter: arguments.get("filter").cloned(),
                    };
                    store.search(query, opts)
                        .map(|hits| json!({ "content": [{ "type": "text", "text": serde_json::to_string(&hits).unwrap_or_default() }] }))
                        .map_err(|e| AppError::internal(e.to_string()))
                },
                "thing_get" => {
                    let collection = arguments
                        .get("collection")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let id = arguments.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    match store
                        .get_object(collection, id)
                        .map_err(|e| AppError::internal(e.to_string()))?
                    {
                        Some(obj) => {
                            let body_val: Value =
                                serde_json::from_str(&obj.body).unwrap_or(Value::Null);
                            Ok(
                                json!({ "content": [{ "type": "text", "text": serde_json::to_string(&body_val).unwrap_or_default() }] }),
                            )
                        },
                        None => Ok(
                            json!({ "content": [{ "type": "text", "text": "Object not found" }], "isError": true }),
                        ),
                    }
                },
                "thing_put" => {
                    let collection = arguments
                        .get("collection")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let obj = arguments.get("object").cloned().unwrap_or(json!({}));
                    let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("new");
                    let memory_obj = thingd::MemoryObject::new(
                        collection.to_string(),
                        id.to_string(),
                        obj.to_string(),
                    );
                    let result = store.put_object(memory_obj)
                        .map(|r| json!({ "content": [{ "type": "text", "text": format!("Created/updated: {}/{}", r.key.collection, r.key.id) }] }))
                        .map_err(|e| AppError::internal(e.to_string()))?;
                    Ok(result)
                },
                "thing_delete" => {
                    let collection = arguments
                        .get("collection")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let id = arguments.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let deleted = store
                        .delete_object(collection, id)
                        .map_err(|e| AppError::internal(e.to_string()))?;
                    Ok(
                        json!({ "content": [{ "type": "text", "text": format!("Deleted: {deleted}") }] }),
                    )
                },
                "thing_objects_list" => {
                    let collection = arguments
                        .get("collection")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let opts = thingd::ListObjectsOptions::default();
                    let objects = store
                        .list_objects(Some(&[collection.to_string()]), &opts)
                        .map_err(|e| AppError::internal(e.to_string()))?;
                    let items: Vec<Value> = objects
                        .iter()
                        .map(|obj| json!({ "id": obj.key.id, "collection": obj.key.collection }))
                        .collect();
                    Ok(
                        json!({ "content": [{ "type": "text", "text": serde_json::to_string(&items).unwrap_or_default() }] }),
                    )
                },
                _ => Ok(json!({
                    "content": [{ "type": "text", "text": format!("Unknown tool: {tool_name}") }],
                    "isError": true
                })),
            };

            match result {
                Ok(content) => Ok(Json(json!({
                    "jsonrpc": "2.0",
                    "result": content,
                    "id": body.get("id")
                }))),
                Err(e) => Ok(Json(json!({
                    "jsonrpc": "2.0",
                    "error": { "code": -32603, "message": e.detail },
                    "id": body.get("id")
                }))),
            }
        },
        "ping" => Ok(Json(json!({
            "jsonrpc": "2.0",
            "result": {},
            "id": body.get("id")
        }))),
        _ => Ok(Json(json!({
            "jsonrpc": "2.0",
            "error": { "code": -32601, "message": "Method not found" },
            "id": body.get("id")
        }))),
    }
}
