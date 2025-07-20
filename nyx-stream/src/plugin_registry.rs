//! Plugin Registration & Permission Model
//!
//! Nyx reserves Frame Type 0x50–0x5F for plugins. Each plugin is identified by
//! a 32-bit ID and may request certain permissions at runtime. This registry is
//! maintained per‐connection during handshake (SETTINGS frame negotiation).
//!
//! For now we support three basic permissions:
//! * `SEND_STREAM` – plugin may transmit STREAM frames.
//! * `SEND_DATAGRAM` – plugin may transmit DATAGRAM (QUIC) encapsulated frames.
//! * `ACCESS_GEO` – plugin may access coarse geolocation sensor data.
//!
//! The registry is kept in memory and can be queried by upper layers before
//! forwarding plugin traffic.
//!
//! Future work: persist to disk, dynamic capability revocation.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use serde::{Serialize, Deserialize};

bitflags::bitflags! {
    #[derive(Serialize, Deserialize)]
    pub struct Permission: u8 {
        const SEND_STREAM  = 0b0000_0001;
        const SEND_DATAGRAM= 0b0000_0010;
        const ACCESS_GEO   = 0b0000_0100;
    }
}

/// Metadata advertised by a plugin during the registration handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: u32,
    pub name: String,
    pub permissions: Permission,
}

/// Runtime registry tracking plugin permissions.
#[derive(Default)]
pub struct PluginRegistry {
    plugins: HashMap<u32, Permission>,
}

impl PluginRegistry {
    #[must_use] pub fn new() -> Self { Self { plugins: HashMap::new() } }

    /// Register a plugin; returns Err if ID already taken.
    pub fn register(&mut self, info: &PluginInfo) -> Result<(), ()> {
        if self.plugins.contains_key(&info.id) { return Err(()); }
        self.plugins.insert(info.id, info.permissions);
        Ok(())
    }

    /// Check whether `plugin_id` holds all required `perm` bits.
    #[must_use]
    pub fn has_permission(&self, plugin_id: u32, perm: Permission) -> bool {
        self.plugins.get(&plugin_id).map_or(false, |p| p.contains(perm))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_basic() {
        let mut reg = PluginRegistry::new();
        let info = PluginInfo { id: 0xdead_beef, name: "geo".into(), permissions: Permission::ACCESS_GEO };
        assert!(reg.register(&info).is_ok());
        assert!(reg.has_permission(0xdead_beef, Permission::ACCESS_GEO));
        assert!(!reg.has_permission(0xdead_beef, Permission::SEND_STREAM));
        assert!(reg.register(&info).is_err()); // duplicate
    }
} 