use crate::config::{AuthConfig, AuthMode, TenantConfig};
use crate::error::AppError;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, Method, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

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

#[derive(Clone)]
pub struct AuthVerifier {
    client: reqwest::Client,
    jwks_url: String,
    issuer: String,
    audience: String,
    tenant_claim: String,
    cache_ttl: Duration,
    cache: Arc<std::sync::RwLock<KeyCache>>,
}

struct KeyCache {
    fetched_at: Option<Instant>,
    keys: HashMap<String, Arc<DecodingKey>>,
}

#[derive(Debug, Deserialize)]
struct Jwks {
    keys: Vec<JsonWebKey>,
}

#[derive(Debug, Deserialize)]
struct JsonWebKey {
    kid: String,
    kty: String,
    n: Option<String>,
    e: Option<String>,
}

impl AuthVerifier {
    pub fn new(config: &AuthConfig) -> Result<Self, String> {
        if config.mode != AuthMode::TenantJwt {
            return Err("JWT verifier requires tenant-jwt auth mode".into());
        }
        if config.jwks_url.is_empty() || config.issuer.is_empty() || config.audience.is_empty() {
            return Err("tenant-jwt auth requires JWKS URL, issuer, and audience".into());
        }
        Ok(Self {
            client: reqwest::Client::new(),
            jwks_url: config.jwks_url.clone(),
            issuer: config.issuer.clone(),
            audience: config.audience.clone(),
            tenant_claim: config.tenant_claim.clone(),
            cache_ttl: Duration::from_secs(config.jwks_cache_secs.max(1)),
            cache: Arc::new(std::sync::RwLock::new(KeyCache {
                fetched_at: None,
                keys: HashMap::new(),
            })),
        })
    }

    async fn fetch_keys(&self) -> Result<(), AppError> {
        let body = self
            .client
            .get(&self.jwks_url)
            .send()
            .await
            .map_err(|_| AppError::unauthorized("Unable to load runtime signing keys"))?
            .error_for_status()
            .map_err(|_| AppError::unauthorized("Unable to load runtime signing keys"))?
            .json::<Jwks>()
            .await
            .map_err(|_| AppError::unauthorized("Invalid runtime signing-key response"))?;

        let mut keys = HashMap::new();
        for jwk in body.keys {
            if jwk.kty != "RSA" {
                continue;
            }
            let (Some(n), Some(e)) = (jwk.n, jwk.e) else {
                continue;
            };
            let key = DecodingKey::from_rsa_components(&n, &e)
                .map_err(|_| AppError::unauthorized("Invalid runtime signing key"))?;
            keys.insert(jwk.kid, Arc::new(key));
        }
        if keys.is_empty() {
            return Err(AppError::unauthorized(
                "Runtime signing keys are unavailable",
            ));
        }

        let mut cache = self
            .cache
            .write()
            .map_err(|_| AppError::unauthorized("Runtime signing-key cache unavailable"))?;
        cache.keys = keys;
        cache.fetched_at = Some(Instant::now());
        Ok(())
    }

    async fn key_for(&self, kid: &str) -> Result<Arc<DecodingKey>, AppError> {
        let fresh = self
            .cache
            .read()
            .ok()
            .and_then(|cache| cache.fetched_at)
            .is_some_and(|fetched| fetched.elapsed() < self.cache_ttl);
        if !fresh {
            // Preserve a usable cached key during temporary JWKS outages.
            let has_key = self
                .cache
                .read()
                .ok()
                .is_some_and(|cache| cache.keys.contains_key(kid));
            if !has_key {
                self.fetch_keys().await?;
            }
        }

        let key = self
            .cache
            .read()
            .ok()
            .and_then(|cache| cache.keys.get(kid).cloned());
        if let Some(key) = key {
            return Ok(key);
        }

        // A new signing key may appear before the normal cache TTL expires.
        self.fetch_keys().await?;
        self.cache
            .read()
            .ok()
            .and_then(|cache| cache.keys.get(kid).cloned())
            .ok_or_else(|| AppError::unauthorized("Unknown runtime signing key"))
    }

    pub async fn tenant_id(&self, token: &str) -> Result<String, AppError> {
        let header =
            decode_header(token).map_err(|_| AppError::unauthorized("Invalid runtime token"))?;
        if header.alg != Algorithm::RS256 {
            return Err(AppError::unauthorized(
                "Unsupported runtime token algorithm",
            ));
        }
        let kid = header
            .kid
            .ok_or_else(|| AppError::unauthorized("Runtime token has no key ID"))?;
        let key = self.key_for(&kid).await?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(std::slice::from_ref(&self.issuer));
        validation.set_audience(std::slice::from_ref(&self.audience));
        let data = decode::<Value>(token, &key, &validation)
            .map_err(|_| AppError::unauthorized("Invalid runtime token"))?;
        let tenant_id = data
            .claims
            .get(&self.tenant_claim)
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::unauthorized("Runtime token has no tenant identity"))?;
        validate_tenant_id(tenant_id)
    }
}

pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
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

    if state.tenant_config.mode == crate::config::TenantMode::MultiTenant
        && let Some(verifier) = state.auth_verifier.as_ref()
    {
        let token = provided
            .as_deref()
            .ok_or_else(|| AppError::unauthorized("Missing or invalid Bearer token"))?;
        let tenant_id = verifier.tenant_id(token).await?;
        if let Some(header_tenant) = req
            .headers()
            .get(&state.tenant_config.header)
            .and_then(|value| value.to_str().ok())
            && header_tenant.trim() != tenant_id
        {
            return Err(AppError::unauthorized(
                "Tenant identity does not match token",
            ));
        }
        let header_name: HeaderName = state
            .tenant_config
            .header
            .parse()
            .map_err(|_| AppError::unauthorized("Invalid tenant header configuration"))?;
        let header_value: HeaderValue = tenant_id
            .parse()
            .map_err(|_| AppError::unauthorized("Invalid tenant identity"))?;
        req.headers_mut().insert(header_name, header_value);
    } else {
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
        Some(tid) => validate_tenant_id(&tid).map(Some),
        None => Err(AppError::bad_request(
            "X-Tenant-Id header is required in multi-tenant mode",
        )),
    }
}

fn validate_tenant_id(tenant_id: &str) -> Result<String, AppError> {
    if tenant_id.is_empty() || tenant_id.contains("..") || tenant_id.contains('/') {
        return Err(AppError::unauthorized("Invalid tenant identity"));
    }
    if !tenant_id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::unauthorized("Invalid tenant identity"));
    }
    Ok(tenant_id.to_string())
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

    #[test]
    fn tenant_id_rejects_path_traversal_and_invalid_characters() {
        assert!(validate_tenant_id("tenant-a").is_ok());
        assert!(validate_tenant_id("../tenant-a").is_err());
        assert!(validate_tenant_id("tenant/a").is_err());
        assert!(validate_tenant_id("tenant a").is_err());
    }
}
