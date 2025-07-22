#![forbid(unsafe_code)]

//! Plugin frame dispatcher with permission enforcement.
//!
//! The dispatcher is responsible for routing incoming Plugin Frames
//! (Type 0x50–0x5F) to the appropriate runtime while ensuring that
//! the sending plugin has been granted the requested permissions.
//!
//! For now a **single-process in-memory sandbox** is used – each plugin
//! runs inside its own [`tokio::task`] and communicates via mpsc channel.
//! OS-level isolation (seccomp / JobObject) can be layered beneath this
//! abstraction later.

use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::warn;

#[cfg(feature = "dynamic_plugin")]
use libloading::Library;

use crate::{plugin_registry::{PluginRegistry, Permission}, plugin::PluginHeader};
use crate::plugin_registry::PluginInfo;
use nyx_telemetry::record_plugin_frame;

/// Message sent to a plugin runtime.
#[derive(Debug)]
pub struct PluginMessage {
    pub header: PluginHeader<'static>,
}

/// Handle to a running plugin instance.
struct RuntimeHandle {
    tx: mpsc::Sender<PluginMessage>,
    #[cfg(feature = "dynamic_plugin")]
    _lib: Option<Library>,
}

/// Central dispatcher mapping plugin IDs → runtime handles.
pub struct PluginDispatcher {
    registry: PluginRegistry,
    runtimes: HashMap<u32, RuntimeHandle>,
}

impl PluginDispatcher {
    #[must_use]
    pub fn new(registry: PluginRegistry) -> Self {
        Self { registry, runtimes: HashMap::new() }
    }

    /// Register a runtime channel for `plugin_id`.
    pub fn attach_runtime(&mut self, plugin_id: u32, tx: mpsc::Sender<PluginMessage>) {
        self.runtimes.insert(plugin_id, RuntimeHandle { tx, _lib: None });
    }

    /// Load a plugin and attach its runtime to dispatcher.
    /// On builds with the `dynamic_plugin` feature this takes an additional `lib` parameter
    /// holding the opened dynamic library handle so the library remains resident for the
    /// lifetime of the plugin. On other builds the plugin is assumed to be statically linked
    /// or otherwise already resident.

    #[cfg(feature = "dynamic_plugin")]
    pub fn load_plugin(&mut self, info: PluginInfo, tx: mpsc::Sender<PluginMessage>, lib: Library) -> Result<(), ()> {
        self.registry.register(&info)?;
        self.runtimes.insert(info.id, RuntimeHandle { tx, _lib: Some(lib) });
        Ok(())
    }

    #[cfg(not(feature = "dynamic_plugin"))]
    pub fn load_plugin(&mut self, info: PluginInfo, tx: mpsc::Sender<PluginMessage>) -> Result<(), ()> {
        self.registry.register(&info)?;
        self.runtimes.insert(info.id, RuntimeHandle { tx });
        Ok(())
    }

    /// Unload plugin runtime and unregister.
    pub fn unload_plugin(&mut self, plugin_id: u32) {
        self.runtimes.remove(&plugin_id);
        self.registry.unregister(plugin_id);
    }

    /// Dispatch incoming raw plugin frame. Returns `Ok(())` when accepted or
    /// `Err(())` if permission denied, unknown runtime, or decode error.
    pub async fn dispatch(&self, frame_bytes: &[u8]) -> Result<(), ()> {
        let hdr = PluginHeader::decode(frame_bytes).map_err(|_| ())?;
        // Map to required permission (future: derive from hdr.flags)
        let required_perm = Permission::SEND_STREAM;
        if !self.registry.has_permission(hdr.id, required_perm) {
            warn!(id = hdr.id, "permission denied for plugin frame");
            return Err(());
        }
        if let Some(r) = self.runtimes.get(&hdr.id) {
            // Deep copy so lifetime 'static
            let owned = frame_bytes.to_vec();
            let hdr_owned = PluginHeader::decode(&owned).map_err(|_| ())?;
            record_plugin_frame(hdr.id, hdr_owned.data.len());
            let _ = r.tx.send(PluginMessage { header: hdr_owned }).await;
            Ok(())
        } else {
            warn!(id = hdr.id, "no runtime for plugin id");
            Err(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_geostat::{plugin_info, GeoStat};

    #[tokio::test]
    async fn deny_without_permission() {
        let info = plugin_info();
        let mut reg = PluginRegistry::new();
        reg.register(&info).unwrap();
        // Revoke permission to simulate policy change.
        reg.revoke(info.id, Permission::ACCESS_GEO);

        let disp = PluginDispatcher::new(reg);
        let frame = GeoStat { lat: 0.0, lon: 0.0, acc: 1.0 }.build_frame();
        assert!(disp.dispatch(&frame).await.is_err());
    }
}