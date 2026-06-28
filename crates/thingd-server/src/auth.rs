use crate::error::AppError;
use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
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

pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    if state.auth_token.is_empty() {
        return Ok(next.run(req).await);
    }

    let provided = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    match provided {
        Some(p) if constant_time_eq(&p, &state.auth_token) => {},
        _ if state.allow_unauthenticated => {
            tracing::warn!("No valid auth token, but allow_unauthenticated is set");
        },
        _ => return Err(AppError::unauthorized("Missing or invalid Bearer token")),
    }

    Ok(next.run(req).await)
}
