mod auth;
mod cluster;
mod config;
mod engine;
mod error;
mod mcp;
mod rate_limit;
mod rest;
mod server;

use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("thingd_server=info".parse().unwrap()),
        )
        .init();

    let config = config::Config::load(std::env::var("THINGD_CONFIG").ok().as_deref())
        .unwrap_or_else(|e| {
            eprintln!("Config error: {}", e);
            std::process::exit(1);
        });

    // Set global production mode for error sanitization
    error::set_production_mode(config.server.production_mode);

    tracing::info!(
        "Starting thingd-server on {}:{} (database: {})",
        config.server.host,
        config.server.port,
        config.server.database,
    );

    let pool = engine::EnginePool::new(config.server.database.clone());
    let app_state = Arc::new(server::AppState {
        pool,
        mcp_config: config.mcp.clone(),
    });
    let app = server::build_router(Arc::clone(&app_state), &config)
        .into_make_service_with_connect_info::<SocketAddr>();

    let listener =
        tokio::net::TcpListener::bind(format!("{}:{}", config.server.host, config.server.port))
            .await
            .unwrap_or_else(|e| {
                eprintln!("Failed to bind: {}", e);
                std::process::exit(1);
            });

    tracing::info!("Listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap_or_else(|e| {
        eprintln!("Server error: {}", e);
        std::process::exit(1);
    });
}
