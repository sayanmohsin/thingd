use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;

use parking_lot::Mutex;

use crate::error::AppError;

struct Bucket {
    tokens: u64,
    last_refill: Instant,
}

/// A simple token-bucket rate limiter keyed by client IP.
pub struct RateLimiter {
    max_tokens: u64,
    refill_interval: Duration,
    buckets: Mutex<HashMap<SocketAddr, Bucket>>,
}

impl RateLimiter {
    pub fn new(rpm: u64) -> Self {
        Self {
            max_tokens: rpm,
            refill_interval: Duration::from_secs(60),
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Returns `true` if the request is allowed, `false` if rate-limited.
    pub fn check(&self, addr: SocketAddr) -> bool {
        let mut buckets = self.buckets.lock();
        let now = Instant::now();
        let bucket = buckets.entry(addr).or_insert(Bucket {
            tokens: self.max_tokens,
            last_refill: now,
        });

        // Refill tokens based on elapsed time
        let elapsed = now.duration_since(bucket.last_refill);
        let tokens_to_add =
            (elapsed.as_secs_f64() / self.refill_interval.as_secs_f64()) * self.max_tokens as f64;
        if tokens_to_add >= 1.0 {
            bucket.tokens = (bucket.tokens + tokens_to_add as u64).min(self.max_tokens);
            bucket.last_refill = now;
        }

        if bucket.tokens > 0 {
            bucket.tokens -= 1;
            true
        } else {
            false
        }
    }
}

pub async fn rate_limit_middleware(
    State(limiter): State<Arc<RateLimiter>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: axum::extract::Request,
    next: Next,
) -> Result<Response, AppError> {
    if limiter.check(addr) {
        Ok(next.run(req).await)
    } else {
        Err(AppError {
            status: StatusCode::TOO_MANY_REQUESTS,
            title: "Too Many Requests",
            detail: "Rate limit exceeded. Try again later.".to_string(),
            error_type: "too_many_requests",
        })
    }
}
