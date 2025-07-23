#![forbid(unsafe_code)]

//! Comprehensive Nyx daemon implementation.
//!
//! This daemon provides the complete Nyx network functionality including:
//! - Stream management with multipath routing
//! - Real-time metrics collection and monitoring
//! - Advanced path building with geographic diversity
//! - DHT integration for peer discovery
//! - Comprehensive gRPC API for client interaction
//! - Session management with Connection IDs (CID)
//! - Error handling and recovery mechanisms

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use std::collections::HashMap;

use anyhow::Result;
use clap::{Arg, Command};
use tokio::signal;
use tokio::sync::{broadcast, RwLock, Mutex};
use tonic::{transport::Server, Request, Response, Status};
use tracing::{debug, error, info, warn, instrument};
use tracing_subscriber::{fmt, EnvFilter};

// Internal modules
use nyx_core::{types::*, config::NyxConfig, install_panic_abort};
use nyx_mix::{cmix::*, cover::CoverTrafficGenerator, anonymity::AnonymitySet};
use nyx_control::{init_control, ControlManager, DhtHandle, rendezvous::RendezvousService};
use nyx_transport::{Transport, PacketHandler};
use nyx_telemetry::TelemetryCollector;

// Internal modules
mod metrics;
mod stream_manager;
mod path_builder;
mod session_manager;
mod config_manager;
mod health_monitor;
mod event_system;

use metrics::MetricsCollector;
use stream_manager::{StreamManager, StreamManagerConfig};
use path_builder::{PathBuilder, PathBuilderConfig};
use session_manager::{SessionManager, SessionManagerConfig};
use config_manager::{ConfigManager, DynamicConfig};
use health_monitor::{HealthMonitor, HealthCheck};
use event_system::EventSystem;
use crate::proto::EventFilter;

/// Convert SystemTime to proto::Timestamp
fn system_time_to_proto_timestamp(time: SystemTime) -> proto::Timestamp {
    let duration = time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    proto::Timestamp {
        seconds: duration.as_secs() as i64,
        nanos: duration.subsec_nanos() as i32,
    }
}

mod proto {
    tonic::include_proto!("nyx.api");
}

use proto::nyx_control_server::{NyxControl, NyxControlServer};
use proto::*;

/// Comprehensive control service implementation
pub struct ControlService {
    // Core components
    start_time: std::time::Instant,
    node_id: NodeId,
    transport: Arc<Transport>,
    control_manager: ControlManager,
    
    // Advanced subsystems
    metrics: Arc<MetricsCollector>,
    stream_manager: Arc<StreamManager>,
    path_builder: Arc<PathBuilder>,
    session_manager: Arc<SessionManager>,
    config_manager: Arc<ConfigManager>,
    health_monitor: Arc<HealthMonitor>,
    event_system: Arc<EventSystem>,
    
    // Mix routing
    cmix_controller: Arc<Mutex<CmixController>>,
    
    // Event broadcasting
    event_tx: broadcast::Sender<Event>,
    
    // Configuration
    config: Arc<RwLock<NyxConfig>>,
    
    // Statistics
    connection_count: Arc<std::sync::atomic::AtomicU32>,
    total_requests: Arc<std::sync::atomic::AtomicU64>,
}

impl ControlService {
    /// Create a new control service with all subsystems
    pub async fn new(config: NyxConfig) -> anyhow::Result<Self> {
        let start_time = std::time::Instant::now();
        let node_id = Self::generate_node_id(&config);
        
        // Initialize transport layer
        let transport = Arc::new(Transport::start(
            config.listen_port,
            Arc::new(DaemonPacketHandler::new()),
        ).await?);
        
        // Initialize control plane (DHT, push notifications)
        let control_manager = init_control(&config).await;
        
        // Initialize metrics collection
        let metrics = Arc::new(MetricsCollector::new());
        let _metrics_task = metrics.start_collection();
        
        // Initialize stream manager
        let stream_config = StreamManagerConfig::default();
        let mut stream_manager = StreamManager::new(
            Arc::clone(&transport),
            Arc::clone(&metrics),
            stream_config,
        )?;
        stream_manager.start().await?;
        let stream_manager = Arc::new(stream_manager);
        
        // Initialize path builder
        let path_config = PathBuilderConfig::default();
        let path_builder = Arc::new(PathBuilder::new(
            Arc::new(control_manager.dht.clone()),
            Arc::clone(&metrics),
            path_config,
        ));
        path_builder.start().await?;
        
        // Initialize session manager
        let session_config = SessionManagerConfig::default();
        let mut session_manager = SessionManager::new(session_config);
        session_manager.start().await?;
        let session_manager = Arc::new(session_manager);
        
        // Initialize configuration manager
        let config_manager = Arc::new(ConfigManager::new(config.clone()));
        
        // Initialize health monitor
        let health_monitor = Arc::new(HealthMonitor::new());
        health_monitor.start().await?;
        
        // Initialize event system
        let event_system = Arc::new(EventSystem::new());
        
        // Initialize cMix controller
        let cmix_controller = Arc::new(Mutex::new(CmixController::default()));
        
        // Event broadcasting
        let (event_tx, _) = broadcast::channel(1000);
        
        let service = Self {
            start_time,
            node_id,
            transport: Arc::clone(&transport),
            control_manager,
            metrics,
            stream_manager,
            path_builder,
            session_manager,
            config_manager,
            health_monitor,
            event_system,
            cmix_controller,
            event_tx,
            config: Arc::new(RwLock::new(config)),
            connection_count: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            total_requests: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        };
        
        // Start background tasks
        service.start_background_tasks().await?;
        
        info!("Control service initialized with node ID: {}", hex::encode(node_id));
        Ok(service)
    }
    
    /// Generate a node ID from configuration
    fn generate_node_id(config: &NyxConfig) -> NodeId {
        if let Some(node_id_hex) = &config.node_id {
            if let Ok(bytes) = hex::decode(node_id_hex) {
                if bytes.len() == 32 {
                    let mut node_id = [0u8; 32];
                    node_id.copy_from_slice(&bytes);
                    return node_id;
                }
            }
        }
        
        // Generate random node ID if not configured
        let mut node_id = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut node_id);
        node_id
    }
    
    /// Start all background tasks
    async fn start_background_tasks(&self) -> anyhow::Result<()> {
        // Start packet forwarding task
        let transport_clone = Arc::clone(&self.transport);
        let cmix_clone = Arc::clone(&self.cmix_controller);
        let path_builder_clone = Arc::clone(&self.path_builder);
        let metrics_clone = Arc::clone(&self.metrics);
        
        tokio::spawn(async move {
            Self::packet_forwarding_loop(transport_clone, cmix_clone, path_builder_clone, metrics_clone).await;
        });
        
        // Start metrics aggregation task
        let metrics_clone = Arc::clone(&self.metrics);
        let event_tx_clone = self.event_tx.clone();
        
        tokio::spawn(async move {
            Self::metrics_aggregation_loop(metrics_clone, event_tx_clone).await;
        });
        
        // Start configuration monitoring task
        let config_manager_clone = Arc::clone(&self.config_manager);
        let config_clone = Arc::clone(&self.config);
        let event_tx_clone = self.event_tx.clone();
        
        tokio::spawn(async move {
            Self::config_monitoring_loop(config_manager_clone, config_clone, event_tx_clone).await;
        });
        
        info!("All background tasks started");
        Ok(())
    }
    
    /// Packet forwarding background loop
    async fn packet_forwarding_loop(
        transport: Arc<Transport>,
        cmix_controller: Arc<Mutex<CmixController>>,
        path_builder: Arc<PathBuilder>,
        metrics: Arc<MetricsCollector>,
    ) {
        let mut cmix = cmix_controller.lock().await;
        
        loop {
            if let Some(batch) = cmix.recv().await {
                for pkt in batch.packets {
                    // Build path for packet forwarding
                    let path_request = PathRequest {
                        target: "127.0.0.1:43301".to_string(), // TODO: Use actual target
                        hops: 3,
                        strategy: "adaptive".to_string(),
                    };
                    
                    match path_builder.build_path(path_request).await {
                        Ok(path_response) => {
                            // Use first hop as next destination
                            if let Some(first_hop) = path_response.path.first() {
                                if let Ok(addr) = "127.0.0.1:43301".parse::<SocketAddr>() {
                                    transport.send(addr, &pkt).await;
                                    metrics.increment_packets_sent(1);
                                    metrics.increment_bytes_sent(pkt.len() as u64);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to build path for packet forwarding: {}", e);
                        }
                    }
                }
            }
        }
    }
    
    /// Metrics aggregation background loop
    async fn metrics_aggregation_loop(
        metrics: Arc<MetricsCollector>,
        event_tx: broadcast::Sender<Event>,
    ) {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        
        loop {
            interval.tick().await;
            
            // Emit performance events based on metrics
            let performance_metrics = metrics.get_performance_metrics();
            
            // Check for performance anomalies
            if performance_metrics.avg_latency_ms > 1000.0 {
                let event = Event {
                    r#type: "performance".to_string(),
                    detail: "High latency detected".to_string(),
                    timestamp: Some(system_time_to_proto_timestamp(SystemTime::now())),
                    severity: "warn".to_string(),
                    attributes: HashMap::new(),
                    event_data: Some(event::EventData::PerformanceEvent(PerformanceEvent {
                        metric: "latency_spike".to_string(),
                        value: performance_metrics.avg_latency_ms,
                        threshold: 1000.0,
                        description: "Average latency exceeded 1000ms".to_string(),
                    })),
                };
                
                let _ = event_tx.send(event);
            }
            
            if performance_metrics.packet_loss_rate > 0.1 {
                let event = Event {
                    r#type: "performance".to_string(),
                    detail: "High packet loss detected".to_string(),
                    timestamp: Some(system_time_to_proto_timestamp(SystemTime::now())),
                    severity: "warn".to_string(),
                    attributes: HashMap::new(),
                    event_data: Some(event::EventData::PerformanceEvent(PerformanceEvent {
                        metric: "packet_loss_high".to_string(),
                        value: performance_metrics.packet_loss_rate,
                        threshold: 0.1,
                        description: "Packet loss rate exceeded 10%".to_string(),
                    })),
                };
                
                let _ = event_tx.send(event);
            }
        }
    }
    
    /// Configuration monitoring background loop
    async fn config_monitoring_loop(
        config_manager: Arc<ConfigManager>,
        config: Arc<RwLock<NyxConfig>>,
        event_tx: broadcast::Sender<Event>,
    ) {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        
        loop {
            interval.tick().await;
            
            // Check for configuration changes
            if let Ok(updated_config) = config_manager.check_for_updates().await {
                if let Some(new_config) = updated_config {
                    *config.write().await = new_config;
                    
                    let event = Event {
                        r#type: "system".to_string(),
                        detail: "Configuration updated".to_string(),
                        timestamp: Some(system_time_to_proto_timestamp(SystemTime::now())),
                        severity: "info".to_string(),
                        attributes: HashMap::new(),
                        event_data: Some(event::EventData::SystemEvent(SystemEvent {
                            component: "daemon".to_string(),
                            action: "config_reload".to_string(),
                            message: "Configuration has been reloaded".to_string(),
                            details: HashMap::new(),
                        })),
                    };
                    
                    let _ = event_tx.send(event);
                }
            }
        }
    }
    
    /// Build comprehensive node information
    async fn build_node_info(&self) -> NodeInfo {
        let performance_metrics = self.metrics.get_performance_metrics();
        let resource_usage = self.metrics.get_resource_usage().unwrap_or_default();
        
        // Get network topology information
        let topology = NetworkTopology {
            peers: Vec::new(), // Would be populated from actual peer data
            paths: Vec::new(),  // Would be populated from path builder
            total_nodes_known: 0,
            reachable_nodes: 0,
            current_region: "unknown".to_string(),
            available_regions: Vec::new(),
        };
        
        NodeInfo {
            node_id: hex::encode(self.node_id),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_sec: self.start_time.elapsed().as_secs() as u32,
            bytes_in: performance_metrics.total_packets_received,
            bytes_out: performance_metrics.total_packets_sent,
            pid: std::process::id(),
            active_streams: self.metrics.get_active_streams_count() as u32,
            connected_peers: self.metrics.get_connected_peers_count() as u32,
            mix_routes: Vec::new(), // Would be populated from actual mix routes
            performance: Some(performance_metrics),
            resources: Some(resource_usage),
            topology: Some(topology),
        }
    }
}

#[async_trait::async_trait]
impl NyxControl for ControlService {
    /// Get comprehensive node information
    #[instrument(skip(self))]
    async fn get_info(
        &self,
        _request: tonic::Request<proto::Empty>,
    ) -> Result<tonic::Response<NodeInfo>, tonic::Status> {
        self.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        let info = self.build_node_info().await;
        Ok(tonic::Response::new(info))
    }
    
    /// Get health status with detailed checks
    #[instrument(skip(self))]
    async fn get_health(
        &self,
        request: tonic::Request<HealthRequest>,
    ) -> Result<tonic::Response<HealthResponse>, tonic::Status> {
        self.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        let req = request.into_inner();
        let health_status = self.health_monitor.get_health_status(req.include_details).await;
        
        Ok(tonic::Response::new(health_status))
    }
    
    /// Open a new stream with comprehensive options
    #[instrument(skip(self), fields(target = %request.get_ref().target_address))]
    async fn open_stream(
        &self,
        request: tonic::Request<OpenRequest>,
    ) -> Result<tonic::Response<StreamResponse>, tonic::Status> {
        self.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        let req = request.into_inner();
        
        match self.stream_manager.open_stream(req).await {
            Ok(response) => {
                info!("Stream {} opened successfully", response.stream_id);
                Ok(tonic::Response::new(response))
            }
            Err(e) => {
                error!("Failed to open stream: {}", e);
                Err(tonic::Status::internal(format!("Failed to open stream: {}", e)))
            }
        }
    }
    
    /// Close a stream
    #[instrument(skip(self), fields(stream_id = request.get_ref().id))]
    async fn close_stream(
        &self,
        request: tonic::Request<StreamId>,
    ) -> Result<tonic::Response<proto::Empty>, tonic::Status> {
        self.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        let stream_id = request.into_inner().id;
        
        match self.stream_manager.close_stream(stream_id).await {
            Ok(()) => {
                info!("Stream {} closed successfully", stream_id);
                Ok(tonic::Response::new(proto::Empty {}))
            }
            Err(e) => {
                error!("Failed to close stream {}: {}", stream_id, e);
                Err(tonic::Status::internal(format!("Failed to close stream: {}", e)))
            }
        }
    }
    
    /// Get detailed stream statistics
    #[instrument(skip(self), fields(stream_id = request.get_ref().id))]
    async fn get_stream_stats(
        &self,
        request: tonic::Request<StreamId>,
    ) -> Result<tonic::Response<StreamStats>, tonic::Status> {
        self.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        let stream_id = request.into_inner().id;
        
        match self.stream_manager.get_stream_stats(stream_id).await {
            Ok(stats) => Ok(tonic::Response::new(stats)),
            Err(e) => {
                error!("Failed to get stream stats for {}: {}", stream_id, e);
                Err(tonic::Status::not_found(format!("Stream not found: {}", stream_id)))
            }
        }
    }
    
    /// List all active streams
    type ListStreamsStream = tokio_stream::wrappers::ReceiverStream<Result<StreamStats, tonic::Status>>;
    
    #[instrument(skip(self))]
    async fn list_streams(
        &self,
        _request: tonic::Request<proto::Empty>,
    ) -> Result<tonic::Response<Self::ListStreamsStream>, tonic::Status> {
        self.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        let stream_manager = Arc::clone(&self.stream_manager);
        tokio::spawn(async move {
            let streams = stream_manager.list_streams().await;
            
            for stream_stats in streams {
                if tx.send(Ok(stream_stats)).await.is_err() {
                    break;
                }
            }
        });
        
        Ok(tonic::Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
    
    /// Subscribe to real-time events
    type SubscribeEventsStream = tokio_stream::wrappers::ReceiverStream<Result<Event, tonic::Status>>;
    
    #[instrument(skip(self))]
    async fn subscribe_events(
        &self,
        request: tonic::Request<EventFilter>,
    ) -> Result<tonic::Response<Self::SubscribeEventsStream>, tonic::Status> {
        self.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        let filter = request.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(1000);
        
        // Subscribe to event broadcasts
        let mut event_rx = self.event_tx.subscribe();
        let event_system = Arc::clone(&self.event_system);
        
        tokio::spawn(async move {
            while let Ok(event) = event_rx.recv().await {
                // Apply filters
                if event_system.matches_filter(&event, &filter) {
                    if tx.send(Ok(event)).await.is_err() {
                        break;
                    }
                }
            }
        });
        
        Ok(tonic::Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
    
    /// Subscribe to real-time statistics
    type SubscribeStatsStream = tokio_stream::wrappers::ReceiverStream<Result<StatsUpdate, tonic::Status>>;
    
    #[instrument(skip(self))]
    async fn subscribe_stats(
        &self,
        request: tonic::Request<StatsRequest>,
    ) -> Result<tonic::Response<Self::SubscribeStatsStream>, tonic::Status> {
        self.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        let req = request.into_inner();
        let interval_ms = if req.interval_ms > 0 { req.interval_ms } else { 1000 };
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        let service = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(interval_ms as u64));
            
            loop {
                interval.tick().await;
                
                let node_info = service.build_node_info().await;
                let stream_stats = service.stream_manager.list_streams().await;
                
                let stats_update = StatsUpdate {
                    timestamp: Some(system_time_to_proto_timestamp(SystemTime::now())),
                    node_info: Some(node_info),
                    stream_stats,
                    custom_metrics: HashMap::new(),
                };
                
                if tx.send(Ok(stats_update)).await.is_err() {
                    break;
                }
            }
        });
        
        Ok(tonic::Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
    
    /// Update configuration dynamically
    #[instrument(skip(self))]
    async fn update_config(
        &self,
        request: tonic::Request<ConfigUpdate>,
    ) -> Result<tonic::Response<ConfigResponse>, tonic::Status> {
        self.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        let config_update = request.into_inner();
        
        match self.config_manager.update_config(config_update).await {
            Ok(response) => Ok(tonic::Response::new(response)),
            Err(e) => {
                error!("Failed to update configuration: {}", e);
                Err(tonic::Status::internal(format!("Configuration update failed: {}", e)))
            }
        }
    }
    
    /// Reload configuration from file
    #[instrument(skip(self))]
    async fn reload_config(
        &self,
        _request: tonic::Request<proto::Empty>,
    ) -> Result<tonic::Response<ConfigResponse>, tonic::Status> {
        self.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        match self.config_manager.reload_config().await {
            Ok(response) => Ok(tonic::Response::new(response)),
            Err(e) => {
                error!("Failed to reload configuration: {}", e);
                Err(tonic::Status::internal(format!("Configuration reload failed: {}", e)))
            }
        }
    }
    
    /// Build a network path
    #[instrument(skip(self), fields(target = %request.get_ref().target))]
    async fn build_path(
        &self,
        request: tonic::Request<PathRequest>,
    ) -> Result<tonic::Response<PathResponse>, tonic::Status> {
        self.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        let path_request = request.into_inner();
        
        match self.path_builder.build_path(path_request).await {
            Ok(response) => Ok(tonic::Response::new(response)),
            Err(e) => {
                error!("Failed to build path: {}", e);
                Err(tonic::Status::internal(format!("Path building failed: {}", e)))
            }
        }
    }
    
    /// Get all network paths
    type GetPathsStream = tokio_stream::wrappers::ReceiverStream<Result<PathInfo, tonic::Status>>;
    
    #[instrument(skip(self))]
    async fn get_paths(
        &self,
        _request: tonic::Request<proto::Empty>,
    ) -> Result<tonic::Response<Self::GetPathsStream>, tonic::Status> {
        self.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        // This would enumerate all active paths
        tokio::spawn(async move {
            // Placeholder implementation
            let path_info = PathInfo {
                path_id: 1,
                hops: vec!["node1".to_string(), "node2".to_string(), "node3".to_string()],
                total_latency_ms: 150.0,
                min_bandwidth_mbps: 100.0,
                status: "active".to_string(),
                packet_count: 1000,
                success_rate: 0.95,
                created_at: Some(system_time_to_proto_timestamp(SystemTime::now())),
            };
            
            let _ = tx.send(Ok(path_info)).await;
        });
        
        Ok(tonic::Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
    
    /// Get network topology
    #[instrument(skip(self))]
    async fn get_topology(
        &self,
        _request: tonic::Request<proto::Empty>,
    ) -> Result<tonic::Response<NetworkTopology>, tonic::Status> {
        self.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        // This would return actual network topology
        let topology = NetworkTopology {
            peers: Vec::new(),
            paths: Vec::new(),
            total_nodes_known: 10,
            reachable_nodes: 8,
            current_region: "us-west".to_string(),
            available_regions: vec!["us-west".to_string(), "eu-central".to_string()],
        };
        
        Ok(tonic::Response::new(topology))
    }
    
    /// Get all network peers
    type GetPeersStream = tokio_stream::wrappers::ReceiverStream<Result<PeerInfo, tonic::Status>>;
    
    #[instrument(skip(self))]
    async fn get_peers(
        &self,
        _request: tonic::Request<proto::Empty>,
    ) -> Result<tonic::Response<Self::GetPeersStream>, tonic::Status> {
        self.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        // This would enumerate all known peers
        tokio::spawn(async move {
            // Placeholder implementation
            let peer_info = PeerInfo {
                node_id: "peer1".to_string(),
                address: "127.0.0.1:43301".to_string(),
                latency_ms: 50.0,
                bandwidth_mbps: 100.0,
                status: "connected".to_string(),
                last_seen: Some(system_time_to_proto_timestamp(SystemTime::now())),
                connection_count: 5,
                region: "us-west".to_string(),
            };
            
            let _ = tx.send(Ok(peer_info)).await;
        });
        
        Ok(tonic::Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

impl Clone for ControlService {
    fn clone(&self) -> Self {
        Self {
            start_time: self.start_time,
            node_id: self.node_id,
            transport: Arc::clone(&self.transport),
            control_manager: self.control_manager.clone(),
            metrics: Arc::clone(&self.metrics),
            stream_manager: Arc::clone(&self.stream_manager),
            path_builder: Arc::clone(&self.path_builder),
            session_manager: Arc::clone(&self.session_manager),
            config_manager: Arc::clone(&self.config_manager),
            health_monitor: Arc::clone(&self.health_monitor),
            event_system: Arc::clone(&self.event_system),
            cmix_controller: Arc::clone(&self.cmix_controller),
            event_tx: self.event_tx.clone(),
            config: Arc::clone(&self.config),
            connection_count: Arc::clone(&self.connection_count),
            total_requests: Arc::clone(&self.total_requests),
        }
    }
}

/// Enhanced packet handler for daemon
struct DaemonPacketHandler {
    packet_count: std::sync::atomic::AtomicU64,
}

impl DaemonPacketHandler {
    fn new() -> Self {
        Self {
            packet_count: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[async_trait::async_trait]
impl PacketHandler for DaemonPacketHandler {
    async fn handle_packet(&self, src: SocketAddr, data: &[u8]) {
        let count = self.packet_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        debug!("Received packet {} from {} ({} bytes)", count, src, data.len());
        
        // Enhanced packet processing would go here
        // - Protocol parsing
        // - Security validation
        // - Routing decisions
        // - Metrics collection
    }
}

#[cfg(unix)]
const DEFAULT_ENDPOINT: &str = "/tmp/nyx.sock";
#[cfg(windows)]
const DEFAULT_ENDPOINT: &str = "\\\\.\\pipe\\nyx-daemon.sock";

#[tokio::main(worker_threads = 8)] // Increased worker threads for better performance
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    install_panic_abort();

    // Load configuration with environment variable support
    let cfg_path = std::env::var("NYX_CONFIG").unwrap_or_else(|_| "nyx.toml".into());
    let cfg = NyxConfig::from_file(&cfg_path).unwrap_or_default();

    // Initialize comprehensive tracing
    let level = cfg.log_level.clone().unwrap_or_else(|| "info".to_string());
    std::env::set_var("RUST_LOG", &level);
    
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    info!("Starting Nyx daemon v{}", env!("CARGO_PKG_VERSION"));
    info!("Configuration loaded from: {}", cfg_path);
    info!("Log level: {}", level);

    // Initialize the comprehensive control service
    let svc = ControlService::new(cfg).await?;

    // Prepare the control endpoint
    #[cfg(unix)]
    let _ = std::fs::remove_file(DEFAULT_ENDPOINT);

    // Use TCP listener for Windows compatibility
    let addr = "127.0.0.1:8080".parse()?;
    info!("Control endpoint bound at {}", addr);

    info!("Nyx daemon fully initialized and ready for connections");
    info!("Node ID: {}", hex::encode(svc.node_id));
    info!("Active streams: {}", svc.metrics.get_active_streams_count());
    info!("Connected peers: {}", svc.metrics.get_connected_peers_count());

    // Start the gRPC server with comprehensive service
    if let Err(e) = Server::builder()
        .add_service(NyxControlServer::new(svc))
        .serve(addr)
        .await
    {
        error!("gRPC server terminated with error: {}", e);
        return Err(e.into());
    }

    info!("Nyx daemon shutdown complete");
    Ok(())
} 