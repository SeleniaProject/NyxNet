#![forbid(unsafe_code)]

//! Daemon connection management for the Nyx SDK.
//!
//! This module provides the main interface for connecting to and communicating with
//! the Nyx daemon, including connection pooling, health monitoring, and status tracking.

use crate::config::NyxConfig;
use crate::error::{NyxError, NyxResult};
use crate::proto::nyx_control_client::NyxControlClient;
use crate::proto::{OpenRequest, StreamId, StreamResponse};
use crate::stream::{NyxStream, StreamOptions};
use crate::events::{NyxEvent, EventHandler};

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Mutex};
use tonic::transport::{Channel, Endpoint};
use tracing::{debug, error, info, warn};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::proto::Empty;

/// Main daemon connection manager
pub struct NyxDaemon {
    config: NyxConfig,
    client: Arc<Mutex<NyxControlClient<Channel>>>,
    connection_info: Arc<RwLock<ConnectionInfo>>,
    event_handler: Option<Arc<dyn EventHandler>>,
    health_monitor: Arc<HealthMonitor>,
}

/// Node information from daemon (SDK representation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub node_id: String,
    pub version: String,
    pub uptime_sec: u32,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub pid: u32,
    pub active_streams: u32,
    pub connected_peers: u32,
    pub mix_routes: Vec<String>,
}

impl From<crate::proto::NodeInfo> for NodeInfo {
    fn from(proto: crate::proto::NodeInfo) -> Self {
        Self {
            node_id: proto.node_id,
            version: proto.version,
            uptime_sec: proto.uptime_sec,
            bytes_in: proto.bytes_in,
            bytes_out: proto.bytes_out,
            pid: proto.pid,
            active_streams: proto.active_streams,
            connected_peers: proto.connected_peers,
            mix_routes: proto.mix_routes,
        }
    }
}

/// Information about the current daemon connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Node information from daemon
    pub node_info: Option<NodeInfo>,
    /// Connection establishment time
    pub connected_at: DateTime<Utc>,
    /// Last successful communication time
    pub last_activity: DateTime<Utc>,
    /// Connection status
    pub status: ConnectionStatus,
    /// Number of active streams
    pub active_streams: u32,
    /// Connection statistics
    pub stats: ConnectionStats,
}

/// Connection status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    /// Connection is healthy
    Connected,
    /// Connection is degraded but functional
    Degraded,
    /// Connection is being reconnected
    Reconnecting,
    /// Connection is disconnected
    Disconnected,
    /// Connection failed
    Failed,
}

/// Connection statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConnectionStats {
    /// Total requests sent
    pub requests_sent: u64,
    /// Total responses received
    pub responses_received: u64,
    /// Total errors encountered
    pub errors: u64,
    /// Average request latency
    pub avg_latency: Duration,
    /// Last error time
    pub last_error: Option<DateTime<Utc>>,
}

/// Daemon status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    /// Connection information
    pub connection: ConnectionInfo,
    /// Daemon node information
    pub node: Option<NodeInfo>,
    /// Health status
    pub health: HealthStatus,
}

/// Health monitoring status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// All systems operational
    Healthy,
    /// Some degradation detected
    Degraded,
    /// Critical issues detected
    Critical,
    /// Health unknown
    Unknown,
}

/// Health monitor for connection monitoring
struct HealthMonitor {
    last_check: Arc<RwLock<Instant>>,
    check_interval: Duration,
    failure_count: Arc<RwLock<u32>>,
    max_failures: u32,
}

impl NyxDaemon {
    /// Connect to the daemon with the provided configuration
    pub async fn connect(config: NyxConfig) -> NyxResult<Self> {
        let endpoint_str = config.daemon_endpoint();
        info!("Connecting to Nyx daemon at {}", endpoint_str);

        let channel = Self::create_channel(endpoint_str, &config).await?;
        let client = NyxControlClient::new(channel);
        
        let connection_info = ConnectionInfo {
            node_info: None,
            connected_at: Utc::now(),
            last_activity: Utc::now(),
            status: ConnectionStatus::Connected,
            active_streams: 0,
            stats: ConnectionStats::default(),
        };

        let health_monitor = HealthMonitor::new(Duration::from_secs(30), 3);

        let daemon = Self {
            config,
            client: Arc::new(Mutex::new(client)),
            connection_info: Arc::new(RwLock::new(connection_info)),
            event_handler: None,
            health_monitor: Arc::new(health_monitor),
        };

        // Perform initial health check
        daemon.health_check().await?;
        
        // Start background health monitoring
        daemon.start_health_monitoring().await;

        info!("Successfully connected to Nyx daemon");
        Ok(daemon)
    }

    /// Create gRPC channel based on endpoint configuration
    async fn create_channel(endpoint_str: String, config: &NyxConfig) -> NyxResult<Channel> {
        if endpoint_str.starts_with("unix://") {
            #[cfg(unix)]
            {
                let path = endpoint_str.strip_prefix("unix://").unwrap();
                let channel = Endpoint::try_from("http://[::]:50051")
                    .map_err(|e| NyxError::daemon_connection("Invalid endpoint", e))?
                    .timeout(config.daemon.connect_timeout)
                    .connect_with_connector(tower::service_fn(move |_: tonic::transport::Uri| {
                        tokio::net::UnixStream::connect(path)
                    }))
                    .await
                    .map_err(|e| NyxError::daemon_connection("Failed to connect via Unix socket", e))?;
                Ok(channel)
            }
            #[cfg(windows)]
            {
                Err(NyxError::daemon_connection_msg("Unix sockets not supported on Windows"))
            }
        } else {
            let channel = Endpoint::try_from(endpoint_str.as_str())
                .map_err(|e| NyxError::daemon_connection("Invalid endpoint", e))?
                .timeout(config.daemon.connect_timeout)
                .connect()
                .await
                .map_err(|e| NyxError::daemon_connection("Failed to connect via TCP", e))?;
            Ok(channel)
        }
    }

    /// Perform health check against the daemon
    pub async fn health_check(&self) -> NyxResult<NodeInfo> {
        debug!("Performing health check");
        
        let start = Instant::now();
        let mut client = self.client.lock().await;
        
        let response = match client.get_info(Empty {}).await {
            Ok(response) => response,
            Err(e) => {
                self.health_monitor.record_failure().await;
                return Err(NyxError::from(e));
            }
        };
        
        let node_info = response.into_inner();
        let latency = start.elapsed();
        
        // Update connection info
        {
            let mut info = self.connection_info.write().await;
            info.node_info = Some(NodeInfo::from(node_info.clone()));
            info.last_activity = Utc::now();
            info.status = ConnectionStatus::Connected;
            info.stats.requests_sent += 1;
            info.stats.responses_received += 1;
            info.stats.avg_latency = latency;
        }
        
        self.health_monitor.record_success().await;
        debug!("Health check completed successfully in {:?}", latency);
        
        Ok(NodeInfo::from(node_info))
    }

    /// Get current daemon status
    pub async fn status(&self) -> DaemonStatus {
        let connection = self.connection_info.read().await.clone();
        let health = self.health_monitor.status().await;
        
        DaemonStatus {
            connection: connection.clone(),
            node: connection.node_info,
            health,
        }
    }

    /// Open a new stream to the specified target
    pub async fn open_stream(&self, target: &str) -> NyxResult<NyxStream> {
        self.open_stream_with_options(target, StreamOptions::default()).await
    }

    /// Open a new stream with custom options
    pub async fn open_stream_with_options(&self, target: &str, options: StreamOptions) -> NyxResult<NyxStream> {
        debug!("Opening stream to {}", target);
        
        let start = Instant::now();
        let mut client = self.client.lock().await;
        
        let request = OpenRequest {
            stream_name: target.to_string(),
            target_address: target.to_string(),
            options: Some(options.clone().into()),
        };
        
        let response = match client.open_stream(tonic::Request::new(request)).await {
            Ok(response) => response,
            Err(e) => {
                self.record_error().await;
                return Err(NyxError::from(e));
            }
        };
        
        let stream_response = response.into_inner();
        let latency = start.elapsed();
        
        // Validate response
        if !stream_response.success {
            return Err(NyxError::stream_error(format!(
                "Failed to open stream: {}", 
                stream_response.message
            ), None));
        }
        
        // Update statistics
        self.record_success(latency).await;
        
        // Increment active stream count
        {
            let mut info = self.connection_info.write().await;
            info.active_streams += 1;
        }
        
        // Emit event
        if let Some(handler) = &self.event_handler {
            handler.handle_event(NyxEvent::StreamOpened {
                stream_id: stream_response.stream_id,
                target: target.to_string(),
            }).await;
        }
        
        debug!("Stream {} opened successfully in {:?}", stream_response.stream_id, latency);
        
        // Create NyxStream wrapper
        let stream = NyxStream::new(
            stream_response.stream_id,
            target.to_string(),
            self.client.clone(),
            self.connection_info.clone(),
            options,
        ).await?;
        
        Ok(stream)
    }

    /// Close a stream by ID
    pub async fn close_stream(&self, stream_id: u32) -> NyxResult<()> {
        debug!("Closing stream {}", stream_id);
        
        let mut client = self.client.lock().await;
        let request = StreamId { id: stream_id };
        
        match client.close_stream(request).await {
            Ok(_) => {},
            Err(e) => {
                self.record_error().await;
                return Err(NyxError::from(e));
            }
        };
        
        // Decrement active stream count
        {
            let mut info = self.connection_info.write().await;
            if info.active_streams > 0 {
                info.active_streams -= 1;
            }
        }
        
        // Emit event
        if let Some(handler) = &self.event_handler {
            handler.handle_event(NyxEvent::StreamClosed { stream_id }).await;
        }
        
        debug!("Stream {} closed successfully", stream_id);
        Ok(())
    }

    /// Set event handler for daemon events
    pub fn set_event_handler(&mut self, handler: Arc<dyn EventHandler>) {
        self.event_handler = Some(handler);
    }

    /// Get configuration
    pub fn config(&self) -> &NyxConfig {
        &self.config
    }

    /// Start background health monitoring
    async fn start_health_monitoring(&self) {
        let client = self.client.clone();
        let connection_info = self.connection_info.clone();
        let health_monitor = self.health_monitor.clone();
        let interval = self.config.daemon.keepalive_interval;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            
            loop {
                interval.tick().await;
                
                // Perform health check
                let mut client_guard = client.lock().await;
                match client_guard.get_info(Empty {}).await {
                    Ok(response) => {
                        let node_info = response.into_inner();
                        health_monitor.record_success().await;
                        
                        // Update connection info
                        let mut info = connection_info.write().await;
                        info.node_info = Some(node_info);
                        info.last_activity = Utc::now();
                        info.status = ConnectionStatus::Connected;
                    }
                    Err(e) => {
                        warn!("Health check failed: {}", e);
                        health_monitor.record_failure().await;
                        
                        let mut info = connection_info.write().await;
                        info.status = ConnectionStatus::Degraded;
                        info.stats.errors += 1;
                        info.stats.last_error = Some(Utc::now());
                    }
                }
            }
        });
    }

    /// Record successful operation
    async fn record_success(&self, latency: Duration) {
        let mut info = self.connection_info.write().await;
        info.last_activity = Utc::now();
        info.stats.requests_sent += 1;
        info.stats.responses_received += 1;
        info.stats.avg_latency = latency;
    }

    /// Record error
    async fn record_error(&self) {
        let mut info = self.connection_info.write().await;
        info.stats.errors += 1;
        info.stats.last_error = Some(Utc::now());
    }
}

impl HealthMonitor {
    fn new(check_interval: Duration, max_failures: u32) -> Self {
        Self {
            last_check: Arc::new(RwLock::new(Instant::now())),
            check_interval,
            failure_count: Arc::new(RwLock::new(0)),
            max_failures,
        }
    }

    async fn record_success(&self) {
        *self.failure_count.write().await = 0;
        *self.last_check.write().await = Instant::now();
    }

    async fn record_failure(&self) {
        *self.failure_count.write().await += 1;
        *self.last_check.write().await = Instant::now();
    }

    async fn status(&self) -> HealthStatus {
        let failure_count = *self.failure_count.read().await;
        let last_check = *self.last_check.read().await;
        
        if last_check.elapsed() > self.check_interval * 2 {
            return HealthStatus::Unknown;
        }
        
        match failure_count {
            0 => HealthStatus::Healthy,
            1..=2 => HealthStatus::Degraded,
            _ => HealthStatus::Critical,
        }
    }
}

impl Default for ConnectionInfo {
    fn default() -> Self {
        Self {
            node_info: None,
            connected_at: Utc::now(),
            last_activity: Utc::now(),
            status: ConnectionStatus::Disconnected,
            active_streams: 0,
            stats: ConnectionStats::default(),
        }
    }
} 