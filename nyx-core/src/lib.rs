#![forbid(unsafe_code)]

pub mod config;
pub mod error;
pub mod types;
#[cfg(target_os = "linux")]
pub mod sandbox;

pub use config::NyxConfig;
pub use error::NyxError;
pub use error::NyxResult;
pub use types::NodeId;
#[cfg(target_os = "linux")]
pub use sandbox::install_seccomp;
