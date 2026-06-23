use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub struct AuthConfig {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub allow_unauthenticated: bool,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
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
fn default_mcp_path() -> String {
    "/mcp".into()
}
fn default_payload_limit() -> usize {
    524_288
}
fn default_true() -> bool {
    true
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
    vec!["http://localhost:8757".to_string()]
}
fn default_cors_max_age() -> u64 {
    86400
}
fn default_rate_limit_enabled() -> bool {
    false
}
fn default_rate_limit_rpm() -> u64 {
    60
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
        if let Ok(v) = std::env::var("THINGD_AUTH_TOKEN") {
            self.auth.token = v;
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
        {
            return Err("auth.token is required when server.production_mode is true".into());
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
