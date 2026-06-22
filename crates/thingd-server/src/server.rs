use axum::{
    Router, middleware,
    routing::{get, post, put},
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use crate::auth::auth_middleware;
use crate::config::Config;
use crate::engine::EnginePool;
use crate::rest;

pub fn build_router(pool: Arc<EnginePool>, config: &Config) -> Router {
    let mut router = Router::new()
        .route("/healthz", get(rest::health))
        .route("/v1/health", get(rest::health))
        .route("/v1/counts/objects", get(rest::count_objects))
        .route("/v1/counts/events", get(rest::count_events))
        .route("/v1/counts/links", get(rest::count_links))
        // Collections / Streams / Queues
        .route("/v1/collections", get(rest::list_collections))
        .route("/v1/streams", get(rest::list_streams))
        .route("/v1/queues", get(rest::list_queues))
        // Objects
        .route("/v1/objects", get(rest::list_objects))
        .route("/v1/objects/batch", put(rest::put_batch).delete(rest::delete_batch))
        .route("/v1/objects/{collection}/{id}", put(rest::put_object).get(rest::get_object).delete(rest::delete_object))
        // Search
        .route("/v1/search", post(rest::search))
        // Events
        .route("/v1/events/{stream}", post(rest::append_event))
        .route("/v1/events", get(rest::list_events))
        // Queues
        .route("/v1/queues/{queue}/push", post(rest::push_job))
        .route("/v1/queues/{queue}/claim", post(rest::claim_job))
        .route("/v1/queues/{queue}/ack", post(rest::ack_job))
        .route("/v1/queues/{queue}/nack", post(rest::nack_job))
        .route("/v1/queues/{queue}/jobs", get(rest::list_jobs))
        .route("/v1/queues/{queue}/dead", get(rest::list_dead_jobs))
        // Links
        .route("/v1/links", post(rest::create_link).get(rest::get_links))
        .route("/v1/links/{id}", get(rest::get_link_by_id).delete(rest::delete_link))
        // MCP
        .route("/mcp", post(crate::mcp::handle_mcp_request))
        // Cluster
        .route("/cluster/status", get(crate::cluster::cluster_status))
        .route("/cluster/peers", get(crate::cluster::cluster_peers))
        .layer(CorsLayer::permissive())
        .with_state(pool);

    // Wire auth middleware when a token is configured and unauthenticated access is not allowed
    if !config.auth.allow_unauthenticated && !config.auth.token.is_empty() {
        router = router.layer(middleware::from_fn(auth_middleware));
    }

    // Apply hardening body size limit
    if config.hardening.max_payload_bytes > 0 {
        router = router.layer(axum::extract::DefaultBodyLimit::max(
            config.hardening.max_payload_bytes,
        ));
    }

    router
}
