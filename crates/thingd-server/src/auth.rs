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
const PUBLIC_PATH_PREFIXES: &[&str] = &["/healthz", "/ready", "/metrics"];

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
    use crate::config::{AuthConfig, AuthMode, Config, TenantMode};
    use crate::engine::EnginePool;
    use crate::server::AppState;
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        middleware as axum_middleware,
        response::Json,
        routing::get,
    };
    use http_body_util::BodyExt;
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::task::JoinHandle;
    use tower::ServiceExt;

    const ISSUER: &str = "https://cloud.example";
    const AUDIENCE: &str = "thingd-runtime";
    const KEY_ID: &str = "test-key";
    const PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDQ0P1whgfegZ0m\n5jtsDdd+PcHj3UIvNX8FBM314LvbyAnN7qs+MSPo4RJ9IisJQQxmACHV3M9R2zxx\nG2fdH4mF3ZGpifcj/AV6DbtoWtt6OM8eAf0y36QdBaRrn2lrQHMdqcXBxx5tT9/H\n8nIBBdBM4SBNbTc6qaXRsOPdJjtiXj/vGQjCBhNWUf3iR7nVflnT/MrMmIs9zLWd\nGwk2IDSGXa0k8Z8HSIBWWOnTJ6bMPMDVPzEv9Ok4xm0v+2+yyN3z4HoPWgrZEJ4E\n1pHelqoV5e1P7LsdM2vGsiIgMm2z2G9lWf5DWKbgtRSK9Dneg/dwhHwfKyrw0ydg\nTvTdyshBAgMBAAECggEABoilh3fwJtiOJOWTEilYNu9igdleXOkK+4qYEbOZmTHI\nCcEoK0rhFWQ2foMbTt+xzj6+kEknlQd7u4vImxEFrfI+AKysFpyIBNbJaIVgRPGi\nXG1iQW1hPpW6vHHZW/VhLLpPUWYEOBYl2cUmp0zJ5N/t8XJDhkLA6R/kb1rnBIfB\nKHzzH1dPT/p6wf+2JaI5n2v48HO3QlVNEx5U/0YbRK+B5HVCYgfbhk3H0kwOyHH1\nm0/MNlz8vOe6yjn0zQswMv/sVwFITlnRGknsB4lPAPS6HAR/AbBpYFIwBCBJquXz\nCfP7md+WMQAjRM+HH6Up+zVeX5DBnU+FtK6Vz3P64QKBgQD8pRR+N2rk/69S14Ei\nE1dfqLby4CA6eb22fAoHLezazOfA5i/ldA6mejvZmuzCTRN3VfK4ZPVQXh7d6P7n\ns2nXqaFcn0Chi/PmFgUhakI1wOOJQLhH8OAfQDZIVGucI+CNjKYolqXGWeofVqPY\ngcFOOWs77eTR5todBN7Ga98l0QKBgQDTlufcam/noJ0NUzfN8VOXUwHCtdlGMfUC\nmoAl3Q4wkZ4sisPt8MN272j3Teuj8EBsPziOjrT+GOAudCnUQllkXVzsylvhiWUB\nszwzPb+L6M6C/Tr4QrqQjLfDWRq4ojIovJBflAz05VFoeCSh0hL20bdRtaLE9Gu8\nMoxJd9RncQKBgQDhDAOnMqIrfn6kIodK3UO1WEovKupKbGtLhE5CeuxDMsc2E1WS\n5MCwFq39dn1zzsiKQqtFCdljT5PbRFLb/ftIOjgck1c1D7+gsvi6/TYhP8LvXhFJ\nNA3QiJhR8bExktvR+vl/qkHc3/cnFzw3/c09avRUm+J5/1NHCjGPOkO6IQKBgQCP\nW3geo9LL+ctOwupVU0OSjH/t322lnKVnLSzT+pDpoU+s2Bvls7GLfKv+msGj4lyT\nusXj+JZyboI9lyDcGlQcpxEVsglMpt1TqI+KHDUSYxrALzhsCjIDTAQZi96J6ALa\nDOA4kcOxjUl54aTYKtAEgJSW/NyaWww/h3P7NwAnUQKBgAVpQAJAVGMjpXSLX/aR\n+9cAon3nqnCJzslhRYjHEMrt6+z6QakDrv3Bxjg6JtdW8D5L8Unui6eAUrCiS7pU\nq/F3m3QFTdhywowlimJXUcDk+CPZki+Ad2aSLVF06qOFVeK1Dqftfsk14Aj91ElA\nfrPFk++0AP1Rg+hdynPpsjoi\n-----END PRIVATE KEY-----";
    const PUBLIC_MODULUS: &str = "0ND9cIYH3oGdJuY7bA3Xfj3B491CLzV_BQTN9eC728gJze6rPjEj6OESfSIrCUEMZgAh1dzPUds8cRtn3R-Jhd2RqYn3I_wFeg27aFrbejjPHgH9Mt-kHQWka59pa0BzHanFwccebU_fx_JyAQXQTOEgTW03Oqml0bDj3SY7Yl4_7xkIwgYTVlH94ke51X5Z0_zKzJiLPcy1nRsJNiA0hl2tJPGfB0iAVljp0yemzDzA1T8xL_TpOMZtL_tvssjd8-B6D1oK2RCeBNaR3paqFeXtT-y7HTNrxrIiIDJts9hvZVn-Q1im4LUUivQ53oP3cIR8Hysq8NMnYE703crIQQ";

    async fn spawn_jwks_server() -> (String, JoinHandle<()>) {
        let app = Router::new().route(
            "/.well-known/jwks.json",
            get(|| async {
                Json(json!({
                    "keys": [{
                        "kid": KEY_ID,
                        "kty": "RSA",
                        "n": PUBLIC_MODULUS,
                        "e": "AQAB"
                    }]
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind JWKS test server");
        let address = listener.local_addr().expect("JWKS test server address");
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve JWKS test server");
        });
        (format!("http://{address}/.well-known/jwks.json"), task)
    }

    fn auth_config(jwks_url: String) -> AuthConfig {
        AuthConfig {
            mode: AuthMode::TenantJwt,
            jwks_url,
            issuer: ISSUER.to_string(),
            audience: AUDIENCE.to_string(),
            tenant_claim: "tenant_id".to_string(),
            ..AuthConfig::default()
        }
    }

    fn claims(tenant_id: Option<&str>, issuer: &str, audience: &str, exp: i64) -> Value {
        let mut claims = json!({
            "iss": issuer,
            "aud": audience,
            "exp": exp,
        });
        if let Some(tenant_id) = tenant_id {
            claims["tenant_id"] = json!(tenant_id);
        }
        claims
    }

    fn token(claims: Value, kid: &str) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.to_string());
        encode(
            &header,
            &claims,
            &EncodingKey::from_rsa_pem(PRIVATE_KEY.as_bytes()).expect("test private key"),
        )
        .expect("encode test JWT")
    }

    fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_secs() as i64
    }

    fn test_state(auth: AuthConfig) -> Arc<AppState> {
        let mut config = Config::default();
        config.tenant.mode = TenantMode::MultiTenant;
        config.auth = auth;
        Arc::new(AppState {
            pool: Arc::new(EnginePool::new(":memory:".to_string())),
            tenant_config: config.tenant,
            mcp_config: config.mcp,
            auth_token: config.auth.token.clone(),
            tenant_tokens: config.auth.tenant_tokens.clone(),
            auth_verifier: Some(Arc::new(
                AuthVerifier::new(&config.auth).expect("test auth verifier"),
            )),
            allow_unauthenticated: config.auth.allow_unauthenticated,
            cluster_config: config.cluster,
            nlq_config: config.nlq,
            sync_config: config.sync,
            hardening_config: config.hardening,
        })
    }

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

    #[tokio::test]
    async fn tenant_jwt_verification_accepts_rs256_and_rejects_invalid_claims() {
        let (jwks_url, server) = spawn_jwks_server().await;
        let verifier = AuthVerifier::new(&auth_config(jwks_url)).expect("create verifier");

        let valid = token(
            claims(Some("tenant-a"), ISSUER, AUDIENCE, now() + 300),
            KEY_ID,
        );
        assert_eq!(
            verifier.tenant_id(&valid).await.expect("valid JWT"),
            "tenant-a"
        );

        let wrong_issuer = token(
            claims(
                Some("tenant-a"),
                "https://wrong.example",
                AUDIENCE,
                now() + 300,
            ),
            KEY_ID,
        );
        assert!(verifier.tenant_id(&wrong_issuer).await.is_err());

        let wrong_audience = token(
            claims(Some("tenant-a"), ISSUER, "wrong-audience", now() + 300),
            KEY_ID,
        );
        assert!(verifier.tenant_id(&wrong_audience).await.is_err());

        let expired = token(
            claims(Some("tenant-a"), ISSUER, AUDIENCE, now() - 120),
            KEY_ID,
        );
        assert!(verifier.tenant_id(&expired).await.is_err());

        let missing_tenant = token(claims(None, ISSUER, AUDIENCE, now() + 300), KEY_ID);
        assert!(verifier.tenant_id(&missing_tenant).await.is_err());

        let invalid_tenant = token(
            claims(Some("tenant/a"), ISSUER, AUDIENCE, now() + 300),
            KEY_ID,
        );
        assert!(verifier.tenant_id(&invalid_tenant).await.is_err());

        let unknown_key = token(
            claims(Some("tenant-a"), ISSUER, AUDIENCE, now() + 300),
            "unknown-key",
        );
        assert!(verifier.tenant_id(&unknown_key).await.is_err());

        server.abort();
    }

    #[tokio::test]
    async fn tenant_jwt_verification_rejects_unsupported_algorithm() {
        let (jwks_url, server) = spawn_jwks_server().await;
        let verifier = AuthVerifier::new(&auth_config(jwks_url)).expect("create verifier");
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims(Some("tenant-a"), ISSUER, AUDIENCE, now() + 300),
            &EncodingKey::from_secret(b"not-an-rsa-key"),
        )
        .expect("encode unsupported JWT");

        assert!(verifier.tenant_id(&token).await.is_err());
        server.abort();
    }

    #[tokio::test]
    async fn tenant_jwt_middleware_derives_and_enforces_tenant_header() {
        let (jwks_url, server) = spawn_jwks_server().await;
        let state = test_state(auth_config(jwks_url));
        let app = Router::new()
            .route(
                "/tenant",
                get(|headers: HeaderMap| async move {
                    (
                        StatusCode::OK,
                        headers
                            .get("x-tenant-id")
                            .expect("derived tenant header")
                            .to_str()
                            .expect("tenant header value")
                            .to_string(),
                    )
                }),
            )
            .layer(axum_middleware::from_fn_with_state(state, auth_middleware));
        let jwt = token(
            claims(Some("tenant-a"), ISSUER, AUDIENCE, now() + 300),
            KEY_ID,
        );

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tenant")
                    .header("authorization", format!("Bearer {jwt}"))
                    .body(Body::empty())
                    .expect("valid request"),
            )
            .await
            .expect("middleware response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("response body")
            .to_bytes();
        assert_eq!(&body[..], b"tenant-a");

        let mismatched = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tenant")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("x-tenant-id", "tenant-b")
                    .body(Body::empty())
                    .expect("mismatched request"),
            )
            .await
            .expect("mismatch response");
        assert_eq!(mismatched.status(), StatusCode::UNAUTHORIZED);

        let missing = app
            .oneshot(
                Request::builder()
                    .uri("/tenant")
                    .body(Body::empty())
                    .expect("missing token request"),
            )
            .await
            .expect("missing token response");
        assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

        server.abort();
    }
}
