use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::{Extensions, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use parking_lot::Mutex;

use crate::error::AppError;

struct Bucket {
    tokens: u64,
    last_refill: Instant,
    last_seen: Instant,
}

const MAX_BUCKETS: usize = 10_000;

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

        // Keep memory bounded even when an attacker rotates source addresses.
        if buckets.len() >= MAX_BUCKETS {
            let cutoff = now - Duration::from_secs(120);
            buckets.retain(|_, b| b.last_refill > cutoff);

            if buckets.len() >= MAX_BUCKETS
                && let Some(oldest_key) = buckets
                    .iter()
                    .min_by_key(|(_, bucket)| bucket.last_seen)
                    .map(|(key, _)| key.clone())
            {
                buckets.remove(&oldest_key);
            }
        }

        let bucket = buckets.entry(key.to_string()).or_insert(Bucket {
            tokens: self.max_tokens,
            last_refill: now,
            last_seen: now,
        });
        bucket.last_seen = now;

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

/// Extract the client address from connection metadata, with a test/runtime fallback.
fn client_key(extensions: &Extensions) -> String {
    extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip().to_string())
        .unwrap_or_else(|| "default".to_string())
}

pub async fn rate_limit_middleware(
    State(limiter): State<Arc<RateLimiter>>,
    req: axum::extract::Request,
    next: Next,
) -> Result<Response, AppError> {
    let key = client_key(req.extensions());
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
    fn client_key_ignores_forwarded_headers_without_peer_info() {
        let extensions = Extensions::new();
        assert_eq!(client_key(&extensions), "default");
    }

    #[test]
    fn limiter_prunes_stale_buckets() {
        let limiter = RateLimiter::new(100_001);
        // Fill with many entries
        for i in 0..10_001 {
            limiter.check(&format!("client{}", i));
        }
        // The active map must never exceed its hard memory bound.
        limiter.check("trigger-prune");
        assert!(limiter.buckets.lock().len() <= MAX_BUCKETS);
    }

    #[test]
    fn client_key_uses_ip_and_ignores_source_port() {
        let mut first = Extensions::new();
        first.insert(ConnectInfo(
            "192.0.2.10:10001".parse::<SocketAddr>().unwrap(),
        ));
        let mut second = Extensions::new();
        second.insert(ConnectInfo(
            "192.0.2.10:54321".parse::<SocketAddr>().unwrap(),
        ));
        assert_eq!(client_key(&first), client_key(&second));
    }

    #[test]
    fn bucket_count_stays_bounded_for_unique_keys() {
        let limiter = RateLimiter::new(1);
        for i in 0..(MAX_BUCKETS + 100) {
            assert!(limiter.check(&format!("client-{i}")));
        }
        assert!(limiter.buckets.lock().len() <= MAX_BUCKETS);
    }
}
