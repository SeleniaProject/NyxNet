#![forbid(unsafe_code)]

//! Capability negotiation helpers as defined in Nyx Protocol v1.0 §8.
//!
//! Capabilities are exchanged as a CBOR array of maps.  Each map entry follows
//! the structure `{id: u32, flags: u8, data: bytes}`.  The least-significant
//! bit of `flags` (0x01) indicates that the capability is *required*; if the
//! peer does not understand a required capability it **MUST** close the
//! connection with error code 0x07 (UNSUPPORTED_CAP) via a CLOSE frame.
//! Optional capabilities (flag 0) may be ignored.
//!
//! This module provides:
//! * [`Capability`] model
//! * CBOR `encode_caps` / `decode_caps`
//! * [`negotiate`] helper that validates peer capability list against the
//!   local implementation and returns the unsupported ID if any.
//!
//! These helpers are intentionally stateless; upper-layer handshake code is
//! responsible for storing negotiated results.

use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// Bit flag indicating the capability is *required*.
pub const FLAG_REQUIRED: u8 = 0x01;

/// Single capability descriptor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capability {
    /// Capability identifier (wire: `id`)
    pub id: u32,
    /// Flags (wire: `flags`)
    pub flags: u8,
    /// Optional opaque data blob (wire: `data`)
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

impl Capability {
    /// Create *required* capability with empty data.
    pub fn required(id: u32) -> Self { Self { id, flags: FLAG_REQUIRED, data: Vec::new() } }
    /// Create *optional* capability with empty data.
    pub fn optional(id: u32) -> Self { Self { id, flags: 0, data: Vec::new() } }
    /// Returns `true` if capability is marked as required.
    pub fn is_required(&self) -> bool { self.flags & FLAG_REQUIRED != 0 }
}

/// CBOR encode capability list.
pub fn encode_caps(caps: &[Capability]) -> Vec<u8> {
    serde_cbor::to_vec(caps).expect("CBOR encode")
}

/// CBOR decode capability list.
pub fn decode_caps(buf: &[u8]) -> Result<Vec<Capability>, serde_cbor::Error> {
    serde_cbor::from_slice(buf)
}

/// Negotiation result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NegotiationError {
    /// Peer advertised required capability we do not support.
    Unsupported(u32),
}

/// Verify that all required peer capabilities are supported locally.
///
/// * `local_supported` – slice of capability IDs this implementation supports.
/// * `peer_caps` – capabilities received from peer (already decoded).
///
/// Returns `Ok(())` when negotiation succeeds, or `Err(NegotiationError)` on
/// first unsupported required capability.
pub fn negotiate(local_supported: &[u32], peer_caps: &[Capability]) -> Result<(), NegotiationError> {
    for cap in peer_caps {
        if cap.is_required() && !local_supported.contains(&cap.id) {
            return Err(NegotiationError::Unsupported(cap.id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cbor_roundtrip() {
        let caps = vec![Capability::required(1), Capability::optional(2)];
        let encoded = encode_caps(&caps);
        let decoded = decode_caps(&encoded).unwrap();
        assert_eq!(caps, decoded);
    }

    #[test]
    fn negotiation() {
        let peer = vec![Capability::required(10), Capability::optional(20)];
        // supported
        assert!(negotiate(&[10, 30], &peer).is_ok());
        // unsupported required
        assert_eq!(negotiate(&[30], &peer).unwrap_err(), NegotiationError::Unsupported(10));
    }
} 