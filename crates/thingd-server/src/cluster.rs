use axum::{Json, extract::State};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::engine::EnginePool;

pub async fn cluster_status(State(_pool): State<Arc<EnginePool>>) -> Json<Value> {
    Json(json!({
        "mode": "single",
        "writable": true,
        "forwarding": false,
    }))
}

pub async fn cluster_peers(State(_pool): State<Arc<EnginePool>>) -> Json<Value> {
    Json(json!({
        "peers": [],
        "discovery": "static",
    }))
}
