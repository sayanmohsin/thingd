use crate::config::TenantConfig;
use crate::error::AppError;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, Method, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use crate::server::AppState;

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Path prefixes that are exempt from authentication.
/// Health, metrics, and cluster-status endpoints are safe to expose without auth.
const PUBLIC_PATH_PREFIXES: &[&str] = &["/healthz", "/metrics"];

fn skip_auth_for_path(path: &str) -> bool {
    PUBLIC_PATH_PREFIXES.iter().any(|p| path.starts_with(p))
}

pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    if (state.auth_token.is_empty()
        && state.tenant_config.mode != crate::config::TenantMode::MultiTenant)
        || req.method() == Method::OPTIONS
        || skip_auth_for_path(req.uri().path())
    {
        return Ok(next.run(req).await);
    }

    let provided = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    let expected_token = if state.tenant_config.mode == crate::config::TenantMode::MultiTenant {
        let tenant_id = extract_tenant_id(req.headers(), &state.tenant_config)?
            .ok_or_else(|| AppError::unauthorized("Tenant identity is required"))?;
        state
            .tenant_tokens
            .get(&tenant_id)
            .ok_or_else(|| AppError::unauthorized("Tenant is not authorized"))?
    } else {
        &state.auth_token
    };

    match provided {
        Some(p) if constant_time_eq(&p, expected_token) => {},
        _ if state.allow_unauthenticated => {
            tracing::warn!("No valid auth token, but allow_unauthenticated is set");
        },
        _ => return Err(AppError::unauthorized("Missing or invalid Bearer token")),
    }

    Ok(next.run(req).await)
}

pub fn extract_tenant_id(
    headers: &HeaderMap,
    config: &TenantConfig,
) -> Result<Option<String>, AppError> {
    if config.mode != crate::config::TenantMode::MultiTenant {
        return Ok(None);
    }

    let header_value = headers
        .get(&config.header)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string());

    match header_value {
        Some(tid) if tid.is_empty() => Err(AppError::bad_request("X-Tenant-Id header is empty")),
        Some(tid) => {
            if tid.contains("..") || tid.contains('/') {
                return Err(AppError::bad_request(
                    "Invalid tenant ID: path traversal rejected",
                ));
            }
            if !tid
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            {
                return Err(AppError::bad_request(
                    "Invalid tenant ID format: only alphanumeric, hyphens, and underscores allowed",
                ));
            }
            Ok(Some(tid))
        },
        None => Err(AppError::bad_request(
            "X-Tenant-Id header is required in multi-tenant mode",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_matching_strings() {
        assert!(constant_time_eq("hello", "hello"));
    }

    #[test]
    fn constant_time_eq_different_strings() {
        assert!(!constant_time_eq("hello", "world"));
    }

    #[test]
    fn constant_time_eq_different_lengths() {
        assert!(!constant_time_eq("hi", "hello"));
    }

    #[test]
    fn constant_time_eq_empty_strings() {
        assert!(constant_time_eq("", ""));
    }

    #[test]
    fn constant_time_eq_empty_vs_nonempty() {
        assert!(!constant_time_eq("", "a"));
    }

    #[test]
    fn constant_time_eq_single_char() {
        assert!(constant_time_eq("a", "a"));
        assert!(!constant_time_eq("a", "b"));
    }

    #[test]
    fn constant_time_eq_unicode() {
        assert!(constant_time_eq("héllo", "héllo"));
        assert!(!constant_time_eq("héllo", "hello"));
    }
}
