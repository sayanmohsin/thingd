use axum::{Json, extract::State};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::engine::EnginePool;
use crate::error::AppError;

pub async fn handle_mcp_request(
    State(_pool): State<Arc<EnginePool>>,
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
                    { "name": "thing_search", "description": "Search", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"] } },
                ]
            },
            "id": body.get("id")
        }))),
        "tools/call" => Err(AppError::internal(
            "Tool not yet implemented in MCP handler",
        )),
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
