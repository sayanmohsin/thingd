use axum::{Json, extract::State};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::config::ClusterMode;
use crate::server::AppState;

pub async fn cluster_status(State(state): State<Arc<AppState>>) -> Json<Value> {
    let cfg = &state.cluster_config;
    let mode_str = match cfg.mode {
        ClusterMode::Single => "single",
        ClusterMode::Leader => "leader",
        ClusterMode::Follower => "follower",
    };
    Json(json!({
        "mode": mode_str,
        "writable": cfg.mode != ClusterMode::Follower,
        "forwarding": cfg.mode == ClusterMode::Follower,
        "advertise_url": cfg.advertise_url,
        "leader_url": cfg.leader_url,
        "leader_election": cfg.leader_election,
        "discovery": cfg.discovery,
    }))
}

pub async fn cluster_peers(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "peers": state.cluster_config.peers,
        "discovery": state.cluster_config.discovery,
    }))
}
