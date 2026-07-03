use std::sync::atomic::{AtomicBool, Ordering};

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

static PRODUCTION_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_production_mode(enabled: bool) {
    PRODUCTION_MODE.store(enabled, Ordering::Relaxed);
}

pub fn is_production_mode() -> bool {
    PRODUCTION_MODE.load(Ordering::Relaxed)
}

#[derive(Debug)]
pub struct AppError {
    pub status: StatusCode,
    pub title: &'static str,
    pub detail: String,
    pub error_type: &'static str,
}

impl AppError {
    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            title: "Bad Request",
            detail: detail.into(),
            error_type: "bad_request",
        }
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            title: "Not Found",
            detail: detail.into(),
            error_type: "not_found",
        }
    }

    pub fn unauthorized(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            title: "Unauthorized",
            detail: detail.into(),
            error_type: "unauthorized",
        }
    }

    pub fn internal(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            title: "Internal Server Error",
            detail: detail.into(),
            error_type: "internal_error",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let detail = if is_production_mode() && self.status == StatusCode::INTERNAL_SERVER_ERROR {
            String::new()
        } else {
            self.detail
        };
        let body = json!({
            "error": {
                "type": self.error_type,
                "title": self.title,
                "status": self.status.as_u16(),
                "detail": detail,
            }
        });
        (self.status, Json(body)).into_response()
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::bad_request(format!("Invalid JSON: {}", e))
    }
}

impl<E: std::fmt::Display> From<AppErrorWrapper<E>> for AppError {
    fn from(w: AppErrorWrapper<E>) -> Self {
        AppError::internal(w.0.to_string())
    }
}

impl From<thingd::ThingdError> for AppError {
    fn from(e: thingd::ThingdError) -> Self {
        match e {
            thingd::ThingdError::InvalidInput(msg) => AppError::bad_request(msg),
            thingd::ThingdError::NotFound(msg) => AppError::not_found(msg),
            thingd::ThingdError::Conflict(msg) => AppError {
                status: StatusCode::CONFLICT,
                title: "Conflict",
                detail: msg,
                error_type: "conflict",
            },
            thingd::ThingdError::Protected(msg) => AppError::bad_request(msg),
            thingd::ThingdError::Storage(msg) => {
                if is_production_mode() {
                    AppError::internal(String::new())
                } else {
                    AppError::internal(msg)
                }
            },
        }
    }
}

pub struct AppErrorWrapper<T: std::fmt::Display>(pub T);
