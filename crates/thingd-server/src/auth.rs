use crate::error::AppError;
use axum::{extract::Request, http::header::AUTHORIZATION, middleware::Next, response::Response};

pub async fn auth_middleware(req: Request, next: Next) -> Result<Response, AppError> {
    let token = std::env::var("THINGD_AUTH_TOKEN")
        .ok()
        .filter(|t| !t.is_empty());

    let allow_unauthenticated = std::env::var("THINGD_ALLOW_UNAUTHENTICATED")
        .map(|v| v == "true")
        .unwrap_or(false);

    if let Some(ref expected) = token {
        let provided = req
            .headers()
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|s| s.to_string());

        match provided {
            Some(p) if p == *expected => {},
            _ if allow_unauthenticated => {
                tracing::warn!("No valid auth token, but allow_unauthenticated is set");
            },
            _ => return Err(AppError::unauthorized("Missing or invalid Bearer token")),
        }
    }

    Ok(next.run(req).await)
}
