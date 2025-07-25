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

#[cfg(feature = "reconnect")]
pub mod reconnect;

// Re-export main types
pub use config::{NyxConfig, ConfigBuilder, NetworkConfig, SecurityConfig};
pub use daemon::{NyxDaemon, ConnectionInfo, DaemonStatus, NodeInfo};
pub use stream::{NyxStream, StreamOptions, StreamState, StreamStats};
pub use error::{NyxError, NyxResult, ErrorKind};
pub use events::{NyxEvent, EventHandler, EventCallback};

// Generated gRPC client code
mod proto {
    tonic::include_proto!("nyx.api");
}

use proto::nyx_control_client::NyxControlClient;
pub use proto::StreamResponse;

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
    tracing_subscriber::fmt::init();
}

/// Initialize the SDK with custom tracing subscriber
pub fn init_with_subscriber<S>(subscriber: S)
where
    S: tracing::Subscriber + Send + Sync + 'static,
{
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
} 