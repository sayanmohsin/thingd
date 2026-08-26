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

use std::io::{Read, Write};
use std::net::SocketAddr;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
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

fn has_arg(flag: &str) -> bool {
    std::env::args().any(|arg| arg == flag)
}

fn healthcheck_url_from_args() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--healthcheck" {
            return args.next();
        }
        if let Some(url) = arg.strip_prefix("--healthcheck=") {
            return Some(url.to_string());
        }
    }
    None
}

fn run_healthcheck(url: &str) -> Result<(), String> {
    let target = url
        .strip_prefix("http://")
        .ok_or_else(|| "healthcheck URL must use http://".to_string())?;
    let (authority, path) = target.split_once('/').unwrap_or((target, "/"));
    if authority.is_empty() {
        return Err("healthcheck URL is missing a host".to_string());
    }
    let address = authority
        .parse::<std::net::SocketAddr>()
        .map_err(|_| format!("healthcheck URL must use a host:port address: {url}"))?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))
        .map_err(|error| format!("healthcheck connection failed: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("healthcheck read timeout setup failed: {error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("healthcheck write timeout setup failed: {error}"))?;
    write!(
        stream,
        "GET /{path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n"
    )
    .map_err(|error| format!("healthcheck request failed: {error}"))?;
    let mut response = [0_u8; 64];
    let bytes_read = stream
        .read(&mut response)
        .map_err(|error| format!("healthcheck response failed: {error}"))?;
    let response = std::str::from_utf8(&response[..bytes_read])
        .map_err(|_| "healthcheck returned an invalid HTTP response".to_string())?;
    let status = response
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "healthcheck returned an incomplete HTTP response".to_string())?;
    if status == "200" {
        Ok(())
    } else {
        Err(format!("healthcheck returned HTTP {status}"))
    }
}

fn parse_storage_backend(value: &str) -> Result<thingd::PersistentBackend, String> {
    match value {
        "rocksdb" => Ok(thingd::PersistentBackend::RocksDb),
        "thingdb" => Ok(thingd::PersistentBackend::ThingDb),
        _ => Err(format!(
            "invalid storage backend {value:?}; expected rocksdb or thingdb"
        )),
    }
}

fn configured_storage_backend() -> Result<thingd::PersistentBackend, String> {
    parse_storage_backend(
        &std::env::var("THINGD_STORAGE_BACKEND").unwrap_or_else(|_| "rocksdb".to_string()),
    )
}

fn value_from_args(flag: &str) -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == flag {
            return args.next().map(PathBuf::from);
        }
        if let Some(path) = arg.strip_prefix(&format!("{flag}=")) {
            return Some(PathBuf::from(path));
        }
    }
    None
}

fn parse_encryption_key(value: Option<&str>) -> Result<Option<thingd::EncryptionConfig>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.len() != 64 {
        return Err("encryption key must contain 64 hexadecimal characters".to_string());
    }
    let bytes = (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| "encryption key must contain hexadecimal characters".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    thingd::EncryptionConfig::from_key(&bytes)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn maintenance_command() -> Option<&'static str> {
    if std::env::args().any(|arg| arg == "--compact" || arg.starts_with("--compact=")) {
        Some("compact")
    } else if std::env::args().any(|arg| arg == "--repack" || arg.starts_with("--repack=")) {
        Some("repack")
    } else {
        None
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

    if std::env::args().any(|arg| arg == "--check" || arg.starts_with("--check=")) {
        let Some(path) = check_path_from_args() else {
            eprintln!("Usage: thingd-server --check <database-path> [--require-migrated]");
            std::process::exit(2);
        };
        let backend = configured_storage_backend().unwrap_or_else(|error| {
            eprintln!("ERROR: {error}");
            std::process::exit(2);
        });
        match thingd::PersistentEngine::validate_path_with_backend(&path, backend) {
            Ok(report) => {
                if has_arg("--require-migrated") && report.legacy_manifest {
                    eprintln!(
                        "ERROR: storage path is a new or legacy-unmarked directory; expected a validated migrated store: {}",
                        path.display()
                    );
                    std::process::exit(1);
                }
                println!(
                    "OK: path={} format={} legacy_manifest={} lock_present={} keyspaces_present={} search_index_compatible={:?}",
                    path.display(),
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

    if has_arg("--healthcheck") || std::env::args().any(|arg| arg.starts_with("--healthcheck=")) {
        let Some(url) = healthcheck_url_from_args() else {
            eprintln!("Usage: thingd-server --healthcheck http://127.0.0.1:8757/healthz");
            std::process::exit(2);
        };
        if let Err(error) = run_healthcheck(&url) {
            eprintln!("Healthcheck failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    let config = config::Config::load(std::env::var("THINGD_CONFIG").ok().as_deref())
        .unwrap_or_else(|e| {
            eprintln!("Config error: {}", e);
            std::process::exit(1);
        });

    if let Some(command) = maintenance_command() {
        let key =
            parse_encryption_key(config.server.encryption_key.as_deref()).unwrap_or_else(|e| {
                eprintln!("Encryption configuration error: {e}");
                std::process::exit(1);
            });
        if command == "compact" {
            let Some(path) = value_from_args("--compact") else {
                eprintln!("Usage: thingd-server --compact <database-path>");
                std::process::exit(2);
            };
            let options = thingd::PersistentOpenOptions {
                encryption: key,
                search_mode: thingd::PersistentSearchMode::Disabled,
                backend: parse_storage_backend(&config.server.storage_backend).unwrap_or_else(
                    |e| {
                        eprintln!("ERROR: {e}");
                        std::process::exit(2);
                    },
                ),
                ..thingd::PersistentOpenOptions::default()
            };
            match thingd::PersistentEngine::open_with_options(&path, options) {
                Ok(mut engine) => match engine.compact_storage() {
                    Ok(()) => {
                        println!(
                            "OK: compacted path={} journal_bytes={} journal_count={}",
                            path.display(),
                            engine.journal_bytes(),
                            engine.journal_count()
                        );
                    },
                    Err(error) => {
                        eprintln!("ERROR: compaction failed: {error}");
                        std::process::exit(1);
                    },
                },
                Err(error) => {
                    eprintln!("ERROR: unable to open database exclusively: {error}");
                    std::process::exit(1);
                },
            }
        } else {
            let Some(source) = value_from_args("--repack") else {
                eprintln!(
                    "Usage: thingd-server --repack <source> --destination <path> [--source-backend rocksdb|thingdb]"
                );
                std::process::exit(2);
            };
            let Some(destination) = value_from_args("--destination") else {
                eprintln!(
                    "Usage: thingd-server --repack <source> --destination <path> [--source-backend rocksdb|thingdb]"
                );
                std::process::exit(2);
            };
            let source_backend = value_from_args("--source-backend")
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_else(|| "rocksdb".to_string());
            let source_backend = parse_storage_backend(&source_backend).unwrap_or_else(|error| {
                eprintln!("ERROR: {error}");
                std::process::exit(2);
            });
            let destination_backend = parse_storage_backend(&config.server.storage_backend)
                .unwrap_or_else(|error| {
                    eprintln!("ERROR: {error}");
                    std::process::exit(2);
                });
            if let Err(error) = thingd::PersistentEngine::repack_to_with_backends(
                &source,
                &destination,
                source_backend,
                destination_backend,
                key,
            ) {
                eprintln!("ERROR: repack failed: {error}");
                std::process::exit(1);
            }
            println!(
                "OK: repacked source={} destination={}",
                source.display(),
                destination.display()
            );
        }
        return;
    }

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
        engine::EnginePool::new_with_encryption_key_search_mode_journal_limit_and_recovery_budget_and_backend(
            config.server.database.clone(),
            config.server.encryption_key.as_deref(),
            match config.server.search_mode {
                config::SearchModeConfig::Persistent => {
                    thingd::PersistentSearchMode::PersistentRecovery
                },
                config::SearchModeConfig::PersistentAsync => {
                    thingd::PersistentSearchMode::PersistentAsync
                },
                config::SearchModeConfig::PersistentNoRebuild => {
                    thingd::PersistentSearchMode::PersistentNoRebuild
                },
                config::SearchModeConfig::Disabled => thingd::PersistentSearchMode::Disabled,
            },
            config.server.journal_max_bytes,
            config.server.recovery_batch_size,
            config.server.recovery_pause_ms,
            config.server.recovery_max_retries,
            config.server.recovery_memory_limit_bytes,
            config.server.search_commit_interval_ms,
            config.server.search_commit_batch_size,
            config.server.search_queue_max_keys,
            parse_storage_backend(&config.server.storage_backend).unwrap_or_else(|error| {
                eprintln!("ERROR: {error}");
                std::process::exit(2);
            }),
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

#[cfg(test)]
mod tests {
    use super::run_healthcheck;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn read_http_request(stream: &mut std::net::TcpStream) {
        let mut request = Vec::new();
        let mut byte = [0_u8; 1];
        while !request.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).expect("read healthcheck");
            request.push(byte[0]);
        }
    }

    #[test]
    fn healthcheck_accepts_success_response() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("test listener address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept healthcheck");
            read_http_request(&mut stream);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .expect("write healthcheck response");
        });

        let result = run_healthcheck(&format!("http://{address}/healthz"));
        server.join().expect("join test server");
        assert!(
            result.is_ok(),
            "expected successful healthcheck: {result:?}"
        );
    }

    #[test]
    fn healthcheck_rejects_non_success_response() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("test listener address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept healthcheck");
            read_http_request(&mut stream);
            stream
                .write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n")
                .expect("write healthcheck response");
        });

        let result = run_healthcheck(&format!("http://{address}/healthz"));
        server.join().expect("join test server");
        assert!(result.is_err(), "expected failed healthcheck");
    }

    #[test]
    fn healthcheck_requires_http_socket_url() {
        assert!(run_healthcheck("https://127.0.0.1:8757/healthz").is_err());
        assert!(run_healthcheck("http://127.0.0.1/healthz").is_err());
    }
}
