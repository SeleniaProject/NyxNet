#![forbid(unsafe_code)]

//! Capability negotiation utilities (Nyx Protocol v1.0 §8).
//!
//! A peer advertises a CBOR array of `{id:u32, flags:u8}` structures in the
//! first CRYPTO frame.  This module provides helpers to parse that CBOR blob
//! and to verify that all *required* capabilities are supported locally.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use crate::{NyxError, NyxResult};

/// Flag indicating the capability is *required* by the sender.
pub const FLAG_REQUIRED: u8 = 0x01;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Capability {
    pub id: u32,
    pub flags: u8,
}

impl Capability {
    /// Whether this capability is marked as required.
    #[must_use]
    pub fn required(&self) -> bool {
        self.flags & FLAG_REQUIRED != 0
    }
}

/// Parse CBOR-encoded capability array.
#[must_use]
pub fn parse_cbor(data: &[u8]) -> NyxResult<Vec<Capability>> {
    let caps: Vec<Capability> = serde_cbor::from_slice(data)?;
    Ok(caps)
}

/// Negotiate capabilities: ensure every remote *required* capability is in the
/// provided `local_supported` set.  Returns `Ok(())` on success or
/// `NyxError::UnsupportedCap` if a required capability is missing.
pub fn negotiate(local_supported: &HashSet<u32>, remote: &[Capability]) -> NyxResult<()> {
    for cap in remote {
        if cap.required() && !local_supported.contains(&cap.id) {
            return Err(NyxError::UnsupportedCap(cap.id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiation_rejects_missing_required() {
        let local: HashSet<u32> = [1u32, 2].into_iter().collect();
        let remote = vec![Capability { id: 2, flags: 0 }, Capability { id: 5, flags: FLAG_REQUIRED }];
        let res = negotiate(&local, &remote);
        assert!(matches!(res, Err(NyxError::UnsupportedCap(5))));
    }

    #[test]
    fn negotiation_accepts_optional_unknown() {
        let local: HashSet<u32> = [1u32].into_iter().collect();
        let remote = vec![Capability { id: 42, flags: 0 }];
        assert!(negotiate(&local, &remote).is_ok());
    }
} 