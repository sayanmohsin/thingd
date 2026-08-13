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
use std::path::PathBuf;
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

fn check_path_from_args() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--check" {
            return args.next().map(PathBuf::from);
        }
        if let Some(path) = arg.strip_prefix("--check=") {
            return Some(PathBuf::from(path));
        }
    }
    None
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

    if std::env::args().any(|arg| arg == "--check" || arg.starts_with("--check=")) {
        let Some(path) = check_path_from_args() else {
            eprintln!("Usage: thingd-server --check <database-path>");
            std::process::exit(2);
        };
        match thingd::PersistentEngine::validate_path(&path) {
            Ok(report) => {
                println!(
                    "OK: format={} legacy_manifest={} lock_present={} keyspaces_present={} search_index_compatible={:?}",
                    report.format_version,
                    report.legacy_manifest,
                    report.lock_present,
                    report.keyspaces_present,
                    report.search_index_compatible,
                );
            },
            Err(error) => {
                eprintln!("ERROR: {error}");
                std::process::exit(1);
            },
        }
        return;
    }

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

    if config.auth.token.is_empty()
        && (config.tenant.mode != config::TenantMode::MultiTenant
            || config.auth.tenant_tokens.is_empty())
    {
        tracing::warn!(
            "No auth token configured — server is unauthenticated. Set THINGD_AUTH_TOKEN for production."
        );
    }

    let pool = Arc::new(
        engine::EnginePool::new_with_encryption_key_and_search_mode(
            config.server.database.clone(),
            config.server.encryption_key.as_deref(),
            match config.server.search_mode {
                config::SearchModeConfig::Persistent => {
                    thingd::PersistentSearchMode::PersistentAsync
                },
                config::SearchModeConfig::PersistentNoRebuild => {
                    thingd::PersistentSearchMode::PersistentNoRebuild
                },
                config::SearchModeConfig::Disabled => thingd::PersistentSearchMode::Disabled,
            },
        )
        .unwrap_or_else(|e| {
            eprintln!("Encryption configuration error: {e}");
            std::process::exit(1);
        }),
    );
    let app_state = Arc::new(server::AppState {
        pool,
        tenant_config: config.tenant.clone(),
        mcp_config: config.mcp.clone(),
        auth_token: config.auth.token.clone(),
        tenant_tokens: config.auth.tenant_tokens.clone(),
        auth_verifier: match config.auth.mode {
            config::AuthMode::TenantJwt => Some(Arc::new(
                auth::AuthVerifier::new(&config.auth).unwrap_or_else(|e| {
                    eprintln!("Auth verifier error: {}", e);
                    std::process::exit(1);
                }),
            )),
            config::AuthMode::Bearer => None,
        },
        allow_unauthenticated: config.auth.allow_unauthenticated,
        cluster_config: config.cluster.clone(),
        nlq_config: config.nlq.clone(),
        sync_config: config.sync.clone(),
        hardening_config: config.hardening.clone(),
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
