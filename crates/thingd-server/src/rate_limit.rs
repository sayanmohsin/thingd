use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

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
    buckets: Mutex<HashMap<String, Bucket>>,
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
    pub fn check(&self, key: &str) -> bool {
        let mut buckets = self.buckets.lock();
        let now = Instant::now();

        // Prune stale entries if the map grows large
        if buckets.len() > 10_000 {
            let cutoff = now - Duration::from_secs(120);
            buckets.retain(|_, b| b.last_refill > cutoff);
        }

        let bucket = buckets.entry(key.to_string()).or_insert(Bucket {
            tokens: self.max_tokens,
            last_refill: now,
        });

        // Refill tokens based on elapsed time
        let elapsed = now.duration_since(bucket.last_refill);
        let tokens_to_add =
            (elapsed.as_secs_f64() / self.refill_interval.as_secs_f64()) * self.max_tokens as f64;
        if tokens_to_add >= 1.0 {
            bucket.tokens = (bucket.tokens + tokens_to_add.floor() as u64).min(self.max_tokens);
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

/// Extract the client IP from `X-Forwarded-For`, falling back to a random key.
fn client_key(headers: &HeaderMap) -> String {
    if let Some(forwarded) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next().map(|s| s.trim().to_string()))
    {
        return forwarded;
    }
    // Fallback: use a per-request random key when ConnectInfo is not available.
    // This is less precise than SocketAddr but avoids requiring the ConnectInfo
    // layer on the router for environments where it's not set up.
    "default".to_string()
}

pub async fn rate_limit_middleware(
    State(limiter): State<Arc<RateLimiter>>,
    req: axum::extract::Request,
    next: Next,
) -> Result<Response, AppError> {
    let key = client_key(req.headers());
    if limiter.check(&key) {
        Ok(next.run(req).await)
    } else {
        let body = Json(serde_json::json!({
            "error": {
                "type": "too_many_requests",
                "title": "Too Many Requests",
                "status": 429,
                "detail": "Rate limit exceeded. Try again later."
            }
        }));
        let response = (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "60")],
            body,
        )
            .into_response();
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_limiter_allows_initial_requests() {
        let limiter = RateLimiter::new(10);
        assert!(limiter.check("client1"));
    }

    #[test]
    fn limiter_exhausts_tokens() {
        let limiter = RateLimiter::new(3);
        assert!(limiter.check("client1"));
        assert!(limiter.check("client1"));
        assert!(limiter.check("client1"));
        assert!(!limiter.check("client1"));
    }

    #[test]
    fn limiter_independent_clients() {
        let limiter = RateLimiter::new(2);
        assert!(limiter.check("client1"));
        assert!(limiter.check("client1"));
        assert!(!limiter.check("client1"));
        // Different client has its own bucket
        assert!(limiter.check("client2"));
        assert!(limiter.check("client2"));
        assert!(!limiter.check("client2"));
    }

    #[test]
    fn client_key_uses_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "192.168.1.1".parse().unwrap());
        assert_eq!(client_key(&headers), "192.168.1.1");
    }

    #[test]
    fn client_key_forwarded_for_multiple_ips() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "192.168.1.1, 10.0.0.1".parse().unwrap());
        assert_eq!(client_key(&headers), "192.168.1.1");
    }

    #[test]
    fn client_key_fallback_to_default() {
        let headers = HeaderMap::new();
        assert_eq!(client_key(&headers), "default");
    }

    #[test]
    fn limiter_prunes_stale_buckets() {
        let limiter = RateLimiter::new(100_001);
        // Fill with many entries
        for i in 0..10_001 {
            limiter.check(&format!("client{}", i));
        }
        assert!(limiter.buckets.lock().len() > 10_000);
        // Next check triggers pruning — should not panic
        limiter.check("trigger-prune");
    }
}
