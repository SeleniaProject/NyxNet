#![forbid(unsafe_code)]

//! Configuration management for the Nyx SDK.
//!
//! This module provides configuration loading from TOML files, environment variables,
//! and programmatic configuration with validation and defaults.

use crate::error::{NyxError, NyxResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;

/// Main configuration structure for the Nyx SDK
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NyxConfig {
    /// Daemon connection configuration
    pub daemon: DaemonConfig,
    /// Network configuration
    pub network: NetworkConfig,
    /// Security configuration
    pub security: SecurityConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
    /// Stream configuration
    pub streams: StreamConfig,
}

/// Daemon connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Daemon endpoint (Unix socket path or TCP address)
    pub endpoint: Option<String>,
    /// Connection timeout
    #[serde(with = "humantime_serde")]
    pub connect_timeout: Duration,
    /// Request timeout
    #[serde(with = "humantime_serde")]
    pub request_timeout: Duration,
    /// Keep-alive interval
    #[serde(with = "humantime_serde")]
    pub keepalive_interval: Duration,
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Enable multipath routing
    pub multipath: bool,
    /// Maximum number of concurrent paths
    pub max_paths: u8,
    /// Path selection strategy
    pub path_strategy: PathStrategy,
    /// Connection retry configuration
    pub retry: RetryConfig,
}

/// Path selection strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathStrategy {
    /// Weighted round-robin based on latency
    LatencyWeighted,
    /// Random path selection
    Random,
    /// Prefer lowest latency path
    LowestLatency,
    /// Load balancing across all paths
    LoadBalance,
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial retry delay
    #[serde(with = "humantime_serde")]
    pub initial_delay: Duration,
    /// Maximum retry delay
    #[serde(with = "humantime_serde")]
    pub max_delay: Duration,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    /// Enable jitter in retry delays
    pub jitter: bool,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Path to keystore file
    pub keystore_path: Option<PathBuf>,
    /// Enable post-quantum cryptography
    pub post_quantum: bool,
    /// Cipher suite preference
    pub cipher_suites: Vec<String>,
    /// Certificate validation mode
    pub cert_validation: CertValidation,
}

/// Certificate validation mode
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CertValidation {
    /// Strict validation (default)
    Strict,
    /// Allow self-signed certificates
    AllowSelfSigned,
    /// Skip validation (dangerous)
    None,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    pub level: String,
    /// Log format
    pub format: LogFormat,
    /// Enable structured logging
    pub structured: bool,
    /// Log file path (optional)
    pub file: Option<PathBuf>,
}

/// Log format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    /// Human-readable format
    Pretty,
    /// JSON format
    Json,
    /// Compact format
    Compact,
}

/// Stream configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// Default stream buffer size
    pub buffer_size: usize,
    /// Stream idle timeout
    #[serde(with = "humantime_serde")]
    pub idle_timeout: Duration,
    /// Enable automatic reconnection
    pub auto_reconnect: bool,
    /// Maximum number of streams per connection
    pub max_streams: u32,
}

impl Default for NyxConfig {
    fn default() -> Self {
        Self {
            daemon: DaemonConfig::default(),
            network: NetworkConfig::default(),
            security: SecurityConfig::default(),
            logging: LoggingConfig::default(),
            streams: StreamConfig::default(),
        }
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            endpoint: None, // Will use platform default
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            keepalive_interval: Duration::from_secs(30),
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            multipath: true,
            max_paths: 4,
            path_strategy: PathStrategy::LatencyWeighted,
            retry: RetryConfig::default(),
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            keystore_path: None,
            post_quantum: false,
            cipher_suites: vec![
                "TLS_AES_256_GCM_SHA384".to_string(),
                "TLS_CHACHA20_POLY1305_SHA256".to_string(),
                "TLS_AES_128_GCM_SHA256".to_string(),
            ],
            cert_validation: CertValidation::Strict,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: LogFormat::Pretty,
            structured: false,
            file: None,
        }
    }
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            buffer_size: 8192,
            idle_timeout: Duration::from_secs(300),
            auto_reconnect: true,
            max_streams: 100,
        }
    }
}

impl NyxConfig {
    /// Load configuration from a TOML file
    pub async fn load<P: AsRef<Path>>(path: P) -> NyxResult<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path).await
            .map_err(|e| NyxError::config_error(
                format!("Failed to read config file: {}", e),
                Some(path.to_string_lossy().to_string())
            ))?;
        
        let config: NyxConfig = toml::from_str(&content)
            .map_err(|e| NyxError::config_error(
                format!("Failed to parse config file: {}", e),
                Some(path.to_string_lossy().to_string())
            ))?;
        
        config.validate()?;
        Ok(config)
    }
    
    /// Load configuration with environment variable overrides
    pub async fn load_with_env<P: AsRef<Path>>(path: P) -> NyxResult<Self> {
        let mut config = Self::load(path).await?;
        config.apply_env_overrides();
        config.validate()?;
        Ok(config)
    }
    
    /// Create configuration from environment variables only
    pub fn from_env() -> NyxResult<Self> {
        let mut config = Self::default();
        config.apply_env_overrides();
        config.validate()?;
        Ok(config)
    }
    
    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        if let Ok(endpoint) = std::env::var("NYX_DAEMON_ENDPOINT") {
            self.daemon.endpoint = Some(endpoint);
        }
        
        if let Ok(level) = std::env::var("NYX_LOG_LEVEL") {
            self.logging.level = level;
        }
        
        if let Ok(keystore) = std::env::var("NYX_KEYSTORE_PATH") {
            self.security.keystore_path = Some(PathBuf::from(keystore));
        }
        
        if let Ok(pq) = std::env::var("NYX_POST_QUANTUM") {
            self.security.post_quantum = pq.parse().unwrap_or(false);
        }
    }
    
    /// Validate configuration
    pub fn validate(&self) -> NyxResult<()> {
        // Validate daemon configuration
        if self.daemon.connect_timeout.is_zero() {
            return Err(NyxError::invalid_input("connect_timeout must be > 0", Some("daemon.connect_timeout".to_string())));
        }
        
        if self.daemon.request_timeout.is_zero() {
            return Err(NyxError::invalid_input("request_timeout must be > 0", Some("daemon.request_timeout".to_string())));
        }
        
        // Validate network configuration
        if self.network.max_paths == 0 || self.network.max_paths > 8 {
            return Err(NyxError::invalid_input("max_paths must be between 1 and 8", Some("network.max_paths".to_string())));
        }
        
        if self.network.retry.max_attempts == 0 {
            return Err(NyxError::invalid_input("max_attempts must be > 0", Some("network.retry.max_attempts".to_string())));
        }
        
        if self.network.retry.backoff_multiplier <= 1.0 {
            return Err(NyxError::invalid_input("backoff_multiplier must be > 1.0", Some("network.retry.backoff_multiplier".to_string())));
        }
        
        // Validate stream configuration
        if self.streams.buffer_size == 0 {
            return Err(NyxError::invalid_input("buffer_size must be > 0", Some("streams.buffer_size".to_string())));
        }
        
        if self.streams.max_streams == 0 {
            return Err(NyxError::invalid_input("max_streams must be > 0", Some("streams.max_streams".to_string())));
        }
        
        Ok(())
    }
    
    /// Get the daemon endpoint, using platform default if not specified
    pub fn daemon_endpoint(&self) -> String {
        self.daemon.endpoint.clone()
            .unwrap_or_else(|| crate::default_daemon_endpoint())
    }
    
    /// Save configuration to a TOML file
    pub async fn save<P: AsRef<Path>>(&self, path: P) -> NyxResult<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| NyxError::config_error(format!("Failed to serialize config: {}", e), None))?;
        
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await
                .map_err(|e| NyxError::config_error(
                    format!("Failed to create config directory: {}", e),
                    Some(parent.to_string_lossy().to_string())
                ))?;
        }
        
        fs::write(path, content).await
            .map_err(|e| NyxError::config_error(
                format!("Failed to write config file: {}", e),
                Some(path.to_string_lossy().to_string())
            ))?;
        
        Ok(())
    }
}

/// Builder for creating NyxConfig programmatically
pub struct ConfigBuilder {
    config: NyxConfig,
}

impl ConfigBuilder {
    /// Create a new config builder with defaults
    pub fn new() -> Self {
        Self {
            config: NyxConfig::default(),
        }
    }
    
    /// Set daemon endpoint
    pub fn daemon_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.config.daemon.endpoint = Some(endpoint.into());
        self
    }
    
    /// Set connection timeout
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.config.daemon.connect_timeout = timeout;
        self
    }
    
    /// Set request timeout
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.config.daemon.request_timeout = timeout;
        self
    }
    
    /// Enable multipath routing
    pub fn multipath(mut self, enabled: bool) -> Self {
        self.config.network.multipath = enabled;
        self
    }
    
    /// Set maximum number of paths
    pub fn max_paths(mut self, max_paths: u8) -> Self {
        self.config.network.max_paths = max_paths;
        self
    }
    
    /// Set path selection strategy
    pub fn path_strategy(mut self, strategy: PathStrategy) -> Self {
        self.config.network.path_strategy = strategy;
        self
    }
    
    /// Enable post-quantum cryptography
    pub fn post_quantum(mut self, enabled: bool) -> Self {
        self.config.security.post_quantum = enabled;
        self
    }
    
    /// Set keystore path
    pub fn keystore_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.security.keystore_path = Some(path.into());
        self
    }
    
    /// Set log level
    pub fn log_level(mut self, level: impl Into<String>) -> Self {
        self.config.logging.level = level.into();
        self
    }
    
    /// Enable auto-reconnect
    pub fn auto_reconnect(mut self, enabled: bool) -> Self {
        self.config.streams.auto_reconnect = enabled;
        self
    }
    
    /// Set stream buffer size
    pub fn buffer_size(mut self, size: usize) -> Self {
        self.config.streams.buffer_size = size;
        self
    }
    
    /// Build the configuration
    pub fn build(self) -> NyxResult<NyxConfig> {
        self.config.validate()?;
        Ok(self.config)
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
} 