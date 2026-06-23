use axum::{Json, extract::State};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::server::AppState;

pub async fn cluster_status(State(_state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "mode": "single",
        "writable": true,
        "forwarding": false,
    }))
}

pub async fn cluster_peers(State(_state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "peers": [],
        "discovery": "static",
    }))
}
