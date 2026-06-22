use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug)]
pub struct AppError {
    pub status: StatusCode,
    pub title: &'static str,
    pub detail: String,
    pub error_type: &'static str,
}

#[allow(dead_code)]
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

    pub fn forbidden(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            title: "Forbidden",
            detail: detail.into(),
            error_type: "forbidden",
        }
    }

    pub fn too_large(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            title: "Payload Too Large",
            detail: detail.into(),
            error_type: "payload_too_large",
        }
    }

    pub fn too_many_requests(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            title: "Too Many Requests",
            detail: detail.into(),
            error_type: "too_many_requests",
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
        let body = json!({
            "error": {
                "type": self.error_type,
                "title": self.title,
                "status": self.status.as_u16(),
                "detail": self.detail,
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

pub struct AppErrorWrapper<T: std::fmt::Display>(pub T);
