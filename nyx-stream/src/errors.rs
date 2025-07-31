use thiserror::Error;

/// Result type for stream operations
pub type StreamResult<T> = Result<T, StreamError>;

/// Stream error types
#[derive(Error, Debug, Clone)]
pub enum StreamError {
    #[error("Invalid frame: {0}")]
    InvalidFrame(String),

    #[error("Stream reset: {0}")]
    StreamReset(String),

    #[error("Buffer full: {0}")]
    BufferFull(String),

    #[error("Flow control violation: {0}")]
    FlowControlViolation(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<std::io::Error> for StreamError {
    fn from(err: std::io::Error) -> Self {
        StreamError::Io(err.to_string())
    }
}

impl From<tokio::time::error::Elapsed> for StreamError {
    fn from(err: tokio::time::error::Elapsed) -> Self {
        StreamError::Timeout(err.to_string())
    }
}
