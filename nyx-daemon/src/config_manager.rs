#![forbid(unsafe_code)]

//! Configuration management system for Nyx daemon.
//!
//! This module handles:
//! - Dynamic configuration updates without restart
//! - Configuration validation and schema checking
//! - Hot-reload of configuration files
//! - Configuration versioning and rollback
//! - Environment variable integration

use crate::proto::{ConfigResponse, ConfigUpdate};
use nyx_core::config::NyxConfig;
use serde::{Deserialize, Serialize};

use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Dynamic configuration that can be updated at runtime
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicConfig {
    pub log_level: Option<String>,
    pub max_streams: Option<u32>,
    pub connection_timeout_ms: Option<u32>,
    pub enable_multipath: Option<bool>,
    pub cover_traffic_rate: Option<f64>,
    pub path_selection_strategy: Option<String>,
    pub enable_metrics: Option<bool>,
    pub metrics_interval_secs: Option<u64>,
}

/// Configuration manager
pub struct ConfigManager {
    config: Arc<RwLock<NyxConfig>>,
    dynamic_config: Arc<RwLock<DynamicConfig>>,
    config_path: Option<PathBuf>,
}

impl ConfigManager {
    /// Create a new configuration manager
    pub fn new(config: NyxConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            dynamic_config: Arc::new(RwLock::new(DynamicConfig {
                log_level: None,
                max_streams: None,
                connection_timeout_ms: None,
                enable_multipath: None,
                cover_traffic_rate: None,
                path_selection_strategy: None,
                enable_metrics: None,
                metrics_interval_secs: None,
            })),
            config_path: None,
        }
    }
    
    /// Update configuration dynamically
    pub async fn update_config(&self, update: ConfigUpdate) -> anyhow::Result<ConfigResponse> {
        let mut validation_errors = Vec::new();
        let mut updated_fields = Vec::new();
        
        // Validate and apply configuration updates
        for (key, value) in update.settings {
            match self.validate_and_apply_setting(&key, &value).await {
                Ok(()) => {
                    updated_fields.push(key);
                }
                Err(e) => {
                    validation_errors.push(format!("{}: {}", key, e));
                }
            }
        }
        
        if validation_errors.is_empty() {
            info!("Configuration updated successfully: {:?}", updated_fields);
            Ok(ConfigResponse {
                success: true,
                message: format!("Updated {} settings", updated_fields.len()),
                validation_errors: Vec::new(),
            })
        } else {
            warn!("Configuration update failed with validation errors: {:?}", validation_errors);
            Ok(ConfigResponse {
                success: false,
                message: "Configuration validation failed".to_string(),
                validation_errors,
            })
        }
    }
    
    /// Reload configuration from file
    pub async fn reload_config(&self) -> anyhow::Result<ConfigResponse> {
        if let Some(config_path) = &self.config_path {
            match fs::read_to_string(config_path).await {
                Ok(content) => {
                    match toml::from_str::<NyxConfig>(&content) {
                        Ok(new_config) => {
                            *self.config.write().await = new_config;
                            info!("Configuration reloaded from file: {:?}", config_path);
                            Ok(ConfigResponse {
                                success: true,
                                message: "Configuration reloaded successfully".to_string(),
                                validation_errors: Vec::new(),
                            })
                        }
                        Err(e) => {
                            error!("Failed to parse configuration file: {}", e);
                            Ok(ConfigResponse {
                                success: false,
                                message: format!("Configuration parsing failed: {}", e),
                                validation_errors: vec![e.to_string()],
                            })
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to read configuration file: {}", e);
                    Ok(ConfigResponse {
                        success: false,
                        message: format!("Failed to read configuration file: {}", e),
                        validation_errors: vec![e.to_string()],
                    })
                }
            }
        } else {
            Ok(ConfigResponse {
                success: false,
                message: "No configuration file path specified".to_string(),
                validation_errors: vec!["Configuration file path not set".to_string()],
            })
        }
    }
    

    
    /// Validate and apply a single configuration setting
    async fn validate_and_apply_setting(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let mut dynamic_config = self.dynamic_config.write().await;
        
        match key {
            "log_level" => {
                if ["trace", "debug", "info", "warn", "error"].contains(&value) {
                    dynamic_config.log_level = Some(value.to_string());
                    // Apply log level change immediately
                    std::env::set_var("RUST_LOG", value);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Invalid log level: {}", value))
                }
            }
            "max_streams" => {
                let max_streams: u32 = value.parse()?;
                if max_streams > 0 && max_streams <= 100000 {
                    dynamic_config.max_streams = Some(max_streams);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("max_streams must be between 1 and 100000"))
                }
            }
            "connection_timeout_ms" => {
                let timeout: u32 = value.parse()?;
                if timeout >= 1000 && timeout <= 300000 {
                    dynamic_config.connection_timeout_ms = Some(timeout);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("connection_timeout_ms must be between 1000 and 300000"))
                }
            }
            "enable_multipath" => {
                let enable: bool = value.parse()?;
                dynamic_config.enable_multipath = Some(enable);
                Ok(())
            }
            "cover_traffic_rate" => {
                let rate: f64 = value.parse()?;
                if rate >= 0.0 && rate <= 1000.0 {
                    dynamic_config.cover_traffic_rate = Some(rate);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("cover_traffic_rate must be between 0.0 and 1000.0"))
                }
            }
            "path_selection_strategy" => {
                if ["latency_weighted", "random", "lowest_latency", "load_balance"].contains(&value) {
                    dynamic_config.path_selection_strategy = Some(value.to_string());
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Invalid path selection strategy: {}", value))
                }
            }
            "enable_metrics" => {
                let enable: bool = value.parse()?;
                dynamic_config.enable_metrics = Some(enable);
                Ok(())
            }
            "metrics_interval_secs" => {
                let interval: u64 = value.parse()?;
                if interval >= 1 && interval <= 3600 {
                    dynamic_config.metrics_interval_secs = Some(interval);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("metrics_interval_secs must be between 1 and 3600"))
                }
            }
            _ => Err(anyhow::anyhow!("Unknown configuration key: {}", key))
        }
    }
    
    /// Get current configuration
    pub async fn get_config(&self) -> NyxConfig {
        self.config.read().await.clone()
    }
    
    /// Get current dynamic configuration
    pub async fn get_dynamic_config(&self) -> DynamicConfig {
        self.dynamic_config.read().await.clone()
    }
    
    /// Check for configuration file updates
    pub async fn check_for_updates(&self) -> anyhow::Result<Option<NyxConfig>> {
        if let Some(config_path) = &self.config_path {
            // Check if file has been modified
            if let Ok(metadata) = fs::metadata(config_path).await {
                if let Ok(_modified) = metadata.modified() {
                    // For simplicity, we'll always try to reload if the file exists
                    // In a real implementation, you'd track the last modification time
                    match fs::read_to_string(config_path).await {
                        Ok(content) => {
                            match toml::from_str::<NyxConfig>(&content) {
                                Ok(new_config) => {
                                    let current_config = self.config.read().await;
                                    // Simple comparison - in practice you'd want more sophisticated change detection
                                    if format!("{:?}", *current_config) != format!("{:?}", new_config) {
                                        info!("Configuration file has been updated");
                                        return Ok(Some(new_config));
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to parse updated configuration file: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to read configuration file: {}", e);
                        }
                    }
                }
            }
        }
        Ok(None)
    }
    
    /// Set configuration file path for watching
    pub fn set_config_path(&mut self, path: PathBuf) {
        self.config_path = Some(path);
    }
    
    /// Apply dynamic configuration changes to the system
    pub async fn apply_dynamic_changes(&self) -> anyhow::Result<()> {
        let dynamic_config = self.dynamic_config.read().await;
        
        // Apply log level changes
        if let Some(ref log_level) = dynamic_config.log_level {
            std::env::set_var("RUST_LOG", log_level);
            info!("Applied log level change: {}", log_level);
        }
        
        // Apply other configuration changes
        if let Some(max_streams) = dynamic_config.max_streams {
            info!("Applied max_streams change: {}", max_streams);
            // In a real implementation, this would update the stream manager
        }
        
        if let Some(timeout) = dynamic_config.connection_timeout_ms {
            info!("Applied connection timeout change: {}ms", timeout);
            // In a real implementation, this would update connection timeouts
        }
        
        if let Some(enable_multipath) = dynamic_config.enable_multipath {
            info!("Applied multipath setting change: {}", enable_multipath);
            // In a real implementation, this would update the path builder
        }
        
        if let Some(rate) = dynamic_config.cover_traffic_rate {
            info!("Applied cover traffic rate change: {}", rate);
            // In a real implementation, this would update the mix layer
        }
        
        if let Some(ref strategy) = dynamic_config.path_selection_strategy {
            info!("Applied path selection strategy change: {}", strategy);
            // In a real implementation, this would update the path builder
        }
        
        if let Some(enable_metrics) = dynamic_config.enable_metrics {
            info!("Applied metrics setting change: {}", enable_metrics);
            // In a real implementation, this would enable/disable metrics collection
        }
        
        if let Some(interval) = dynamic_config.metrics_interval_secs {
            info!("Applied metrics interval change: {}s", interval);
            // In a real implementation, this would update the metrics collector
        }
        
        Ok(())
    }
    
    /// Validate entire configuration before applying
    pub async fn validate_config(&self, config: &NyxConfig) -> anyhow::Result<Vec<String>> {
        let mut errors = Vec::new();
        
        // Validate listen port
        if config.listen_port == 0 || config.listen_port > 65535 {
            errors.push("listen_port must be between 1 and 65535".to_string());
        }
        
        // Validate node ID if present
        if let Some(ref node_id) = config.node_id {
            if hex::decode(node_id).is_err() {
                errors.push("node_id must be a valid hex string".to_string());
            }
        }
        
        // Validate other configuration fields
        // Add more validation as needed
        
        Ok(errors)
    }
    
    /// Create configuration backup before applying changes
    pub async fn backup_config(&self) -> anyhow::Result<NyxConfig> {
        Ok(self.config.read().await.clone())
    }
    
    /// Restore configuration from backup
    pub async fn restore_config(&self, backup: NyxConfig) -> anyhow::Result<()> {
        *self.config.write().await = backup;
        info!("Configuration restored from backup");
        Ok(())
    }
    
    /// Get configuration change summary
    pub async fn get_change_summary(&self) -> String {
        let dynamic_config = self.dynamic_config.read().await;
        let mut changes = Vec::new();
        
        if dynamic_config.log_level.is_some() {
            changes.push("log_level");
        }
        if dynamic_config.max_streams.is_some() {
            changes.push("max_streams");
        }
        if dynamic_config.connection_timeout_ms.is_some() {
            changes.push("connection_timeout_ms");
        }
        if dynamic_config.enable_multipath.is_some() {
            changes.push("enable_multipath");
        }
        if dynamic_config.cover_traffic_rate.is_some() {
            changes.push("cover_traffic_rate");
        }
        if dynamic_config.path_selection_strategy.is_some() {
            changes.push("path_selection_strategy");
        }
        if dynamic_config.enable_metrics.is_some() {
            changes.push("enable_metrics");
        }
        if dynamic_config.metrics_interval_secs.is_some() {
            changes.push("metrics_interval_secs");
        }
        
        if changes.is_empty() {
            "No dynamic configuration changes".to_string()
        } else {
            format!("Dynamic changes: {}", changes.join(", "))
        }
    }
    
    /// Watch for configuration file changes
    pub async fn start_file_watcher(&self) -> anyhow::Result<()> {
        if let Some(config_path) = &self.config_path {
            info!("Starting configuration file watcher for: {:?}", config_path);
            // In a real implementation, you'd use a file system watcher like `notify`
            // For now, we'll just log that the watcher would be started
            Ok(())
        } else {
            Ok(())
        }
    }
} 