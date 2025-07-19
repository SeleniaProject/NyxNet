#![forbid(unsafe_code)]
//! Nyx Secure Stream layer (skeleton)

pub mod frame;

pub use frame::{FrameHeader, parse_header};
