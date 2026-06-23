use axum::{
    Router,
    http::{Method, header},
    middleware,
    routing::{get, post, put},
};
use std::sync::Arc;
use std::time::Duration;
use tower_http::cors::CorsLayer;

use crate::auth::auth_middleware;
use crate::config::{Config, McpConfig};
use crate::engine::EnginePool;
use crate::rest;

/// Shared application state passed to all handlers via axum's State extractor.
pub struct AppState {
    pub pool: EnginePool,
    pub mcp_config: McpConfig,
}

pub fn build_router(state: Arc<AppState>, config: &Config) -> Router {
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
        .with_state(state);

    // Configurable CORS — empty origins = permissive (backward compat)
    if config.hardening.cors_allowed_origins.is_empty() {
        router = router.layer(CorsLayer::permissive());
    } else {
        let origins: Vec<axum::http::HeaderValue> = config
            .hardening
            .cors_allowed_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        let cors = CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([
                header::AUTHORIZATION,
                header::CONTENT_TYPE,
                header::HeaderName::from_static("mcp-protocol-version"),
            ])
            .max_age(Duration::from_secs(config.hardening.cors_max_age_secs));
        router = router.layer(cors);
    }

    // Wire auth middleware when a token is configured and unauthenticated access is not allowed
    if !config.auth.allow_unauthenticated && !config.auth.token.is_empty() {
        router = router.layer(middleware::from_fn(auth_middleware));
    }

    // Apply rate limiting
    if config.hardening.rate_limit_enabled {
        let limiter =
            crate::rate_limit::RateLimiter::new(config.hardening.rate_limit_requests_per_minute);
        router = router.layer(middleware::from_fn_with_state(
            Arc::new(limiter),
            crate::rate_limit::rate_limit_middleware,
        ));
    }

    // Apply hardening body size limit
    if config.hardening.max_payload_bytes > 0 {
        router = router.layer(axum::extract::DefaultBodyLimit::max(
            config.hardening.max_payload_bytes,
        ));
    }

    router
}
