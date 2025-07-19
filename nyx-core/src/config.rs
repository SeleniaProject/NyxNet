#![forbid(unsafe_code)]

//! Nyx configuration handling. Parses a TOML file into a strongly-typed structure and supports
//! hot-reloading via the `notify` crate. All public APIs are `async`-ready but do not impose an
//! async runtime themselves.

use serde::Deserialize;
use std::{fs, path::Path, sync::Arc};
use tokio::sync::watch;
use notify::{RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher, Event, EventKind};

use crate::NyxError;

/// Primary configuration structure shared across Nyx components.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct NyxConfig {
    /// Optional node identifier. If omitted a random value will be generated at startup.
    pub node_id: Option<String>,

    /// Logging verbosity (`error`, `warn`, `info`, `debug`, `trace`).
    pub log_level: Option<String>,

    /// UDP listen port for incoming Nyx traffic.
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
}

impl Default for NyxConfig {
    fn default() -> Self {
        Self {
            node_id: None,
            log_level: Some("info".to_string()),
            listen_port: default_listen_port(),
        }
    }
}

fn default_listen_port() -> u16 {
    43300
}

impl NyxConfig {
    /// Load a configuration file from the given path.
    pub fn from_file<P: AsRef<Path>>(path: P) -> crate::NyxResult<Self> {
        let data = fs::read_to_string(&path).map_err(NyxError::from)?;
        let cfg = toml::from_str::<NyxConfig>(&data).map_err(NyxError::ConfigParse)?;
        Ok(cfg)
    }

    /// Load config alias version
    pub fn load<P: AsRef<Path>>(path: P) -> crate::NyxResult<Self> {
        Self::from_file(path)
    }

    /// Watch the configuration file for changes and receive updates through a watch channel.
    ///
    /// Returns the initial configuration and a [`watch::Receiver`] that yields a new [`NyxConfig`]
    /// wrapped in [`Arc`] every time the file is modified on disk.
    pub fn watch_file<P: AsRef<Path>>(path: P) -> crate::NyxResult<(Arc<NyxConfig>, watch::Receiver<Arc<NyxConfig>>)> {
        let path_buf = path.as_ref().to_path_buf();
        let initial_cfg = Arc::new(Self::from_file(&path_buf)?);
        // Clone for closure capture to avoid moving the original `path_buf`.
        let path_in_closure = path_buf.clone();
        let (tx, rx) = watch::channel::<Arc<NyxConfig>>(initial_cfg.clone());

        // `notify` requires the watcher to stay alive for as long as we want events. We therefore
        // spawn it on a background task and intentionally leak it so that it lives for the process
        // lifetime. This avoids polluting the public API with a guard type the caller must hold.
        let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |res: NotifyResult<Event>| {
            if let Ok(event) = res {
                // Interested only in content modifications.
                if matches!(event.kind, EventKind::Modify(_)) {
                    if let Ok(updated) = Self::from_file(&path_in_closure) {
                        let _ = tx.send(Arc::new(updated));
                    }
                }
            }
        })?;

        watcher.watch(&path_buf, RecursiveMode::NonRecursive)?;
        // Leak the watcher so it keeps running. Safe because it lives for the entire program.
        std::mem::forget(watcher);

        Ok((initial_cfg, rx))
    }
} 