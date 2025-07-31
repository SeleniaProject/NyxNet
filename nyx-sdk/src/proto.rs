#![allow(clippy::all)]
#![allow(dead_code)]
#![allow(unused_imports)]

//! Generated protobuf code for Nyx control protocol

// Include the generated protobuf code
tonic::include_proto!("nyx.api");

// Re-export commonly used types
pub use nyx_control_client::NyxControlClient;

// Note: Conversions are implemented in stream.rs to avoid conflicts