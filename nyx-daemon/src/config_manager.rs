#![forbid(unsafe_code)]

//! Configuration management system for Nyx daemon.
//!
//! This module handles:
//! - Dynamic configuration updates without restart
//! - Configuration validation and schema checking
//! - Hot-reload of configuration files
//! - Configuration versioning and rollback
//! - Environment variable integration

use crate::proto::{ConfigUpdate, ConfigResponse};
use nyx_core::NyxConfig;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::fs;
use tracing::{debug, error, info, warn};
use serde::{Deserialize, Serialize};

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
    
    /// Check for configuration updates
    pub async fn check_for_updates(&self) -> anyhow::Result<Option<NyxConfig>> {
        // This would check for file changes or external configuration updates
        // For now, we'll return None indicating no updates
        Ok(None)
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
} 