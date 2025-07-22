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
use std::path::Path;
use std::fs::{File, create_dir_all};
use std::io::{self, Read, Write};
use semver::Version;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Permission: u8 {
        const SEND_STREAM  = 0b0000_0001;
        const SEND_DATAGRAM= 0b0000_0010;
        const ACCESS_GEO   = 0b0000_0100;
    }
}

/// Permissions that the host is willing to grant.
const ALLOWED_PERMS: Permission = Permission::SEND_STREAM
    .union(Permission::SEND_DATAGRAM)
    .union(Permission::ACCESS_GEO);

/// Metadata advertised by a plugin during the registration handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Unique 32-bit identifier assigned to the plugin.
    pub id: u32,
    /// Human-readable plugin name.
    pub name: String,
    /// Semantic version of the plugin implementation.
    pub version: Version,
    /// Permission bits requested by the plugin.
    pub permissions: Permission,
}

/// Runtime registry tracking plugin permissions.
#[derive(Default)]
/// Thread‐unsafe plugin registry. Callers must provide their own sync wrapper if shared.
pub struct PluginRegistry {
    plugins: HashMap<u32, (Permission, Version)>,
}

impl PluginRegistry {
    #[must_use] pub fn new() -> Self { Self { plugins: HashMap::new() } }

    /// Register a plugin; returns Err if ID already taken.
    pub fn register(&mut self, info: &PluginInfo) -> Result<(), ()> {
        if self.plugins.contains_key(&info.id) { return Err(()); }
        // API version compatibility check: we currently support major version 1 only.
        const SUPPORTED_MAJOR: u64 = 1;
        if info.version.major != SUPPORTED_MAJOR {
            return Err(());
        }
        // Permission enforcement.
        if !ALLOWED_PERMS.contains(info.permissions) {
            return Err(());
        }
        self.plugins.insert(info.id, (info.permissions, info.version.clone()));
        Ok(())
    }

    /// Check whether `plugin_id` holds all required `perm` bits.
    #[must_use]
    pub fn has_permission(&self, plugin_id: u32, perm: Permission) -> bool {
        self.plugins.get(&plugin_id).map_or(false, |(p, _)| p.contains(perm))
    }

    /// Persist registry as CBOR to `path`. Creates parent dir if missing.
    pub fn save_to<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        let p = path.as_ref();
        if let Some(parent) = p.parent() { create_dir_all(parent)?; }
        let mut file = File::create(p)?;
        let bytes = serde_cbor::to_vec(&self.plugins).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        file.write_all(&bytes)
    }

    /// Load registry from CBOR file; returns empty registry if file missing.
    pub fn load_from<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let p = path.as_ref();
        if !p.exists() { return Ok(Self::new()); }
        let mut buf = Vec::new();
        File::open(p)?.read_to_end(&mut buf)?;
        let plugins: HashMap<u32, (Permission, Version)> = serde_cbor::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        Ok(Self { plugins })
    }

    /// Revoke given permission bits from plugin at runtime.
    /// Returns `true` if any bit was cleared.
    pub fn revoke(&mut self, plugin_id: u32, perm: Permission) -> bool {
        if let Some((p, _)) = self.plugins.get_mut(&plugin_id) {
            let before = *p;
            p.remove(perm);
            return *p != before;
        }
        false
    }

    /// Unregister a plugin; returns `true` if an entry existed.
    pub fn unregister(&mut self, plugin_id: u32) -> bool {
        self.plugins.remove(&plugin_id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;

    #[test]
    fn registry_basic() {
        let mut reg = PluginRegistry::new();
        let info = PluginInfo { id: 0xdead_beef, name: "geo".into(), version: Version::parse("1.0.0").unwrap(), permissions: Permission::ACCESS_GEO };
        assert!(reg.register(&info).is_ok());
        assert!(reg.has_permission(0xdead_beef, Permission::ACCESS_GEO));
        assert!(!reg.has_permission(0xdead_beef, Permission::SEND_STREAM));
        assert!(reg.register(&info).is_err()); // duplicate
    }

    #[test]
    fn persistence() {
        use std::env::temp_dir;
        let mut reg = PluginRegistry::new();
        let info = PluginInfo { id: 1, name: "test".into(), version: Version::parse("1.0.0").unwrap(), permissions: Permission::SEND_STREAM | Permission::ACCESS_GEO };
        reg.register(&info).unwrap();
        let path = temp_dir().join("nyx_plugin_test.cbor");
        reg.save_to(&path).unwrap();
        let loaded = PluginRegistry::load_from(&path).unwrap();
        assert!(loaded.has_permission(1, Permission::ACCESS_GEO));
    }

    #[test]
    fn revoke_perm() {
        let mut reg = PluginRegistry::new();
        let info = PluginInfo { id: 2, name: "geo".into(), version: Version::parse("1.0.0").unwrap(), permissions: Permission::ACCESS_GEO | Permission::SEND_STREAM };
        reg.register(&info).unwrap();
        assert!(reg.revoke(2, Permission::ACCESS_GEO));
        assert!(!reg.has_permission(2, Permission::ACCESS_GEO));
        assert!(reg.has_permission(2, Permission::SEND_STREAM));
    }
} 