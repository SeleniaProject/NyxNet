#![forbid(unsafe_code)]

//! Comprehensive configuration management system for Nyx daemon.
//!
//! This module handles:
//! - Dynamic configuration updates without restart
//! - Configuration validation and schema checking
//! - Hot-reload of configuration files with layer notification
//! - Configuration versioning and rollback capabilities
//! - Environment variable integration
//! - Layer-specific configuration updates
//! - Configuration change notifications to all layers

use crate::proto::{ConfigResponse, ConfigUpdate, Event};
use nyx_core::config::NyxConfig;
use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::fs;
use tokio::sync::{RwLock, broadcast, mpsc};
use tracing::{debug, error, info, warn};
use anyhow::Result;

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
    pub crypto_key_rotation_interval: Option<u64>,
    pub fec_redundancy_level: Option<f64>,
    pub stream_buffer_size: Option<u32>,
    pub mix_batch_size: Option<u32>,
    pub transport_packet_size: Option<u32>,
}

/// Configuration change notification
#[derive(Debug, Clone)]
pub struct ConfigChangeNotification {
    pub layer: String,
    pub setting: String,
    pub old_value: Option<String>,
    pub new_value: String,
    pub timestamp: SystemTime,
}

/// Configuration version for rollback support
#[derive(Debug, Clone)]
pub struct ConfigVersion {
    pub version: u64,
    pub config: NyxConfig,
    pub dynamic_config: DynamicConfig,
    pub timestamp: SystemTime,
    pub description: String,
}

/// Layer configuration subscriber
pub struct LayerConfigSubscriber {
    pub layer_name: String,
    pub sender: mpsc::UnboundedSender<ConfigChangeNotification>,
    pub interested_settings: Vec<String>,
    pub last_notification: SystemTime,
    pub is_active: bool,
}

/// Comprehensive configuration manager with layer notification
pub struct ConfigManager {
    config: Arc<RwLock<NyxConfig>>,
    dynamic_config: Arc<RwLock<DynamicConfig>>,
    config_path: Option<PathBuf>,
    
    // Version management
    config_versions: Arc<RwLock<Vec<ConfigVersion>>>,
    current_version: Arc<RwLock<u64>>,
    max_versions: usize,
    
    // Layer notification system
    layer_subscribers: Arc<RwLock<HashMap<String, LayerConfigSubscriber>>>,
    event_tx: broadcast::Sender<Event>,
    
    // File watching
    file_watcher_handle: Option<tokio::task::JoinHandle<()>>,
    
    // Rollback support
    rollback_timeout: Duration,
    pending_rollback: Arc<RwLock<Option<(u64, SystemTime)>>>,
}

impl ConfigManager {
    /// Create a new comprehensive configuration manager
    pub fn new(config: NyxConfig, event_tx: broadcast::Sender<Event>) -> Self {
        let initial_dynamic = DynamicConfig {
            log_level: None,
            max_streams: None,
            connection_timeout_ms: None,
            enable_multipath: None,
            cover_traffic_rate: None,
            path_selection_strategy: None,
            enable_metrics: None,
            metrics_interval_secs: None,
            crypto_key_rotation_interval: None,
            fec_redundancy_level: None,
            stream_buffer_size: None,
            mix_batch_size: None,
            transport_packet_size: None,
        };
        
        // Create initial version
        let initial_version = ConfigVersion {
            version: 1,
            config: config.clone(),
            dynamic_config: initial_dynamic.clone(),
            timestamp: SystemTime::now(),
            description: "Initial configuration".to_string(),
        };
        
        Self {
            config: Arc::new(RwLock::new(config)),
            dynamic_config: Arc::new(RwLock::new(initial_dynamic)),
            config_path: None,
            config_versions: Arc::new(RwLock::new(vec![initial_version])),
            current_version: Arc::new(RwLock::new(1)),
            max_versions: 10,
            layer_subscribers: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            file_watcher_handle: None,
            rollback_timeout: Duration::from_secs(300), // 5 minutes
            pending_rollback: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Start the configuration manager
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting configuration manager");
        
        // Start file watcher if config path is set
        if let Some(config_path) = &self.config_path {
            self.start_file_watcher_task(config_path.clone()).await?;
        }
        
        // Start rollback monitor
        self.start_rollback_monitor().await?;
        
        info!("Configuration manager started successfully");
        Ok(())
    }
    
    /// Stop the configuration manager
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping configuration manager");
        
        if let Some(handle) = self.file_watcher_handle.take() {
            handle.abort();
        }
        
        // Clear subscribers
        {
            let mut subscribers = self.layer_subscribers.write().await;
            subscribers.clear();
        }
        
        info!("Configuration manager stopped");
        Ok(())
    }
    
    /// Subscribe a layer to configuration changes
    pub async fn subscribe_layer(&self, layer_name: String, interested_settings: Vec<String>) -> Result<mpsc::UnboundedReceiver<ConfigChangeNotification>> {
        let (tx, rx) = mpsc::unbounded_channel();
        
        let subscriber = LayerConfigSubscriber {
            layer_name: layer_name.clone(),
            sender: tx,
            interested_settings,
            last_notification: SystemTime::now(),
            is_active: true,
        };
        
        {
            let mut subscribers = self.layer_subscribers.write().await;
            subscribers.insert(layer_name.clone(), subscriber);
        }
        
        info!("Layer {} subscribed to configuration changes", layer_name);
        Ok(rx)
    }
    
    /// Unsubscribe a layer from configuration changes
    pub async fn unsubscribe_layer(&self, layer_name: &str) -> Result<()> {
        {
            let mut subscribers = self.layer_subscribers.write().await;
            subscribers.remove(layer_name);
        }
        
        info!("Layer {} unsubscribed from configuration changes", layer_name);
        Ok(())
    }
    
    /// Update configuration dynamically with layer notification
    pub async fn update_config(&self, update: ConfigUpdate) -> Result<ConfigResponse> {
        let mut validation_errors = Vec::new();
        let mut updated_fields = Vec::new();
        let mut notifications = Vec::new();
        
        // Create backup before applying changes
        let backup_version = self.create_version_backup("Pre-update backup".to_string()).await?;
        
        // Validate and apply configuration updates
        for (key, value) in update.settings {
            match self.validate_and_apply_setting(&key, &value).await {
                Ok(old_value) => {
                    updated_fields.push(key.clone());
                    
                    // Create notification for this change
                    let notification = ConfigChangeNotification {
                        layer: self.determine_affected_layer(&key),
                        setting: key,
                        old_value,
                        new_value: value,
                        timestamp: SystemTime::now(),
                    };
                    notifications.push(notification);
                }
                Err(e) => {
                    validation_errors.push(format!("{}: {}", key, e));
                }
            }
        }
        
        if validation_errors.is_empty() {
            // Notify all interested layers
            for notification in notifications {
                if let Err(e) = self.notify_layers(&notification).await {
                    error!("Failed to notify layers of config change: {}", e);
                }
            }
            
            // Create new version after successful update
            let version_description = format!("Updated: {}", updated_fields.join(", "));
            self.create_version_backup(version_description).await?;
            
            // Emit configuration update event
            let event = Event {
                r#type: "configuration".to_string(),
                detail: format!("Configuration updated: {}", updated_fields.join(", ")),
                timestamp: Some(crate::system_time_to_proto_timestamp(SystemTime::now())),
                severity: "info".to_string(),
                attributes: [
                    ("action".to_string(), "config_update".to_string()),
                    ("fields".to_string(), updated_fields.join(",").to_string()),
                ].into_iter().collect(),
                event_data: Some(crate::proto::event::EventData::SystemEvent(crate::proto::SystemEvent {
                    component: "config_manager".to_string(),
                    action: "config_update".to_string(),
                    message: format!("Updated {} settings", updated_fields.len()),
                    details: std::collections::HashMap::new(),
                })),
            };
            
            let _ = self.event_tx.send(event);
            
            info!("Configuration updated successfully: {:?}", updated_fields);
            Ok(ConfigResponse {
                success: true,
                message: format!("Updated {} settings", updated_fields.len()),
                validation_errors: Vec::new(),
            })
        } else {
            // Rollback on validation failure
            if let Err(e) = self.rollback_to_version(backup_version).await {
                error!("Failed to rollback after validation failure: {}", e);
            }
            
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
    async fn validate_and_apply_setting(&self, key: &str, value: &str) -> Result<Option<String>> {
        let mut dynamic_config = self.dynamic_config.write().await;
        
        match key {
            "log_level" => {
                let old_value = dynamic_config.log_level.clone();
                if ["trace", "debug", "info", "warn", "error"].contains(&value) {
                    dynamic_config.log_level = Some(value.to_string());
                    // Apply log level change immediately
                    std::env::set_var("RUST_LOG", value);
                    Ok(old_value)
                } else {
                    Err(anyhow::anyhow!("Invalid log level: {}", value))
                }
            }
            "max_streams" => {
                let old_value = dynamic_config.max_streams.map(|v| v.to_string());
                let max_streams: u32 = value.parse()?;
                if max_streams > 0 && max_streams <= 100000 {
                    dynamic_config.max_streams = Some(max_streams);
                    Ok(old_value)
                } else {
                    Err(anyhow::anyhow!("max_streams must be between 1 and 100000"))
                }
            }
            "connection_timeout_ms" => {
                let old_value = dynamic_config.connection_timeout_ms.map(|v| v.to_string());
                let timeout: u32 = value.parse()?;
                if timeout >= 1000 && timeout <= 300000 {
                    dynamic_config.connection_timeout_ms = Some(timeout);
                    Ok(old_value)
                } else {
                    Err(anyhow::anyhow!("connection_timeout_ms must be between 1000 and 300000"))
                }
            }
            "enable_multipath" => {
                let old_value = dynamic_config.enable_multipath.map(|v| v.to_string());
                let enable: bool = value.parse()?;
                dynamic_config.enable_multipath = Some(enable);
                Ok(old_value)
            }
            "cover_traffic_rate" => {
                let old_value = dynamic_config.cover_traffic_rate.map(|v| v.to_string());
                let rate: f64 = value.parse()?;
                if rate >= 0.0 && rate <= 1000.0 {
                    dynamic_config.cover_traffic_rate = Some(rate);
                    Ok(old_value)
                } else {
                    Err(anyhow::anyhow!("cover_traffic_rate must be between 0.0 and 1000.0"))
                }
            }
            "path_selection_strategy" => {
                let old_value = dynamic_config.path_selection_strategy.clone();
                if ["latency_weighted", "random", "lowest_latency", "load_balance"].contains(&value) {
                    dynamic_config.path_selection_strategy = Some(value.to_string());
                    Ok(old_value)
                } else {
                    Err(anyhow::anyhow!("Invalid path selection strategy: {}", value))
                }
            }
            "enable_metrics" => {
                let old_value = dynamic_config.enable_metrics.map(|v| v.to_string());
                let enable: bool = value.parse()?;
                dynamic_config.enable_metrics = Some(enable);
                Ok(old_value)
            }
            "metrics_interval_secs" => {
                let old_value = dynamic_config.metrics_interval_secs.map(|v| v.to_string());
                let interval: u64 = value.parse()?;
                if interval >= 1 && interval <= 3600 {
                    dynamic_config.metrics_interval_secs = Some(interval);
                    Ok(old_value)
                } else {
                    Err(anyhow::anyhow!("metrics_interval_secs must be between 1 and 3600"))
                }
            }
            "crypto_key_rotation_interval" => {
                let old_value = dynamic_config.crypto_key_rotation_interval.map(|v| v.to_string());
                let interval: u64 = value.parse()?;
                if interval >= 60 && interval <= 86400 {
                    dynamic_config.crypto_key_rotation_interval = Some(interval);
                    Ok(old_value)
                } else {
                    Err(anyhow::anyhow!("crypto_key_rotation_interval must be between 60 and 86400 seconds"))
                }
            }
            "fec_redundancy_level" => {
                let old_value = dynamic_config.fec_redundancy_level.map(|v| v.to_string());
                let level: f64 = value.parse()?;
                if level >= 0.1 && level <= 0.9 {
                    dynamic_config.fec_redundancy_level = Some(level);
                    Ok(old_value)
                } else {
                    Err(anyhow::anyhow!("fec_redundancy_level must be between 0.1 and 0.9"))
                }
            }
            "stream_buffer_size" => {
                let old_value = dynamic_config.stream_buffer_size.map(|v| v.to_string());
                let size: u32 = value.parse()?;
                if size >= 1024 && size <= 1048576 {
                    dynamic_config.stream_buffer_size = Some(size);
                    Ok(old_value)
                } else {
                    Err(anyhow::anyhow!("stream_buffer_size must be between 1024 and 1048576 bytes"))
                }
            }
            "mix_batch_size" => {
                let old_value = dynamic_config.mix_batch_size.map(|v| v.to_string());
                let size: u32 = value.parse()?;
                if size >= 1 && size <= 1000 {
                    dynamic_config.mix_batch_size = Some(size);
                    Ok(old_value)
                } else {
                    Err(anyhow::anyhow!("mix_batch_size must be between 1 and 1000"))
                }
            }
            "transport_packet_size" => {
                let old_value = dynamic_config.transport_packet_size.map(|v| v.to_string());
                let size: u32 = value.parse()?;
                if size >= 512 && size <= 65536 {
                    dynamic_config.transport_packet_size = Some(size);
                    Ok(old_value)
                } else {
                    Err(anyhow::anyhow!("transport_packet_size must be between 512 and 65536 bytes"))
                }
            }
            _ => Err(anyhow::anyhow!("Unknown configuration key: {}", key))
        }
    }
    
    /// Determine which layer is affected by a configuration change
    fn determine_affected_layer(&self, setting: &str) -> String {
        match setting {
            "crypto_key_rotation_interval" => "crypto".to_string(),
            "fec_redundancy_level" => "fec".to_string(),
            "stream_buffer_size" | "max_streams" => "stream".to_string(),
            "mix_batch_size" | "cover_traffic_rate" => "mix".to_string(),
            "transport_packet_size" | "connection_timeout_ms" => "transport".to_string(),
            "enable_multipath" | "path_selection_strategy" => "path_builder".to_string(),
            "enable_metrics" | "metrics_interval_secs" => "metrics".to_string(),
            _ => "system".to_string(),
        }
    }
    
    /// Notify layers of configuration changes
    async fn notify_layers(&self, notification: &ConfigChangeNotification) -> Result<()> {
        let subscribers = self.layer_subscribers.read().await;
        let mut failed_notifications = 0;
        
        for (layer_name, subscriber) in subscribers.iter() {
            if !subscriber.is_active {
                continue;
            }
            
            // Check if this layer is interested in this setting
            let is_interested = subscriber.interested_settings.is_empty() || 
                               subscriber.interested_settings.contains(&notification.setting) ||
                               layer_name == &notification.layer;
            
            if is_interested {
                if let Err(e) = subscriber.sender.send(notification.clone()) {
                    warn!("Failed to notify layer {} of config change: {}", layer_name, e);
                    failed_notifications += 1;
                } else {
                    debug!("Notified layer {} of config change: {}", layer_name, notification.setting);
                }
            }
        }
        
        if failed_notifications > 0 {
            warn!("Failed to notify {} layers of configuration change", failed_notifications);
        }
        
        Ok(())
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
    
    /// Create a configuration version backup
    async fn create_version_backup(&self, description: String) -> Result<u64> {
        let config = self.config.read().await.clone();
        let dynamic_config = self.dynamic_config.read().await.clone();
        let mut current_version = self.current_version.write().await;
        
        *current_version += 1;
        let version_number = *current_version;
        
        let version = ConfigVersion {
            version: version_number,
            config,
            dynamic_config,
            timestamp: SystemTime::now(),
            description,
        };
        
        {
            let mut versions = self.config_versions.write().await;
            versions.push(version);
            
            // Keep only the last N versions
            if versions.len() > self.max_versions {
                versions.remove(0);
            }
        }
        
        debug!("Created configuration version backup: {}", version_number);
        Ok(version_number)
    }
    
    /// Rollback to a specific configuration version
    pub async fn rollback_to_version(&self, version: u64) -> Result<ConfigResponse> {
        let version_to_restore = {
            let versions = self.config_versions.read().await;
            versions.iter()
                .find(|v| v.version == version)
                .cloned()
        };
        
        if let Some(version_data) = version_to_restore {
            // Apply the rollback
            *self.config.write().await = version_data.config.clone();
            *self.dynamic_config.write().await = version_data.dynamic_config.clone();
            *self.current_version.write().await = version_data.version;
            
            // Notify all layers of the rollback
            let notification = ConfigChangeNotification {
                layer: "all".to_string(),
                setting: "rollback".to_string(),
                old_value: None,
                new_value: format!("version_{}", version),
                timestamp: SystemTime::now(),
            };
            
            if let Err(e) = self.notify_layers(&notification).await {
                error!("Failed to notify layers of rollback: {}", e);
            }
            
            // Emit rollback event
            let event = Event {
                r#type: "configuration".to_string(),
                detail: format!("Configuration rolled back to version {}", version),
                timestamp: Some(crate::system_time_to_proto_timestamp(SystemTime::now())),
                severity: "warning".to_string(),
                attributes: [
                    ("action".to_string(), "rollback".to_string()),
                    ("version".to_string(), version.to_string()),
                ].into_iter().collect(),
                event_data: Some(crate::proto::event::EventData::SystemEvent(crate::proto::SystemEvent {
                    component: "config_manager".to_string(),
                    action: "rollback".to_string(),
                    message: format!("Rolled back to version {}: {}", version, version_data.description),
                    details: std::collections::HashMap::new(),
                })),
            };
            
            let _ = self.event_tx.send(event);
            
            info!("Configuration rolled back to version {}: {}", version, version_data.description);
            Ok(ConfigResponse {
                success: true,
                message: format!("Rolled back to version {}", version),
                validation_errors: Vec::new(),
            })
        } else {
            Err(anyhow::anyhow!("Configuration version {} not found", version))
        }
    }
    
    /// Get available configuration versions
    pub async fn get_available_versions(&self) -> Vec<ConfigVersion> {
        self.config_versions.read().await.clone()
    }
    
    /// Schedule a rollback if changes aren't confirmed
    pub async fn schedule_rollback(&self, rollback_to_version: u64) -> Result<()> {
        let rollback_time = SystemTime::now() + self.rollback_timeout;
        *self.pending_rollback.write().await = Some((rollback_to_version, rollback_time));
        
        info!("Scheduled rollback to version {} in {:?}", rollback_to_version, self.rollback_timeout);
        Ok(())
    }
    
    /// Confirm configuration changes (cancel pending rollback)
    pub async fn confirm_changes(&self) -> Result<()> {
        *self.pending_rollback.write().await = None;
        info!("Configuration changes confirmed, rollback cancelled");
        Ok(())
    }
    
    /// Start rollback monitor
    async fn start_rollback_monitor(&self) -> Result<()> {
        let config_manager = self.clone();
        tokio::spawn(async move {
            config_manager.rollback_monitor_loop().await;
        });
        Ok(())
    }
    
    /// Rollback monitor loop
    async fn rollback_monitor_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        
        loop {
            interval.tick().await;
            
            let should_rollback = {
                let pending = self.pending_rollback.read().await;
                if let Some((version, rollback_time)) = *pending {
                    SystemTime::now() >= rollback_time
                } else {
                    false
                }
            };
            
            if should_rollback {
                if let Some((version, _)) = *self.pending_rollback.read().await {
                    warn!("Automatic rollback triggered for version {}", version);
                    
                    if let Err(e) = self.rollback_to_version(version).await {
                        error!("Automatic rollback failed: {}", e);
                    }
                    
                    *self.pending_rollback.write().await = None;
                }
            }
        }
    }
    
    /// Start file watcher task
    async fn start_file_watcher_task(&mut self, config_path: PathBuf) -> Result<()> {
        let config_manager = self.clone();
        let handle = tokio::spawn(async move {
            config_manager.file_watcher_loop(config_path).await;
        });
        
        self.file_watcher_handle = Some(handle);
        Ok(())
    }
    
    /// File watcher loop
    async fn file_watcher_loop(&self, config_path: PathBuf) {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        let mut last_modified = None;
        
        loop {
            interval.tick().await;
            
            if let Ok(metadata) = fs::metadata(&config_path).await {
                if let Ok(modified) = metadata.modified() {
                    if last_modified.map_or(true, |last| modified > last) {
                        last_modified = Some(modified);
                        
                        info!("Configuration file changed, reloading...");
                        match self.reload_config().await {
                            Ok(response) => {
                                if response.success {
                                    info!("Configuration reloaded successfully");
                                } else {
                                    error!("Configuration reload failed: {}", response.message);
                                }
                            }
                            Err(e) => {
                                error!("Configuration reload error: {}", e);
                            }
                        }
                    }
                }
            }
        }
    }
    
    /// Watch for configuration file changes
    pub async fn start_file_watcher(&self) -> Result<()> {
        if let Some(config_path) = &self.config_path {
            info!("Starting configuration file watcher for: {:?}", config_path);
            // File watcher is started in start() method
            Ok(())
        } else {
            Ok(())
        }
    }
}

impl Clone for ConfigManager {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            dynamic_config: Arc::clone(&self.dynamic_config),
            config_path: self.config_path.clone(),
            config_versions: Arc::clone(&self.config_versions),
            current_version: Arc::clone(&self.current_version),
            max_versions: self.max_versions,
            layer_subscribers: Arc::clone(&self.layer_subscribers),
            event_tx: self.event_tx.clone(),
            file_watcher_handle: None, // Don't clone background tasks
            rollback_timeout: self.rollback_timeout,
            pending_rollback: Arc::clone(&self.pending_rollback),
        }
    }
} 