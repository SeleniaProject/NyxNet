#![forbid(unsafe_code)]

//! Nyx SDK for application integration.
//!
//! This crate provides a high-level API for applications to interact with the Nyx network
//! through the daemon. It includes:
//!
//! - `NyxStream`: AsyncRead + AsyncWrite stream interface with automatic reconnection
//! - `NyxDaemon`: Connection manager and configuration builder
//! - `NyxConfig`: Configuration management with TOML loading
//! - Comprehensive error handling and propagation
//! - Event callbacks and monitoring
//!
//! # Example
//!
//! ```rust,no_run
//! use nyx_sdk::{NyxConfig, NyxDaemon, NyxStream};
//! use tokio::io::{AsyncReadExt, AsyncWriteExt};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = NyxConfig::load("~/.config/nyx.toml").await?;
//!     let mut daemon = NyxDaemon::connect(config).await?;
//!     
//!     let mut stream = daemon.open_stream("example.com:80").await?;
//!     stream.write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await?;
//!     
//!     let mut buffer = Vec::new();
//!     stream.read_to_end(&mut buffer).await?;
//!     println!("Response: {}", String::from_utf8_lossy(&buffer));
//!     
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod daemon;
pub mod stream;
pub mod error;
pub mod events;
pub mod proto;
pub mod retry;

#[cfg(feature = "reconnect")]
pub mod reconnect;

// Re-export main types
pub use config::{NyxConfig, ConfigBuilder, NetworkConfig, SecurityConfig};
pub use daemon::{NyxDaemon, ConnectionInfo, DaemonStatus, NodeInfo};
pub use stream::{NyxStream, StreamOptions, StreamState, StreamStats};
pub use error::{NyxError, NyxResult, ErrorKind, ErrorSeverity};
pub use events::{NyxEvent, EventHandler, EventCallback};
pub use retry::{RetryStrategy, RetryExecutor, CircuitBreaker, retry, retry_with_strategy};

pub use proto::NyxControlClient;

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROTOCOL_VERSION: u16 = 0x0001; // v0.1

/// Default daemon endpoint for the current platform
pub fn default_daemon_endpoint() -> String {
    #[cfg(unix)]
    return "unix:///tmp/nyx.sock".to_string();
    #[cfg(windows)]
    return "tcp://127.0.0.1:43299".to_string();
}

/// Initialize the SDK with default logging configuration
pub fn init() {
    use tracing_subscriber::fmt;
    fmt::init();
}

/// Initialize the SDK with custom tracing subscriber
pub fn init_with_subscriber<S>(subscriber: S)
where
    S: tracing::Subscriber + Send + Sync + 'static,
{
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
}

/// Create a new SDK client with default configuration
pub async fn connect() -> crate::error::NyxResult<crate::daemon::NyxDaemon> {
    let config = crate::config::NyxConfig::default();
    crate::daemon::NyxDaemon::connect(config).await
}

/// Create a new SDK client with custom configuration
pub async fn connect_with_config(config: crate::config::NyxConfig) -> crate::error::NyxResult<crate::daemon::NyxDaemon> {
    crate::daemon::NyxDaemon::connect(config).await
}

/// Create a new SDK client from environment variables
pub async fn connect_from_env() -> crate::error::NyxResult<crate::daemon::NyxDaemon> {
    let config = crate::config::NyxConfig::from_env()?;
    crate::daemon::NyxDaemon::connect(config).await
}

/// Create a new SDK client from configuration file
pub async fn connect_from_file<P: AsRef<std::path::Path>>(path: P) -> crate::error::NyxResult<crate::daemon::NyxDaemon> {
    let config = crate::config::NyxConfig::load_with_env(path).await?;
    crate::daemon::NyxDaemon::connect(config).await
}

/// Utility function to create a simple event handler
pub fn create_logging_handler(name: impl Into<String>) -> std::sync::Arc<dyn crate::events::EventHandler> {
    std::sync::Arc::new(crate::events::LoggingEventHandler::new(name))
}

/// Utility function to create a filtered event handler
pub fn create_filtered_handler(
    handler: std::sync::Arc<dyn crate::events::EventHandler>,
    filter: crate::events::EventFilter,
) -> std::sync::Arc<dyn crate::events::EventHandler> {
    std::sync::Arc::new(crate::events::FilteredEventHandler::new(handler, filter))
}

 