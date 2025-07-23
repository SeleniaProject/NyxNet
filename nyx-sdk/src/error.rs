#![forbid(unsafe_code)]

//! Error handling for the Nyx SDK.
//!
//! This module provides comprehensive error types that can occur during SDK operations,
//! with detailed error information and context for debugging and error propagation.

use std::fmt;
use thiserror::Error;

/// Result type alias for SDK operations
pub type NyxResult<T> = Result<T, NyxError>;

/// Main error type for all SDK operations
#[derive(Error, Debug)]
pub enum NyxError {
    /// Connection to daemon failed
    #[error("Failed to connect to Nyx daemon: {message}")]
    DaemonConnection { message: String, source: Option<Box<dyn std::error::Error + Send + Sync>> },
    
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
            _ => false,
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
            source: Some(Box::new(source)),
        }
    }
    
    pub fn daemon_connection_msg(message: impl Into<String>) -> Self {
        Self::DaemonConnection {
            message: message.into(),
            source: None,
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
        let code = match status.code() {
            tonic::Code::NotFound => Some(0x404),
            tonic::Code::PermissionDenied => Some(0x403),
            tonic::Code::Unauthenticated => Some(0x401),
            tonic::Code::InvalidArgument => Some(0x400),
            tonic::Code::DeadlineExceeded => return Self::timeout(std::time::Duration::from_secs(30)),
            tonic::Code::Unavailable => return Self::daemon_connection_msg("Daemon unavailable"),
            _ => None,
        };
        
        Self::Protocol {
            message: status.message().to_string(),
            code,
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