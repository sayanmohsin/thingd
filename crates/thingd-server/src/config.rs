use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub tenant: TenantConfig,
    #[serde(default)]
    pub mcp: McpConfig,
    #[serde(default)]
    pub rest: RestConfig,
    #[serde(default)]
    pub cluster: ClusterConfig,
    #[serde(default)]
    pub hardening: HardeningConfig,
    #[serde(default)]
    pub nlq: NlqConfig,
    #[serde(default)]
    pub sync: SyncConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncConfig {
    /// Stable identifier for this Thingd instance's replication source.
    #[serde(default = "default_sync_source_id")]
    pub source_id: String,
    /// Whether this instance accepts normal writes or is a replica.
    #[serde(default)]
    pub role: SyncRole,
    /// Optional collection allowlist for replication changes.
    #[serde(default)]
    pub collections: Vec<String>,
    /// Deployment/provider label used for safe operator decisions. This has
    /// no effect on the provider-neutral protocol.
    #[serde(default = "default_sync_provider")]
    pub provider: String,
    /// Optional cloud project identity used to prevent cross-project routing.
    #[serde(default)]
    pub project_id: String,
    /// Optional cloud instance identity used to prevent default-instance fallbacks.
    #[serde(default)]
    pub instance_slug: String,
    /// Explicit opt-in for writes into a protected cloud target.
    #[serde(default)]
    pub allow_cloud_target: bool,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            source_id: default_sync_source_id(),
            role: SyncRole::Source,
            collections: Vec::new(),
            provider: default_sync_provider(),
            project_id: String::new(),
            instance_slug: String::new(),
            allow_cloud_target: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SyncRole {
    #[default]
    Source,
    Replica,
}

fn default_sync_source_id() -> String {
    std::env::var("THINGD_SYNC_SOURCE_ID").unwrap_or_else(|_| "thingd-default".to_string())
}

fn default_sync_provider() -> String {
    std::env::var("THINGD_SYNC_PROVIDER").unwrap_or_else(|_| "self-hosted".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_db")]
    pub database: String,
    #[serde(default = "default_request_timeout")]
    pub request_timeout_secs: u64,
    #[serde(default)]
    pub production_mode: bool,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    /// Optional 64-character hexadecimal persistent encryption key.
    #[serde(default)]
    pub encryption_key: Option<String>,
    /// Search index mode. `disabled` avoids Tantivy memory usage in embedded deployments.
    #[serde(default)]
    pub search_mode: SearchModeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum SearchModeConfig {
    #[default]
    #[serde(rename = "persistent")]
    Persistent,
    #[serde(rename = "persistent-no-rebuild")]
    PersistentNoRebuild,
    #[serde(rename = "disabled")]
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub struct AuthConfig {
    #[serde(default)]
    pub mode: AuthMode,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub allow_unauthenticated: bool,
    #[serde(default)]
    pub tenant_tokens: HashMap<String, String>,
    #[serde(default)]
    pub jwks_url: String,
    #[serde(default)]
    pub issuer: String,
    #[serde(default)]
    pub audience: String,
    #[serde(default = "default_auth_tenant_claim")]
    pub tenant_claim: String,
    #[serde(default = "default_auth_jwks_cache_secs")]
    pub jwks_cache_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum AuthMode {
    #[default]
    #[serde(rename = "bearer")]
    Bearer,
    #[serde(rename = "tenant-jwt")]
    TenantJwt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TenantConfig {
    #[serde(default = "default_tenant_mode")]
    pub mode: TenantMode,
    #[serde(default = "default_tenant_header")]
    pub header: String,
    #[serde(default = "default_tenant_db_prefix")]
    pub database_prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum TenantMode {
    #[serde(rename = "single")]
    #[default]
    Single,
    #[serde(rename = "multi-tenant")]
    MultiTenant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpConfig {
    #[serde(default = "default_mcp_path")]
    pub path: String,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default = "default_payload_limit")]
    pub max_payload_bytes: usize,
    #[serde(default)]
    pub collection_allowlist: Vec<String>,
    #[serde(default = "default_true")]
    pub audit: bool,
    #[serde(default = "default_mcp_audit_actor")]
    pub audit_actor: String,
    #[serde(default = "default_mcp_audit_source")]
    pub audit_source: String,
    #[serde(default = "default_mcp_audit_stream")]
    pub audit_stream: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub struct NlqConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_nlq_model")]
    pub model: String,
    #[serde(default = "default_nlq_endpoint")]
    pub endpoint: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_nlq_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_nlq_sample_size")]
    pub sample_size: usize,
    #[serde(default)]
    pub format_result: bool,
}

fn default_nlq_model() -> String {
    "llama3".to_string()
}
fn default_nlq_endpoint() -> String {
    "http://localhost:11434/v1".to_string()
}
fn default_nlq_max_tokens() -> u32 {
    1024
}
fn default_nlq_sample_size() -> usize {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ClusterMode {
    #[serde(rename = "single")]
    #[default]
    Single,
    #[serde(rename = "leader")]
    Leader,
    #[serde(rename = "follower")]
    Follower,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterConfig {
    #[serde(default = "default_cluster_mode")]
    pub mode: ClusterMode,
    #[serde(default)]
    pub advertise_url: String,
    #[serde(default)]
    pub leader_url: String,
    #[serde(default)]
    pub fallback_leader_url: String,
    #[serde(default)]
    pub peers: Vec<String>,
    #[serde(default = "default_discovery")]
    pub discovery: String,
    #[serde(default)]
    pub forward_auth_token: String,
    #[serde(default)]
    pub leader_election: bool,
    #[serde(default = "default_election_failures")]
    pub election_max_failures: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HardeningConfig {
    #[serde(default = "default_payload_limit")]
    pub max_payload_bytes: usize,
    #[serde(default = "default_cors_origins")]
    pub cors_allowed_origins: Vec<String>,
    #[serde(default = "default_cors_max_age")]
    pub cors_max_age_secs: u64,
    #[serde(default = "default_rate_limit_enabled")]
    pub rate_limit_enabled: bool,
    #[serde(default = "default_rate_limit_rpm")]
    pub rate_limit_requests_per_minute: u64,
    #[serde(default = "default_connector_file_bytes")]
    pub max_connector_file_bytes: u64,
    #[serde(default)]
    pub connector_file_root: Option<String>,
    #[serde(default)]
    pub connector_allowed_hosts: Vec<String>,
    #[serde(default = "default_connector_require_tls")]
    pub connector_require_tls: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            database: default_db(),
            request_timeout_secs: default_request_timeout(),
            max_connections: default_max_connections(),
            production_mode: false,
            encryption_key: None,
            search_mode: SearchModeConfig::Persistent,
        }
    }
}

impl TenantConfig {
    pub fn resolve_db_path(&self, tenant_id: Option<&str>) -> String {
        match self.mode {
            TenantMode::Single => String::new(),
            TenantMode::MultiTenant => {
                if let Some(tid) = tenant_id {
                    format!("{}{}/thingd.db", self.database_prefix, tid)
                } else {
                    String::new()
                }
            },
        }
    }
}

impl Default for TenantConfig {
    fn default() -> Self {
        Self {
            mode: default_tenant_mode(),
            header: default_tenant_header(),
            database_prefix: default_tenant_db_prefix(),
        }
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            path: default_mcp_path(),
            read_only: false,
            max_payload_bytes: default_payload_limit(),
            collection_allowlist: Vec::new(),
            audit: true,
            audit_actor: default_mcp_audit_actor(),
            audit_source: default_mcp_audit_source(),
            audit_stream: default_mcp_audit_stream(),
        }
    }
}

impl Default for RestConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            mode: ClusterMode::Single,
            advertise_url: String::new(),
            leader_url: String::new(),
            fallback_leader_url: String::new(),
            peers: Vec::new(),
            discovery: default_discovery(),
            forward_auth_token: String::new(),
            leader_election: false,
            election_max_failures: default_election_failures(),
        }
    }
}

impl Default for HardeningConfig {
    fn default() -> Self {
        Self {
            max_payload_bytes: default_payload_limit(),
            cors_allowed_origins: default_cors_origins(),
            cors_max_age_secs: default_cors_max_age(),
            rate_limit_enabled: default_rate_limit_enabled(),
            rate_limit_requests_per_minute: default_rate_limit_rpm(),
            max_connector_file_bytes: default_connector_file_bytes(),
            connector_file_root: None,
            connector_allowed_hosts: Vec::new(),
            connector_require_tls: default_connector_require_tls(),
        }
    }
}

fn default_host() -> String {
    "127.0.0.1".into()
}
fn default_port() -> u16 {
    8757
}
fn default_db() -> String {
    "/data/thingd.db".into()
}
fn default_request_timeout() -> u64 {
    30
}
fn default_max_connections() -> u32 {
    256
}
fn default_tenant_mode() -> TenantMode {
    TenantMode::Single
}
fn default_tenant_header() -> String {
    "X-Tenant-Id".into()
}
fn default_tenant_db_prefix() -> String {
    "/data/".into()
}

fn default_auth_tenant_claim() -> String {
    "tenant_id".into()
}

fn default_auth_jwks_cache_secs() -> u64 {
    300
}
fn default_mcp_path() -> String {
    "/mcp".into()
}
fn default_payload_limit() -> usize {
    524_288
}
fn default_true() -> bool {
    true
}
fn default_mcp_audit_actor() -> String {
    "mcp-client".into()
}
fn default_mcp_audit_source() -> String {
    "thingd-mcp".into()
}
fn default_mcp_audit_stream() -> String {
    "__thingd:mcp:audit".into()
}
fn default_cluster_mode() -> ClusterMode {
    ClusterMode::Single
}
fn default_discovery() -> String {
    "static".into()
}
fn default_election_failures() -> u32 {
    3
}
fn default_cors_origins() -> Vec<String> {
    vec![]
}
fn default_cors_max_age() -> u64 {
    86400
}
fn default_rate_limit_enabled() -> bool {
    true
}
fn default_rate_limit_rpm() -> u64 {
    300
}
fn default_connector_file_bytes() -> u64 {
    64 * 1024 * 1024
}
fn default_connector_require_tls() -> bool {
    true
}

impl Config {
    pub fn load(path: Option<&str>) -> Result<Self, Box<dyn std::error::Error>> {
        let mut config = if let Some(p) = path {
            let content = std::fs::read_to_string(p)
                .map_err(|e| format!("Failed to read config file {}: {}", p, e))?;
            serde_yaml::from_str(&content)?
        } else {
            Config::default()
        };

        config.apply_env_overrides();
        config.validate()?;
        Ok(config)
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("THINGD_HOST") {
            self.server.host = v;
        }
        if let Ok(v) = std::env::var("THINGD_PORT")
            && let Ok(n) = v.parse()
        {
            self.server.port = n;
        }
        if let Ok(v) = std::env::var("THINGD_PATH") {
            self.server.database = v;
        }
        if let Ok(v) = std::env::var("THINGD_ENCRYPTION_KEY") {
            self.server.encryption_key = Some(v);
        }
        if let Ok(v) = std::env::var("THINGD_SEARCH_MODE") {
            self.server.search_mode = match v.as_str() {
                "disabled" => SearchModeConfig::Disabled,
                "persistent-no-rebuild" => SearchModeConfig::PersistentNoRebuild,
                _ => SearchModeConfig::Persistent,
            };
        }
        if let Ok(v) = std::env::var("THINGD_SYNC_SOURCE_ID") {
            self.sync.source_id = v;
        }
        if let Ok(v) = std::env::var("THINGD_SYNC_ROLE") {
            self.sync.role = if v == "replica" {
                SyncRole::Replica
            } else {
                SyncRole::Source
            };
        }
        if let Ok(v) = std::env::var("THINGD_SYNC_COLLECTIONS") {
            self.sync.collections = v
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(String::from)
                .collect();
        }
        if let Ok(v) = std::env::var("THINGD_AUTH_TOKEN") {
            self.auth.token = v;
        }
        if let Ok(v) = std::env::var("THINGD_AUTH_MODE") {
            self.auth.mode = match v.as_str() {
                "tenant-jwt" => AuthMode::TenantJwt,
                _ => AuthMode::Bearer,
            };
        }
        if let Ok(v) = std::env::var("THINGD_AUTH_JWKS_URL") {
            self.auth.jwks_url = v;
        }
        if let Ok(v) = std::env::var("THINGD_AUTH_ISSUER") {
            self.auth.issuer = v;
        }
        if let Ok(v) = std::env::var("THINGD_AUTH_AUDIENCE") {
            self.auth.audience = v;
        }
        if let Ok(v) = std::env::var("THINGD_AUTH_TENANT_CLAIM") {
            self.auth.tenant_claim = v;
        }
        if let Ok(v) = std::env::var("THINGD_AUTH_JWKS_CACHE_SECS")
            && let Ok(n) = v.parse()
        {
            self.auth.jwks_cache_secs = n;
        }
        if let Ok(v) = std::env::var("THINGD_ALLOW_UNAUTHENTICATED") {
            self.auth.allow_unauthenticated = v == "true";
        }
        if let Ok(v) = std::env::var("THINGD_MCP_READ_ONLY") {
            self.mcp.read_only = v == "true";
        }
        if let Ok(v) = std::env::var("THINGD_MCP_MAX_PAYLOAD_BYTES")
            && let Ok(n) = v.parse()
        {
            self.mcp.max_payload_bytes = n;
        }
        if let Ok(v) = std::env::var("THINGD_MCP_COLLECTIONS") {
            self.mcp.collection_allowlist = v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        if let Ok(v) = std::env::var("THINGD_MCP_AUDIT") {
            self.mcp.audit = v == "true";
        }
        if let Ok(v) = std::env::var("THINGD_MCP_ACTOR") {
            self.mcp.audit_actor = v;
        }
        if let Ok(v) = std::env::var("THINGD_MCP_SOURCE") {
            self.mcp.audit_source = v;
        }
        if let Ok(v) = std::env::var("THINGD_MCP_AUDIT_STREAM") {
            self.mcp.audit_stream = v;
        }
        if let Ok(v) = std::env::var("THINGD_CLUSTER_MODE") {
            self.cluster.mode = match v.as_str() {
                "leader" => ClusterMode::Leader,
                "follower" => ClusterMode::Follower,
                _ => ClusterMode::Single,
            };
        }
        if let Ok(v) = std::env::var("THINGD_ADVERTISE_URL") {
            self.cluster.advertise_url = v;
        }
        if let Ok(v) = std::env::var("THINGD_CLUSTER_LEADER_URL") {
            self.cluster.leader_url = v;
        }
        if let Ok(v) = std::env::var("THINGD_CLUSTER_LEADER_FALLBACK_URL") {
            self.cluster.fallback_leader_url = v;
        }
        if let Ok(v) = std::env::var("THINGD_CLUSTER_PEERS") {
            self.cluster.peers = v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        if let Ok(v) = std::env::var("THINGD_CLUSTER_FORWARD_AUTH_TOKEN") {
            self.cluster.forward_auth_token = v;
        }
        if let Ok(v) = std::env::var("THINGD_CLUSTER_LEADER_ELECTION") {
            self.cluster.leader_election = v == "true";
        }
        if let Ok(v) = std::env::var("THINGD_CLUSTER_LEADER_ELECTION_MAX_FAILURES")
            && let Ok(n) = v.parse()
        {
            self.cluster.election_max_failures = n;
        }
        if let Ok(v) = std::env::var("THINGD_CORS_ORIGIN") {
            self.hardening.cors_allowed_origins = v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        if let Ok(v) = std::env::var("THINGD_CONNECTOR_ALLOWED_HOSTS") {
            self.hardening.connector_allowed_hosts = v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        if let Ok(v) = std::env::var("THINGD_CONNECTOR_REQUIRE_TLS") {
            self.hardening.connector_require_tls = v == "true";
        }
        if let Ok(v) = std::env::var("THINGD_NLQ_ENABLED") {
            self.nlq.enabled = v == "true";
        }
        if let Ok(v) = std::env::var("THINGD_NLQ_MODEL") {
            self.nlq.model = v;
        }
        if let Ok(v) = std::env::var("THINGD_NLQ_ENDPOINT") {
            self.nlq.endpoint = v;
        }
        if let Ok(v) = std::env::var("THINGD_NLQ_API_KEY") {
            self.nlq.api_key = v;
        }
        if let Ok(v) = std::env::var("THINGD_NLQ_MAX_TOKENS")
            && let Ok(n) = v.parse()
        {
            self.nlq.max_tokens = n;
        }
        if let Ok(v) = std::env::var("THINGD_TENANT_MODE") {
            self.tenant.mode = match v.as_str() {
                "multi-tenant" => TenantMode::MultiTenant,
                _ => TenantMode::Single,
            };
        }
        if let Ok(v) = std::env::var("THINGD_TENANT_HEADER") {
            self.tenant.header = v;
        }
        if let Ok(v) = std::env::var("THINGD_TENANT_DB_PREFIX") {
            self.tenant.database_prefix = v;
        }
    }

    fn validate(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.server.port == 0 {
            return Err("server.port must be between 1 and 65535".into());
        }
        if !self.auth.allow_unauthenticated
            && !self.auth.token.is_empty()
            && self.auth.token.len() < 16
        {
            return Err(
                "auth.token must be at least 16 characters when allow_unauthenticated is false"
                    .into(),
            );
        }
        if (self.server.host == "0.0.0.0" || self.server.host == "::") && self.auth.token.is_empty()
        {
            return Err("binding to 0.0.0.0 requires an auth token (set THINGD_AUTH_TOKEN)".into());
        }
        if self.cluster.mode == ClusterMode::Follower && self.cluster.leader_url.is_empty() {
            return Err("cluster.leader_url is required when mode is 'follower'".into());
        }
        if self.cluster.mode == ClusterMode::Leader && self.cluster.advertise_url.is_empty() {
            return Err("cluster.advertise_url is required when mode is 'leader'".into());
        }
        if self.mcp.max_payload_bytes == 0 {
            return Err("mcp.max_payload_bytes must be greater than 0".into());
        }
        if self.hardening.max_payload_bytes == 0 {
            return Err("hardening.max_payload_bytes must be greater than 0".into());
        }
        if self.hardening.cors_max_age_secs == 0 {
            return Err("hardening.cors_max_age_secs must be greater than 0".into());
        }
        if self.server.production_mode
            && std::env::var("THINGD_AUTH_TOKEN")
                .ok()
                .filter(|t| !t.is_empty())
                .is_none()
            && self.auth.token.is_empty()
            && self.auth.tenant_tokens.is_empty()
            && self.auth.mode != AuthMode::TenantJwt
        {
            return Err("auth.token is required when server.production_mode is true".into());
        }
        if self.tenant.mode == TenantMode::MultiTenant && self.tenant.database_prefix.contains("..")
        {
            return Err("tenant.database_prefix must not contain '..'".into());
        }
        if self.tenant.mode == TenantMode::MultiTenant {
            if self.auth.allow_unauthenticated {
                return Err("multi-tenant mode disallows unauthenticated access".into());
            }
            match self.auth.mode {
                AuthMode::Bearer if self.auth.tenant_tokens.is_empty() => {
                    return Err(
                        "multi-tenant bearer mode requires authenticated tenant_tokens".into(),
                    );
                },
                AuthMode::TenantJwt
                    if self.auth.jwks_url.is_empty()
                        || self.auth.issuer.is_empty()
                        || self.auth.audience.is_empty() =>
                {
                    return Err(
                        "tenant-jwt mode requires auth.jwks_url, auth.issuer, and auth.audience"
                            .into(),
                    );
                },
                _ => {},
            }
        }
        if self.auth.mode == AuthMode::Bearer
            && self
                .auth
                .tenant_tokens
                .iter()
                .any(|(tenant, token)| tenant.is_empty() || token.len() < 16)
        {
            return Err(
                "auth.tenant_tokens require non-empty tenant IDs and tokens of at least 16 characters"
                    .into(),
            );
        }
        if self.hardening.max_connector_file_bytes == 0 {
            return Err("hardening.max_connector_file_bytes must be greater than 0".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_port_range() {
        let mut config = Config::default();
        config.server.port = 0;
        assert!(config.validate().is_err());
        assert!(config.validate().unwrap_err().to_string().contains("port"));
    }

    #[test]
    fn validates_token_length() {
        let mut config = Config::default();
        config.auth.allow_unauthenticated = false;
        config.auth.token = "short".to_string();
        assert!(config.validate().is_err(), "expected err for short token");
        assert!(config.validate().unwrap_err().to_string().contains("token"));
    }

    #[test]
    fn allows_empty_token() {
        let mut config = Config::default();
        config.auth.allow_unauthenticated = false;
        config.auth.token = "".to_string();
        assert!(
            config.validate().is_ok(),
            "expected ok for empty token, got: {:?}",
            config.validate().err()
        );
    }

    #[test]
    fn allows_empty_token_when_unauthenticated() {
        let mut config = Config::default();
        config.auth.allow_unauthenticated = true;
        config.auth.token = "".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn multi_tenant_requires_per_tenant_tokens() {
        let mut config = Config::default();
        config.tenant.mode = TenantMode::MultiTenant;
        config.auth.token = "server-token-that-is-long-enough".to_string();
        assert!(config.validate().is_err());

        config.auth.tenant_tokens.insert(
            "tenant-a".to_string(),
            "tenant-a-token-that-is-long-enough".to_string(),
        );
        assert!(config.validate().is_ok());
    }

    #[test]
    fn multi_tenant_jwt_mode_does_not_require_per_tenant_tokens() {
        let mut config = Config::default();
        config.tenant.mode = TenantMode::MultiTenant;
        config.auth.mode = AuthMode::TenantJwt;
        config.auth.jwks_url = "https://cloud.example/.well-known/jwks.json".to_string();
        config.auth.issuer = "https://cloud.example".to_string();
        config.auth.audience = "thingd-runtime".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn requires_follower_leader_url() {
        let mut config = Config::default();
        config.cluster.mode = ClusterMode::Follower;
        config.cluster.leader_url = "".to_string();
        assert!(config.validate().is_err());
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("leader_url")
        );
    }

    #[test]
    fn requires_leader_advertise_url() {
        let mut config = Config::default();
        config.cluster.mode = ClusterMode::Leader;
        config.cluster.advertise_url = "".to_string();
        assert!(config.validate().is_err());
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("advertise_url")
        );
    }

    #[test]
    fn validates_mcp_payload_limit() {
        let mut config = Config::default();
        config.mcp.max_payload_bytes = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn validates_hardening_payload_limit() {
        let mut config = Config::default();
        config.hardening.max_payload_bytes = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn valid_default_config_passes() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }
}
