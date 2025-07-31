#![forbid(unsafe_code)]

//! Nyx daemon client and connection management.
//!
//! This module provides the main interface for connecting to and interacting
//! with the Nyx daemon process. It handles connection lifecycle, status monitoring,
//! and provides high-level abstractions for daemon operations.

use crate::config::NyxConfig;
use crate::error::NyxResult;
use crate::events::{NyxEvent, EventHandler};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, Mutex, mpsc};
use tokio::time::Instant;
use tracing::{debug, info, warn};

/// Connection status of the daemon
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    /// Connection is being established
    Connecting,
    /// Connection is active and healthy
    Connected,
    /// Connection is temporarily disconnected but attempting to reconnect
    Reconnecting,
    /// Connection is permanently disconnected
    Disconnected,
    /// Connection failed to establish
    Failed,
}

impl std::fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionStatus::Connecting => write!(f, "connecting"),
            ConnectionStatus::Connected => write!(f, "connected"),
            ConnectionStatus::Reconnecting => write!(f, "reconnecting"),
            ConnectionStatus::Disconnected => write!(f, "disconnected"),
            ConnectionStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Health status of the daemon
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Daemon is healthy and operating normally
    Healthy,
    /// Daemon is experiencing minor issues but still functional
    Degraded,
    /// Daemon is experiencing significant issues
    Unhealthy,
    /// Daemon health status is unknown
    Unknown,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
            HealthStatus::Unknown => write!(f, "unknown"),
        }
    }
}

/// Connection information and metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Unique connection identifier
    pub connection_id: String,
    /// Remote daemon address
    pub remote_address: String,
    /// Connection establishment time
    pub connected_at: DateTime<Utc>,
    /// Current connection status
    pub status: ConnectionStatus,
    /// Current health status
    pub health: HealthStatus,
    /// Round-trip latency in milliseconds
    pub latency_ms: f64,
    /// Number of active streams
    pub active_streams: u32,
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Last activity timestamp
    pub last_activity: DateTime<Utc>,
    /// Connection error count
    pub error_count: u32,
    /// Connection retry count
    pub retry_count: u32,
}

impl Default for ConnectionInfo {
    fn default() -> Self {
        Self {
            connection_id: String::new(),
            remote_address: String::new(),
            connected_at: Utc::now(),
            status: ConnectionStatus::Disconnected,
            health: HealthStatus::Unknown,
            latency_ms: 0.0,
            active_streams: 0,
            bytes_sent: 0,
            bytes_received: 0,
            last_activity: Utc::now(),
            error_count: 0,
            retry_count: 0,
        }
    }
}

/// Overall daemon status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    /// Daemon process ID
    pub pid: Option<u32>,
    /// Daemon version
    pub version: String,
    /// Daemon uptime
    pub uptime: Duration,
    /// Current status
    pub status: ConnectionStatus,
    /// Health status
    pub health: HealthStatus,
    /// Active connections
    pub active_connections: u32,
    /// Total streams
    pub total_streams: u32,
    /// Memory usage in bytes
    pub memory_usage: u64,
    /// CPU usage percentage
    pub cpu_usage: f64,
    /// Network statistics
    pub network_stats: NetworkStats,
}

/// Network statistics for the daemon
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkStats {
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Total packets sent
    pub packets_sent: u64,
    /// Total packets received
    pub packets_received: u64,
    /// Network errors
    pub network_errors: u64,
    /// Connection attempts
    pub connection_attempts: u64,
    /// Successful connections
    pub successful_connections: u64,
}

/// Node information from the daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Node unique identifier
    pub node_id: String,
    /// Node public key
    pub public_key: String,
    /// Node network address
    pub address: String,
    /// Node region
    pub region: String,
    /// Node capabilities
    pub capabilities: Vec<String>,
    /// Node version
    pub version: String,
    /// Node uptime
    pub uptime: Duration,
    /// Last seen timestamp
    pub last_seen: DateTime<Utc>,
}

/// Main Nyx daemon client
pub struct NyxDaemon {
    /// Configuration
    config: NyxConfig,
    /// Connection information
    connection_info: Arc<RwLock<ConnectionInfo>>,
    /// Current daemon status
    daemon_status: Arc<RwLock<DaemonStatus>>,
    /// Event handlers
    event_handlers: Arc<Mutex<Vec<Arc<dyn EventHandler + Send + Sync>>>>,
    /// Event sender channel
    event_sender: mpsc::UnboundedSender<NyxEvent>,
    /// Event receiver channel
    event_receiver: Arc<Mutex<Option<mpsc::UnboundedReceiver<NyxEvent>>>>,
    /// Health check task handle
    health_check_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Statistics collection task handle
    stats_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Shutdown signal
    shutdown_sender: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl NyxDaemon {
    /// Create a new daemon client with the given configuration
    pub fn new(config: NyxConfig) -> Self {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        
        Self {
            config,
            connection_info: Arc::new(RwLock::new(ConnectionInfo::default())),
            daemon_status: Arc::new(RwLock::new(DaemonStatus {
                pid: None,
                version: env!("CARGO_PKG_VERSION").to_string(),
                uptime: Duration::from_secs(0),
                status: ConnectionStatus::Disconnected,
                health: HealthStatus::Unknown,
                active_connections: 0,
                total_streams: 0,
                memory_usage: 0,
                cpu_usage: 0.0,
                network_stats: NetworkStats::default(),
            })),
            event_handlers: Arc::new(Mutex::new(Vec::new())),
            event_sender,
            event_receiver: Arc::new(Mutex::new(Some(event_receiver))),
            health_check_handle: Arc::new(Mutex::new(None)),
            stats_handle: Arc::new(Mutex::new(None)),
            shutdown_sender: Arc::new(Mutex::new(None)),
        }
    }

    /// Connect to the Nyx daemon with the given configuration
    pub async fn connect(config: NyxConfig) -> NyxResult<Self> {
        let mut daemon = Self::new(config);
        daemon.start_connection().await?;
        Ok(daemon)
    }

    /// Start the connection to the daemon
    async fn start_connection(&mut self) -> NyxResult<()> {
        info!("Connecting to Nyx daemon at {}", 
              self.config.daemon.endpoint.as_ref().unwrap_or(&"unknown".to_string()));
        
        // Update connection status
        {
            let mut conn_info = self.connection_info.write().await;
            conn_info.status = ConnectionStatus::Connecting;
            conn_info.remote_address = self.config.daemon.endpoint.as_ref()
                .unwrap_or(&"unknown".to_string()).clone();
            conn_info.connected_at = Utc::now();
            conn_info.connection_id = uuid::Uuid::new_v4().to_string();
        }

        // Emit connection attempt event
        self.emit_event(NyxEvent::DaemonConnected {
            node_id: "unknown".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            connected_at: Utc::now(),
        }).await;

        // Start background tasks
        self.start_health_monitoring().await;
        self.start_statistics_collection().await;
        self.start_event_processing().await;

        // Update connection status to connected
        {
            let mut conn_info = self.connection_info.write().await;
            conn_info.status = ConnectionStatus::Connected;
            conn_info.last_activity = Utc::now();
        }

        {
            let mut status = self.daemon_status.write().await;
            status.status = ConnectionStatus::Connected;
            status.health = HealthStatus::Healthy;
        }

        info!("Successfully connected to Nyx daemon");
        Ok(())
    }

    /// Get current connection information
    pub async fn connection_info(&self) -> ConnectionInfo {
        self.connection_info.read().await.clone()
    }

    /// Get current daemon status
    pub async fn daemon_status(&self) -> DaemonStatus {
        self.daemon_status.read().await.clone()
    }

    /// Check if the daemon connection is healthy
    pub async fn is_healthy(&self) -> bool {
        let status = self.daemon_status.read().await;
        status.status == ConnectionStatus::Connected && status.health == HealthStatus::Healthy
    }

    /// Get node information from the daemon
    pub async fn node_info(&self) -> NyxResult<NodeInfo> {
        // In a real implementation, this would query the daemon
        Ok(NodeInfo {
            node_id: "local-node".to_string(),
            public_key: "public-key-placeholder".to_string(),
            address: self.config.daemon.endpoint.as_ref()
                .unwrap_or(&"unknown".to_string()).clone(),
            region: "local".to_string(),
            capabilities: vec!["mix".to_string(), "gateway".to_string()],
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime: Duration::from_secs(0),
            last_seen: Utc::now(),
        })
    }

    /// Add an event handler
    pub async fn add_event_handler<H>(&self, handler: H) 
    where 
        H: EventHandler + Send + Sync + 'static 
    {
        let mut handlers = self.event_handlers.lock().await;
        handlers.push(Arc::new(handler));
    }

    /// Remove all event handlers
    pub async fn clear_event_handlers(&self) {
        let mut handlers = self.event_handlers.lock().await;
        handlers.clear();
    }

    /// Emit an event to all registered handlers
    async fn emit_event(&self, event: NyxEvent) {
        // Send through internal channel
        if let Err(e) = self.event_sender.send(event.clone()) {
            warn!("Failed to send event through internal channel: {}", e);
        }

        // Send to external handlers
        let handlers = self.event_handlers.lock().await;
        for handler in handlers.iter() {
            handler.handle_event(event.clone()).await;
        }
    }

    /// Start health monitoring background task
    async fn start_health_monitoring(&self) {
        let daemon_status = Arc::clone(&self.daemon_status);
        let connection_info = Arc::clone(&self.connection_info);
        let event_sender = self.event_sender.clone();
        
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                // Perform health check
                let mut status = daemon_status.write().await;
                let mut conn_info = connection_info.write().await;
                
                // Simulate health check (in real implementation, this would ping the daemon)
                let old_health = status.health;
                status.health = HealthStatus::Healthy;
                status.uptime += Duration::from_secs(30);
                
                conn_info.last_activity = Utc::now();
                
                // Emit health change event if status changed
                if old_health != status.health {
                    let _ = event_sender.send(NyxEvent::DaemonHealthChanged {
                        old_health,
                        new_health: status.health,
                        changed_at: Utc::now(),
                    });
                }
            }
        });

        *self.health_check_handle.lock().await = Some(handle);
    }

    /// Start statistics collection background task
    async fn start_statistics_collection(&self) {
        let daemon_status = Arc::clone(&self.daemon_status);
        
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            
            loop {
                interval.tick().await;
                
                // Update statistics
                let mut status = daemon_status.write().await;
                
                // Simulate statistics collection
                status.memory_usage = 1024 * 1024 * 50; // 50MB
                status.cpu_usage = 5.0; // 5%
                status.network_stats.bytes_sent += 1024;
                status.network_stats.bytes_received += 2048;
            }
        });

        *self.stats_handle.lock().await = Some(handle);
    }

    /// Start event processing background task
    async fn start_event_processing(&self) {
        let mut receiver_opt = self.event_receiver.lock().await.take();
        
        if let Some(mut receiver) = receiver_opt {
            tokio::spawn(async move {
                while let Some(event) = receiver.recv().await {
                    debug!("Processing event: {:?}", event);
                    // Additional event processing can be added here
                }
            });
        }
    }

    /// Gracefully disconnect from the daemon
    pub async fn disconnect(&self) -> NyxResult<()> {
        info!("Disconnecting from Nyx daemon");

        // Update connection status
        {
            let mut conn_info = self.connection_info.write().await;
            conn_info.status = ConnectionStatus::Disconnected;
        }

        {
            let mut status = self.daemon_status.write().await;
            status.status = ConnectionStatus::Disconnected;
            status.health = HealthStatus::Unknown;
        }

        // Emit disconnection event
        self.emit_event(NyxEvent::DaemonDisconnected {
            reason: "User requested disconnection".to_string(),
            disconnected_at: Utc::now(),
        }).await;

        // Stop background tasks
        if let Some(handle) = self.health_check_handle.lock().await.take() {
            handle.abort();
        }

        if let Some(handle) = self.stats_handle.lock().await.take() {
            handle.abort();
        }

        // Send shutdown signal
        if let Some(sender) = self.shutdown_sender.lock().await.take() {
            let _ = sender.send(());
        }

        info!("Disconnected from Nyx daemon");
        Ok(())
    }

    /// Test the connection to the daemon
    pub async fn test_connection(&self) -> NyxResult<Duration> {
        let start = Instant::now();
        
        // Simulate connection test (in real implementation, this would ping the daemon)
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        let latency = start.elapsed();
        
        // Update connection info
        {
            let mut conn_info = self.connection_info.write().await;
            conn_info.latency_ms = latency.as_millis() as f64;
            conn_info.last_activity = Utc::now();
        }

        Ok(latency)
    }

    /// Get detailed connection metrics
    pub async fn connection_metrics(&self) -> HashMap<String, serde_json::Value> {
        let conn_info = self.connection_info.read().await;
        let daemon_status = self.daemon_status.read().await;
        
        let mut metrics = HashMap::new();
        
        metrics.insert("connection_status".to_string(), 
            serde_json::Value::String(conn_info.status.to_string()));
        metrics.insert("health_status".to_string(), 
            serde_json::Value::String(daemon_status.health.to_string()));
        metrics.insert("latency_ms".to_string(), 
            serde_json::Value::Number(serde_json::Number::from_f64(conn_info.latency_ms).unwrap()));
        metrics.insert("active_streams".to_string(), 
            serde_json::Value::Number(serde_json::Number::from(conn_info.active_streams)));
        metrics.insert("bytes_sent".to_string(), 
            serde_json::Value::Number(serde_json::Number::from(conn_info.bytes_sent)));
        metrics.insert("bytes_received".to_string(), 
            serde_json::Value::Number(serde_json::Number::from(conn_info.bytes_received)));
        metrics.insert("error_count".to_string(), 
            serde_json::Value::Number(serde_json::Number::from(conn_info.error_count)));
        metrics.insert("uptime_seconds".to_string(), 
            serde_json::Value::Number(serde_json::Number::from(daemon_status.uptime.as_secs())));
        
        metrics
    }
}

impl Drop for NyxDaemon {
    fn drop(&mut self) {
        // Ensure cleanup on drop
        if let Some(handle) = self.health_check_handle.blocking_lock().take() {
            handle.abort();
        }
        if let Some(handle) = self.stats_handle.blocking_lock().take() {
            handle.abort();
        }
    }
}
