mod auth;
mod cluster;
mod config;
mod engine;
mod error;
mod mcp;
mod nlq;
mod rate_limit;
mod rest;
mod server;

use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[cfg(unix)]
async fn shutdown_signal() {
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("failed to install SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received SIGINT, starting graceful shutdown...");
        }
        _ = sigterm.recv() => {
            tracing::info!("Received SIGTERM, starting graceful shutdown...");
        }
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.ok();
    tracing::info!("Received SIGINT, starting graceful shutdown...");
}

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

    if config.auth.token.is_empty() {
        tracing::warn!(
            "No auth token configured — server is unauthenticated. Set THINGD_AUTH_TOKEN for production."
        );
    }

    let pool = Arc::new(engine::EnginePool::new(config.server.database.clone()));
    let app_state = Arc::new(server::AppState {
        pool,
        tenant_config: config.tenant.clone(),
        mcp_config: config.mcp.clone(),
        auth_token: config.auth.token.clone(),
        allow_unauthenticated: config.auth.allow_unauthenticated,
        cluster_config: config.cluster.clone(),
        nlq_config: config.nlq.clone(),
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
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap_or_else(|e| {
            eprintln!("Server error: {}", e);
            std::process::exit(1);
        });

    tracing::info!("Shutdown complete");
}
