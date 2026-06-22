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

            let result = match tool_name {
                "thing_search" => {
                    let e = pool.get("");
                    let g = e.lock();
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
                    g.search(query, opts)
                        .map(|hits| json!({ "content": [{ "type": "text", "text": serde_json::to_string(&hits).unwrap_or_default() }] }))
                        .map_err(|e| AppError::internal(e.to_string()))
                },
                "thing_get" => {
                    let e = pool.get("");
                    let g = e.lock();
                    let collection = arguments
                        .get("collection")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let id = arguments.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    match g
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
                    let e = pool.get("");
                    let mut g = e.lock();
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
                    let result = g.put_object(memory_obj)
                        .map(|r| json!({ "content": [{ "type": "text", "text": format!("Created/updated: {}/{}", r.key.collection, r.key.id) }] }))
                        .map_err(|e| AppError::internal(e.to_string()))?;
                    Ok(result)
                },
                "thing_delete" => {
                    let e = pool.get("");
                    let mut g = e.lock();
                    let collection = arguments
                        .get("collection")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let id = arguments.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let deleted = g
                        .delete_object(collection, id)
                        .map_err(|e| AppError::internal(e.to_string()))?;
                    Ok(
                        json!({ "content": [{ "type": "text", "text": format!("Deleted: {deleted}") }] }),
                    )
                },
                "thing_objects_list" => {
                    let e = pool.get("");
                    let g = e.lock();
                    let collection = arguments
                        .get("collection")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let opts = thingd::ListObjectsOptions::default();
                    let objects = g
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

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::engine::EnginePool;
    use crate::server;

    fn test_pool() -> Arc<EnginePool> {
        Arc::new(EnginePool::new(":memory:".to_string()))
    }

    async fn call_mcp_with(pool: &Arc<EnginePool>, body: Value) -> (StatusCode, Value) {
        let app = server::build_router(pool.clone(), &crate::config::Config::default());
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
        let pool = test_pool();
        let (_status, result) = call_mcp_with(
            &pool,
            json!({
                "jsonrpc": "2.0",
                "method": "initialize",
                "id": 1
            }),
        )
        .await;
        assert_eq!(result["jsonrpc"], "2.0");
        assert!(result["result"]["protocolVersion"].is_string());
        assert_eq!(result["id"], 1);
    }

    #[tokio::test]
    async fn test_mcp_tools_list() {
        let pool = test_pool();
        let (_status, result) = call_mcp_with(
            &pool,
            json!({
                "jsonrpc": "2.0",
                "method": "tools/list",
                "id": 1
            }),
        )
        .await;
        let tools = result["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 5);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"thing_search"));
        assert!(names.contains(&"thing_get"));
        assert!(names.contains(&"thing_put"));
        assert!(names.contains(&"thing_delete"));
        assert!(names.contains(&"thing_objects_list"));
        assert_eq!(names.len(), 5);
    }

    #[tokio::test]
    async fn test_mcp_tools_call_thing_get_not_found() {
        let pool = test_pool();
        let (_status, result) = call_mcp_with(
            &pool,
            json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "thing_get",
                    "arguments": { "collection": "test", "id": "nonexistent" }
                },
                "id": 1
            }),
        )
        .await;
        assert_eq!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_tools_call_thing_put_and_get() {
        let pool = test_pool();
        let (_status, result) = call_mcp_with(
            &pool,
            json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "thing_put",
                    "arguments": { "collection": "test", "object": { "id": "mcp1", "val": 42 } }
                },
                "id": 1
            }),
        )
        .await;
        assert!(
            result["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Created/updated")
        );

        let (_status, result) = call_mcp_with(
            &pool,
            json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "thing_get",
                    "arguments": { "collection": "test", "id": "mcp1" }
                },
                "id": 2
            }),
        )
        .await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_tools_call_thing_search() {
        let pool = test_pool();
        call_mcp_with(&pool, json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": "thing_put",
                "arguments": { "collection": "search_test", "object": { "id": "s1", "text": "hello world" } }
            },
            "id": 1
        }))
        .await;

        let (_status, result) = call_mcp_with(
            &pool,
            json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "thing_search",
                    "arguments": { "query": "hello" }
                },
                "id": 2
            }),
        )
        .await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_tools_call_thing_delete() {
        let pool = test_pool();
        call_mcp_with(
            &pool,
            json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "thing_put",
                    "arguments": { "collection": "test", "object": { "id": "delete_me" } }
                },
                "id": 1
            }),
        )
        .await;

        let (_status, result) = call_mcp_with(
            &pool,
            json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "thing_delete",
                    "arguments": { "collection": "test", "id": "delete_me" }
                },
                "id": 2
            }),
        )
        .await;
        assert!(
            result["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Deleted")
        );
    }

    #[tokio::test]
    async fn test_mcp_tools_call_thing_objects_list() {
        let pool = test_pool();
        for i in 0..3 {
            call_mcp_with(&pool, json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "thing_put",
                    "arguments": { "collection": "list_test", "object": { "id": format!("l{i}") } }
                },
                "id": i
            }))
            .await;
        }

        let (_status, result) = call_mcp_with(
            &pool,
            json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "thing_objects_list",
                    "arguments": { "collection": "list_test" }
                },
                "id": 10
            }),
        )
        .await;
        assert_ne!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_unknown_tool() {
        let pool = test_pool();
        let (_status, result) = call_mcp_with(
            &pool,
            json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "nonexistent_tool",
                    "arguments": {}
                },
                "id": 1
            }),
        )
        .await;
        assert_eq!(result["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_mcp_ping() {
        let pool = test_pool();
        let (_status, result) = call_mcp_with(
            &pool,
            json!({
                "jsonrpc": "2.0",
                "method": "ping",
                "id": 1
            }),
        )
        .await;
        assert_eq!(result["result"], json!({}));
    }

    #[tokio::test]
    async fn test_mcp_method_not_found() {
        let pool = test_pool();
        let (_status, result) = call_mcp_with(
            &pool,
            json!({
                "jsonrpc": "2.0",
                "method": "unknown_method",
                "id": 1
            }),
        )
        .await;
        assert_eq!(result["error"]["code"], -32601);
    }
}
