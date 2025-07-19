//! Runtime SETTINGS values and watcher.
#![forbid(unsafe_code)]

use tokio::sync::{watch, watch::Receiver};

use crate::management::{SettingsFrame, Setting};

/// Mutable stream-layer configuration updated via SETTINGS frames.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamSettings {
    pub max_streams: u32,
    pub max_data: u32,
    pub idle_timeout: u16,
    pub pq_supported: bool,
}

impl Default for StreamSettings {
    fn default() -> Self {
        Self {
            max_streams: 256,
            max_data: 1_048_576,
            idle_timeout: 30,
            pq_supported: false,
        }
    }
}

impl StreamSettings {
    /// Apply values from a received SETTINGS frame.
    pub fn apply(&mut self, frame: &SettingsFrame) {
        for s in &frame.settings {
            match s.id {
                0x01 => self.max_streams = s.value,
                0x02 => self.max_data = s.value,
                0x03 => self.idle_timeout = s.value as u16,
                0x04 => self.pq_supported = s.value != 0,
                _ => {}
            }
        }
    }

    /// Build a SETTINGS frame representing current config.
    pub fn to_frame(&self) -> SettingsFrame {
        let mut settings = Vec::<Setting>::new();
        settings.push(Setting { id: 0x01, value: self.max_streams });
        settings.push(Setting { id: 0x02, value: self.max_data });
        settings.push(Setting { id: 0x03, value: self.idle_timeout as u32 });
        settings.push(Setting { id: 0x04, value: if self.pq_supported { 1 } else { 0 } });
        SettingsFrame { settings }
    }
}

/// Create a watch channel seeded with default settings.
pub fn settings_watch() -> (watch::Sender<StreamSettings>, Receiver<StreamSettings>) {
    watch::channel(StreamSettings::default())
} 