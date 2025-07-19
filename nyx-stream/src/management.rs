#![forbid(unsafe_code)]

//! Management frame definitions (PING/PONG, SETTINGS, etc.) according to Nyx Protocol §16.
//! Currently implements PING (0x31) and PONG (0x32) frames.

use nom::{IResult, number::complete::be_u64};

/// PING frame (Type=0x31).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PingFrame {
    /// Random nonce echoed back by the peer.
    pub nonce: u64,
}

/// PONG frame (Type=0x32).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PongFrame {
    /// Nonce copied from corresponding PING frame.
    pub nonce: u64,
}

/// Parse PING payload (8-byte big-endian nonce).
pub fn parse_ping_frame(input: &[u8]) -> IResult<&[u8], PingFrame> {
    let (input, nonce) = be_u64(input)?;
    Ok((input, PingFrame { nonce }))
}

/// Build PING payload (8-byte big-endian nonce).
pub fn build_ping_frame(frame: &PingFrame) -> [u8; 8] {
    frame.nonce.to_be_bytes()
}

/// Parse PONG payload (8-byte big-endian nonce).
pub fn parse_pong_frame(input: &[u8]) -> IResult<&[u8], PongFrame> {
    let (input, nonce) = be_u64(input)?;
    Ok((input, PongFrame { nonce }))
}

/// Build PONG payload (8-byte big-endian nonce).
pub fn build_pong_frame(frame: &PongFrame) -> [u8; 8] {
    frame.nonce.to_be_bytes()
} 