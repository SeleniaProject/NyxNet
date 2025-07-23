#![forbid(unsafe_code)]

//! Compliance level detection helper.
//!
//! According to Nyx Protocol v1.0 §10 the levels are:
//!   * Core  – implements v0.1 baseline feature set
//!   * Plus  – additionally supports Multipath & Hybrid PQ handshake
//!   * Full  – all features (cMix, Plugins, LowPower)
//!
//! This module evaluates a peer's announced capability list and classifies it
//! into one of the levels for monitoring or policy enforcement.

use crate::capability::Capability;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComplianceLevel {
    Core,
    Plus,
    Full,
}

/// Predefined capability IDs (from spec v1.0 Appendix).
pub mod cap {
    pub const MULTIPATH: u32 = 0x0001;
    pub const HYBRID_PQ: u32 = 0x0002;
    pub const CMIX: u32 = 0x0003;
    pub const PLUGIN: u32 = 0x0004;
    pub const LOW_POWER: u32 = 0x0005;
}

/// Determine compliance level from advertised capabilities.
#[must_use]
pub fn determine(caps: &[Capability]) -> ComplianceLevel {
    let has = |id| caps.iter().any(|c| c.id == id);

    let _core_ok = true; // baseline assumed
    let plus_ok = has(cap::MULTIPATH) && has(cap::HYBRID_PQ);
    let full_ok = plus_ok && has(cap::CMIX) && has(cap::PLUGIN) && has(cap::LOW_POWER);

    if full_ok {
        ComplianceLevel::Full
    } else if plus_ok {
        ComplianceLevel::Plus
    } else {
        ComplianceLevel::Core
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn core_level() {
        let caps = vec![];
        assert_eq!(determine(&caps), ComplianceLevel::Core);
    }

    #[test]
    fn plus_level() {
        let caps = vec![Capability{ id: cap::MULTIPATH, flags:0}, Capability{ id:cap::HYBRID_PQ, flags:0}];
        assert_eq!(determine(&caps), ComplianceLevel::Plus);
    }

    #[test]
    fn full_level() {
        let caps = vec![
            Capability{ id: cap::MULTIPATH, flags:0},
            Capability{ id: cap::HYBRID_PQ, flags:0},
            Capability{ id: cap::CMIX, flags:0},
            Capability{ id: cap::PLUGIN, flags:0},
            Capability{ id: cap::LOW_POWER, flags:0},
        ];
        assert_eq!(determine(&caps), ComplianceLevel::Full);
    }
} 