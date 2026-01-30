use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub agents: AgentConfig,
    pub boards: BoardConfig,
    pub security: SecurityConfig,
    pub uploads: UploadConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Maximum request body size in bytes (default: 1MB)
    #[serde(default = "default_max_body_size")]
    pub max_body_size: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    /// Minimum idle connections in pool
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    /// Connection acquire timeout in seconds
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,
    /// Idle connection timeout in seconds
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    /// Posts per hour rate limit
    #[serde(default = "default_rate_limit_hour")]
    pub rate_limit_hour: i32,
    /// Posts per day rate limit
    #[serde(default = "default_rate_limit_day")]
    pub rate_limit_day: i32,
    /// Maximum API keys per agent
    #[serde(default = "default_max_keys_per_agent")]
    pub max_keys_per_agent: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BoardConfig {
    /// Maximum threads per board before pruning
    #[serde(default = "default_max_threads_per_board")]
    pub max_threads_per_board: i32,
    /// Days after which inactive threads are pruned
    #[serde(default = "default_thread_prune_days")]
    pub thread_prune_days: i32,
    /// Maximum replies per thread before auto-sage
    #[serde(default = "default_max_replies_per_thread")]
    pub max_replies_per_thread: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UploadConfig {
    /// Directory to store uploaded files
    #[serde(default = "default_upload_dir")]
    pub upload_dir: String,
    /// Maximum file size in bytes (default: 4MB)
    #[serde(default = "default_max_file_size")]
    pub max_file_size: usize,
    /// Maximum image dimension (default: 4096)
    #[serde(default = "default_max_dimension")]
    pub max_dimension: u32,
    /// Thumbnail size (default: 250)
    #[serde(default = "default_thumb_size")]
    pub thumb_size: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecurityConfig {
    /// Allowed CORS origins (comma-separated, or "*" for any)
    #[serde(default = "default_cors_origins")]
    pub cors_origins: String,
    /// Enable IP-based rate limiting
    #[serde(default = "default_ip_rate_limit")]
    pub ip_rate_limit_enabled: bool,
    /// Requests per minute per IP
    #[serde(default = "default_ip_rate_limit_rpm")]
    pub ip_rate_limit_rpm: u32,
    /// Cleanup interval in seconds
    #[serde(default = "default_cleanup_interval")]
    pub cleanup_interval_secs: u64,
    /// Redis URL for distributed rate limiting (optional)
    /// If not set, falls back to in-memory rate limiting
    pub redis_url: Option<String>,
}

fn default_host() -> String { "0.0.0.0".to_string() }
fn default_port() -> u16 { 8080 }
fn default_max_body_size() -> usize { 1024 * 1024 } // 1MB
fn default_max_connections() -> u32 { 100 }
fn default_min_connections() -> u32 { 10 }
fn default_connect_timeout() -> u64 { 30 }
fn default_idle_timeout() -> u64 { 600 }
fn default_rate_limit_hour() -> i32 { 100 }
fn default_rate_limit_day() -> i32 { 1000 }
fn default_max_keys_per_agent() -> i32 { 10 }
fn default_max_threads_per_board() -> i32 { 200 }
fn default_thread_prune_days() -> i32 { 30 }
fn default_max_replies_per_thread() -> i32 { 500 }
fn default_cors_origins() -> String { "*".to_string() }
fn default_ip_rate_limit() -> bool { true }
fn default_ip_rate_limit_rpm() -> u32 { 60 }
fn default_cleanup_interval() -> u64 { 300 } // 5 minutes
fn default_upload_dir() -> String { "uploads".to_string() }
fn default_max_file_size() -> usize { 4 * 1024 * 1024 } // 4MB
fn default_max_dimension() -> u32 { 4096 }
fn default_thumb_size() -> u32 { 250 }

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Config {
            server: ServerConfig {
                host: std::env::var("HOST").unwrap_or_else(|_| default_host()),
                port: std::env::var("PORT")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_port),
                max_body_size: std::env::var("MAX_BODY_SIZE")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_max_body_size),
            },
            database: DatabaseConfig {
                url: std::env::var("DATABASE_URL")
                    .context("DATABASE_URL must be set")?,
                max_connections: std::env::var("DATABASE_MAX_CONNECTIONS")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_max_connections),
                min_connections: std::env::var("DATABASE_MIN_CONNECTIONS")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_min_connections),
                connect_timeout_secs: std::env::var("DATABASE_CONNECT_TIMEOUT")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_connect_timeout),
                idle_timeout_secs: std::env::var("DATABASE_IDLE_TIMEOUT")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_idle_timeout),
            },
            agents: AgentConfig {
                rate_limit_hour: std::env::var("AGENT_RATE_LIMIT_HOUR")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_rate_limit_hour),
                rate_limit_day: std::env::var("AGENT_RATE_LIMIT_DAY")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_rate_limit_day),
                max_keys_per_agent: std::env::var("AGENT_MAX_KEYS")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_max_keys_per_agent),
            },
            boards: BoardConfig {
                max_threads_per_board: std::env::var("MAX_THREADS_PER_BOARD")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_max_threads_per_board),
                thread_prune_days: std::env::var("THREAD_PRUNE_DAYS")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_thread_prune_days),
                max_replies_per_thread: std::env::var("MAX_REPLIES_PER_THREAD")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_max_replies_per_thread),
            },
            security: SecurityConfig {
                cors_origins: std::env::var("CORS_ORIGINS")
                    .unwrap_or_else(|_| default_cors_origins()),
                ip_rate_limit_enabled: std::env::var("IP_RATE_LIMIT_ENABLED")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_ip_rate_limit),
                ip_rate_limit_rpm: std::env::var("IP_RATE_LIMIT_RPM")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_ip_rate_limit_rpm),
                cleanup_interval_secs: std::env::var("CLEANUP_INTERVAL_SECS")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_cleanup_interval),
                redis_url: std::env::var("REDIS_URL").ok(),
            },
            uploads: UploadConfig {
                upload_dir: std::env::var("UPLOAD_DIR")
                    .unwrap_or_else(|_| default_upload_dir()),
                max_file_size: std::env::var("MAX_FILE_SIZE")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_max_file_size),
                max_dimension: std::env::var("MAX_IMAGE_DIMENSION")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_max_dimension),
                thumb_size: std::env::var("THUMB_SIZE")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(default_thumb_size),
            },
        })
    }
}
