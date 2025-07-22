//! Sample Plugin: GeoStat collection
//!
//! This plugin periodically reports coarse geolocation to peers. The payload is
//! encoded as CBOR `{lat:f64, lon:f64, acc:f64}` (WGS-84 degrees, accuracy in
//! meters). The plugin uses Nyx Plugin Frame (Type 0x50–0x5F) with a fixed
//! 32-bit ID.
//!
//! Geolocation retrieval is platform-specific; here we expose a helper to build
//! frames from already-known coordinates so callers can integrate OS APIs.
//!
//! Permission requirement: `ACCESS_GEO`.

#![forbid(unsafe_code)]

use serde::{Serialize, Deserialize};
use serde_cbor;
use semver::Version;
use super::{PluginHeader, PluginInfo, Permission};

/// Assigned plugin ID (0x47454F53 = 'GEOS').
pub const GEO_PLUGIN_ID: u32 = 0x4745_4F53;

/// CBOR-encoded geolocation payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeoStat {
    pub lat: f64,
    pub lon: f64,
    pub acc: f64, // accuracy (meters)
}

impl GeoStat {
    /// Build a Plugin Frame containing this geolocation sample.
    /// Returns raw bytes ready to be inserted after Nyx base header.
    pub fn build_frame(&self) -> Vec<u8> {
        let data = serde_cbor::to_vec(self).expect("cbor encode");
        let hdr = PluginHeader { id: GEO_PLUGIN_ID, flags: 0, data: &data };
        hdr.encode()
    }

    /// Decode from plugin payload data slice.
    pub fn parse_frame(bytes: &[u8]) -> Result<Self, serde_cbor::Error> {
        let hdr = PluginHeader::decode(bytes)?;
        serde_cbor::from_slice(hdr.data)
    }
}

/// Return the [`PluginInfo`] metadata required during registration.
#[must_use]
pub fn plugin_info() -> PluginInfo {
    PluginInfo {
        id: GEO_PLUGIN_ID,
        name: "GeoStat".into(),
        version: Version::new(1, 0, 0),
        permissions: Permission::ACCESS_GEO,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let g = GeoStat { lat: 35.0, lon: 139.0, acc: 15.0 };
        let bytes = g.build_frame();
        let parsed = GeoStat::parse_frame(&bytes).unwrap();
        assert_eq!(g, parsed);
    }
} 