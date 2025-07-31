#![forbid(unsafe_code)]

//! Error handling for the Nyx SDK.
//!
//! This module provides comprehensive error types that can occur during SDK operations,
//! with detailed error information and context for debugging and error propagation.

use thiserror::Error;

/// Result type alias for SDK operations
pub type NyxResult<T> = Result<T, NyxError>;

/// Main error type for all SDK operations
#[derive(Error, Debug, Clone)]
pub enum NyxError {
    /// Connection to daemon failed
    #[error("Failed to connect to Nyx daemon: {message}")]
    DaemonConnection { message: String, source_msg: Option<String> },
    
    /// Stream operation failed
    #[error("Stream operation failed: {message}")]
    Stream { message: String, stream_id: Option<u32> },
    
    /// Network error occurred
    #[error("Network error: {message}")]
    Network { message: String, endpoint: Option<String> },
    
    /// Configuration error
    #[error("Configuration error: {message}")]
    Config { message: String, path: Option<String> },
    
    /// Authentication/authorization error
    #[error("Authentication error: {message}")]
    Auth { message: String },
    
    /// Protocol error
    #[error("Protocol error: {message}")]
    Protocol { message: String, code: Option<u16> },
    
    /// Timeout occurred
    #[error("Operation timed out after {duration:?}")]
    Timeout { duration: std::time::Duration },
    
    /// Resource not found
    #[error("Resource not found: {resource}")]
    NotFound { resource: String },
    
    /// Resource already exists
    #[error("Resource already exists: {resource}")]
    AlreadyExists { resource: String },
    
    /// Permission denied
    #[error("Permission denied: {operation}")]
    PermissionDenied { operation: String },
    
    /// Invalid input provided
    #[error("Invalid input: {message}")]
    InvalidInput { message: String, field: Option<String> },
    
    /// Internal SDK error
    #[error("Internal error: {message}")]
    Internal { message: String },
    
    /// Operation was cancelled
    #[error("Operation cancelled")]
    Cancelled,
    
    /// Reconnection failed
    #[cfg(feature = "reconnect")]
    #[error("Reconnection failed after {attempts} attempts")]
    ReconnectionFailed { attempts: u32 },
}

/// Error severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Low severity - operation can continue
    Low,
    /// Medium severity - may affect performance
    Medium,
    /// High severity - significant impact
    High,
    /// Critical severity - system failure
    Critical,
}

/// Error kind for categorizing errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Connection-related errors
    Connection,
    /// Stream-related errors
    Stream,
    /// Network-related errors
    Network,
    /// Configuration-related errors
    Configuration,
    /// Authentication-related errors
    Authentication,
    /// Protocol-related errors
    Protocol,
    /// Timeout errors
    Timeout,
    /// Resource errors
    Resource,
    /// Permission errors
    Permission,
    /// Input validation errors
    Input,
    /// Internal errors
    Internal,
    /// Cancellation errors
    Cancellation,
    /// Reconnection errors
    #[cfg(feature = "reconnect")]
    Reconnection,
}

impl NyxError {
    /// Get the error kind
    pub fn kind(&self) -> ErrorKind {
        match self {
            NyxError::DaemonConnection { .. } => ErrorKind::Connection,
            NyxError::Stream { .. } => ErrorKind::Stream,
            NyxError::Network { .. } => ErrorKind::Network,
            NyxError::Config { .. } => ErrorKind::Configuration,
            NyxError::Auth { .. } => ErrorKind::Authentication,
            NyxError::Protocol { .. } => ErrorKind::Protocol,
            NyxError::Timeout { .. } => ErrorKind::Timeout,
            NyxError::NotFound { .. } | NyxError::AlreadyExists { .. } => ErrorKind::Resource,
            NyxError::PermissionDenied { .. } => ErrorKind::Permission,
            NyxError::InvalidInput { .. } => ErrorKind::Input,
            NyxError::Internal { .. } => ErrorKind::Internal,
            NyxError::Cancelled => ErrorKind::Cancellation,
            #[cfg(feature = "reconnect")]
            NyxError::ReconnectionFailed { .. } => ErrorKind::Reconnection,
        }
    }
    
    /// Check if the error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            NyxError::DaemonConnection { .. } => true,
            NyxError::Network { .. } => true,
            NyxError::Timeout { .. } => true,
            NyxError::Internal { .. } => true,
            NyxError::Protocol { code, .. } => {
                // Retry on server errors (5xx) but not client errors (4xx)
                match code {
                    Some(code) => *code >= 500,
                    None => true, // Unknown protocol errors are retryable
                }
            }
            _ => false,
        }
    }
    
    /// Check if the error is recoverable (can continue operation)
    pub fn is_recoverable(&self) -> bool {
        match self {
            NyxError::Stream { .. } => true,
            NyxError::Network { .. } => true,
            NyxError::Timeout { .. } => true,
            NyxError::NotFound { .. } => true,
            NyxError::Protocol { code, .. } => {
                // Most protocol errors are recoverable except authentication
                match code {
                    Some(401) | Some(403) => false, // Auth errors not recoverable
                    _ => true,
                }
            }
            NyxError::Auth { .. } => false, // Auth errors require manual intervention
            NyxError::PermissionDenied { .. } => false, // Permission errors not recoverable
            NyxError::InvalidInput { .. } => false, // Input errors need fixing
            _ => true,
        }
    }
    
    /// Get retry delay suggestion based on error type
    pub fn retry_delay(&self) -> Option<std::time::Duration> {
        match self {
            NyxError::DaemonConnection { .. } => Some(std::time::Duration::from_millis(500)),
            NyxError::Network { .. } => Some(std::time::Duration::from_millis(200)),
            NyxError::Timeout { .. } => Some(std::time::Duration::from_millis(100)),
            NyxError::Internal { .. } => Some(std::time::Duration::from_secs(1)),
            NyxError::Protocol { .. } => Some(std::time::Duration::from_millis(300)),
            _ => None,
        }
    }
    
    /// Get troubleshooting information for the error
    pub fn troubleshooting_info(&self) -> Option<String> {
        match self {
            NyxError::DaemonConnection { .. } => Some(
                "Check if the Nyx daemon is running and accessible at the configured endpoint. \
                 Verify network connectivity and firewall settings.".to_string()
            ),
            NyxError::Auth { .. } => Some(
                "Authentication failed. Check your authentication token or credentials. \
                 Ensure the token is valid and has not expired.".to_string()
            ),
            NyxError::PermissionDenied { .. } => Some(
                "Permission denied. Verify that your account has the necessary permissions \
                 for this operation. Contact your administrator if needed.".to_string()
            ),
            NyxError::Network { .. } => Some(
                "Network error occurred. Check your internet connection and try again. \
                 If the problem persists, the remote service may be temporarily unavailable.".to_string()
            ),
            NyxError::Timeout { .. } => Some(
                "Operation timed out. This may be due to network congestion or server load. \
                 Try increasing the timeout value or retry the operation.".to_string()
            ),
            NyxError::NotFound { .. } => Some(
                "The requested resource was not found. Verify the resource identifier \
                 and ensure it exists.".to_string()
            ),
            NyxError::InvalidInput { .. } => Some(
                "Invalid input provided. Check the input parameters and ensure they \
                 meet the required format and constraints.".to_string()
            ),
            _ => None,
        }
    }
    
    /// Get error severity level
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            NyxError::DaemonConnection { .. } => ErrorSeverity::High,
            NyxError::Auth { .. } => ErrorSeverity::High,
            NyxError::PermissionDenied { .. } => ErrorSeverity::High,
            NyxError::Stream { .. } => ErrorSeverity::Medium,
            NyxError::Network { .. } => ErrorSeverity::Medium,
            NyxError::Protocol { .. } => ErrorSeverity::Medium,
            NyxError::Timeout { .. } => ErrorSeverity::Low,
            NyxError::NotFound { .. } => ErrorSeverity::Low,
            NyxError::InvalidInput { .. } => ErrorSeverity::Low,
            NyxError::Internal { .. } => ErrorSeverity::Critical,
            NyxError::Cancelled => ErrorSeverity::Low,
            NyxError::AlreadyExists { .. } => ErrorSeverity::Low,
            NyxError::Config { .. } => ErrorSeverity::High,
            #[cfg(feature = "reconnect")]
            NyxError::ReconnectionFailed { .. } => ErrorSeverity::High,
        }
    }
    
    /// Get error code for protocol errors
    pub fn error_code(&self) -> Option<u16> {
        match self {
            NyxError::Protocol { code, .. } => *code,
            _ => None,
        }
    }
    
    /// Get stream ID if available
    pub fn stream_id(&self) -> Option<u32> {
        match self {
            NyxError::Stream { stream_id, .. } => *stream_id,
            _ => None,
        }
    }
}

// Convenience constructors
impl NyxError {
    pub fn daemon_connection<E: std::error::Error + Send + Sync + 'static>(message: impl Into<String>, source: E) -> Self {
        Self::DaemonConnection {
            message: message.into(),
            source_msg: Some(source.to_string()),
        }
    }
    
    pub fn daemon_connection_msg(message: impl Into<String>) -> Self {
        Self::DaemonConnection {
            message: message.into(),
            source_msg: None,
        }
    }
    
    pub fn stream_error(message: impl Into<String>, stream_id: Option<u32>) -> Self {
        Self::Stream {
            message: message.into(),
            stream_id,
        }
    }
    
    pub fn network_error(message: impl Into<String>, endpoint: Option<String>) -> Self {
        Self::Network {
            message: message.into(),
            endpoint,
        }
    }
    
    pub fn config_error(message: impl Into<String>, path: Option<String>) -> Self {
        Self::Config {
            message: message.into(),
            path,
        }
    }
    
    pub fn protocol_error(message: impl Into<String>, code: Option<u16>) -> Self {
        Self::Protocol {
            message: message.into(),
            code,
        }
    }
    
    pub fn timeout(duration: std::time::Duration) -> Self {
        Self::Timeout { duration }
    }
    
    pub fn not_found(resource: impl Into<String>) -> Self {
        Self::NotFound {
            resource: resource.into(),
        }
    }
    
    pub fn invalid_input(message: impl Into<String>, field: Option<String>) -> Self {
        Self::InvalidInput {
            message: message.into(),
            field,
        }
    }
    
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }
}

// Convert from common error types
impl From<tonic::Status> for NyxError {
    fn from(status: tonic::Status) -> Self {
        // Comprehensive gRPC status code to NyxError mapping
        match status.code() {
            tonic::Code::Ok => {
                // This shouldn't happen, but handle gracefully
                Self::internal("Received OK status as error")
            }
            tonic::Code::Cancelled => {
                Self::Cancelled
            }
            tonic::Code::Unknown => {
                Self::internal(format!("Unknown gRPC error: {}", status.message()))
            }
            tonic::Code::InvalidArgument => {
                Self::invalid_input(status.message().to_string(), None)
            }
            tonic::Code::DeadlineExceeded => {
                Self::timeout(std::time::Duration::from_secs(30))
            }
            tonic::Code::NotFound => {
                Self::not_found(status.message().to_string())
            }
            tonic::Code::AlreadyExists => {
                Self::AlreadyExists {
                    resource: status.message().to_string(),
                }
            }
            tonic::Code::PermissionDenied => {
                Self::PermissionDenied {
                    operation: status.message().to_string(),
                }
            }
            tonic::Code::ResourceExhausted => {
                Self::network_error(
                    format!("Resource exhausted: {}", status.message()),
                    None
                )
            }
            tonic::Code::FailedPrecondition => {
                Self::protocol_error(
                    format!("Failed precondition: {}", status.message()),
                    Some(0x428) // 428 Precondition Required
                )
            }
            tonic::Code::Aborted => {
                Self::network_error(
                    format!("Operation aborted: {}", status.message()),
                    None
                )
            }
            tonic::Code::OutOfRange => {
                Self::invalid_input(
                    format!("Out of range: {}", status.message()),
                    None
                )
            }
            tonic::Code::Unimplemented => {
                Self::protocol_error(
                    format!("Unimplemented: {}", status.message()),
                    Some(0x501) // 501 Not Implemented
                )
            }
            tonic::Code::Internal => {
                Self::internal(format!("Internal server error: {}", status.message()))
            }
            tonic::Code::Unavailable => {
                Self::daemon_connection_msg(format!("Daemon unavailable: {}", status.message()))
            }
            tonic::Code::DataLoss => {
                Self::network_error(
                    format!("Data loss detected: {}", status.message()),
                    None
                )
            }
            tonic::Code::Unauthenticated => {
                Self::Auth {
                    message: format!("Authentication failed: {}", status.message()),
                }
            }
        }
    }
}

impl From<tonic::transport::Error> for NyxError {
    fn from(error: tonic::transport::Error) -> Self {
        Self::daemon_connection("Transport error", error)
    }
}

impl From<std::io::Error> for NyxError {
    fn from(error: std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::NotFound => Self::not_found("Resource not found"),
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied {
                operation: "I/O operation".to_string(),
            },
            std::io::ErrorKind::TimedOut => Self::timeout(std::time::Duration::from_secs(30)),
            _ => Self::network_error(error.to_string(), None),
        }
    }
}

impl From<toml::de::Error> for NyxError {
    fn from(error: toml::de::Error) -> Self {
        Self::config_error(format!("TOML parse error: {}", error), None)
    }
}

impl From<serde_json::Error> for NyxError {
    fn from(error: serde_json::Error) -> Self {
        Self::config_error(format!("JSON parse error: {}", error), None)
    }
} 