#![forbid(unsafe_code)]

//! Common error type for Nyx crates.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NyxError {
    /// I/O related failures.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Configuration parsing failures.
    #[error("Config parse error: {0}")]
    ConfigParse(toml::de::Error),

    /// Filesystem watch errors.
    #[error("Notify error: {0}")]
    Notify(#[from] notify::Error),

    /// CBOR decode errors
    #[error("CBOR decode error: {0}")]
    Cbor(#[from] serde_cbor::Error),

    /// Required capability not supported by local implementation
    #[error("Unsupported required capability {0}")]
    UnsupportedCap(u32),
}

/// Convenient alias for results throughout Nyx crates.
pub type NyxResult<T> = Result<T, NyxError>; 