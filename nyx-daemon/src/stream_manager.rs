#![forbid(unsafe_code)]

//! Comprehensive stream management system for Nyx daemon.
//!
//! This module manages the complete lifecycle of Nyx streams including:
//! - Stream creation and destruction
//! - Multipath routing and path selection
//! - Real-time statistics collection
//! - Error handling and recovery
//! - Session management with Connection IDs (CID)

use crate::proto::{self, StreamRequest, StreamResponse, StreamStats, StreamInfo as ProtoStreamInfo, StreamEvent, Event, StreamPathStats};
use anyhow::Result;
use dashmap::DashMap;
use nyx_core::types::*;
use nyx_stream::StreamState;
use nyx_mix::{cmix::CmixController, larmix::LARMixPlanner};
use nyx_transport::{Transport, PacketHandler};

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, SystemTime};
use tokio::sync::{RwLock, broadcast};
use tokio::time::interval;
use tracing::{debug, error, info, warn, instrument};

/// Stream-related errors
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("Invalid target address: {0}")]
    InvalidAddress(String),
    #[error("Stream not found: {0}")]
    StreamNotFound(u32),
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Transport error: {0}")]
    TransportError(String),
}

impl From<std::net::AddrParseError> for StreamError {
    fn from(err: std::net::AddrParseError) -> Self {
        StreamError::InvalidAddress(err.to_string())
    }
}

/// Maximum number of concurrent streams per daemon instance
const MAX_CONCURRENT_STREAMS: u32 = 10000;

/// Default stream timeout in milliseconds
const DEFAULT_STREAM_TIMEOUT_MS: u32 = 30000;

/// Maximum number of paths per multipath stream
const MAX_PATHS_PER_STREAM: u8 = 8;

/// Stream manager configuration
#[derive(Debug, Clone)]
pub struct StreamManagerConfig {
    pub max_concurrent_streams: u32,
    pub default_timeout_ms: u32,
    pub cleanup_interval_secs: u64,
    pub monitoring_interval_secs: u64,
    pub latency_bias: f64,
    pub max_paths_per_stream: u8,
    pub path_validation_timeout_ms: u32,
    pub enable_multipath: bool,
    pub enable_path_redundancy: bool,
}

impl Default for StreamManagerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_streams: MAX_CONCURRENT_STREAMS,
            default_timeout_ms: DEFAULT_STREAM_TIMEOUT_MS,
            cleanup_interval_secs: 60,
            monitoring_interval_secs: 30,
            latency_bias: 0.8,
            max_paths_per_stream: MAX_PATHS_PER_STREAM,
            path_validation_timeout_ms: 5000,
            enable_multipath: true,
            enable_path_redundancy: false,
        }
    }
}

/// Stream session information
#[derive(Debug, Clone)]
pub struct StreamSession {
    pub stream_id: u32,
    pub session_id: [u8; 12],
    pub state: StreamState,
    pub created_at: SystemTime,
    pub last_activity: SystemTime,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub error_count: u32,
    pub last_error: Option<String>,
    pub last_error_at: Option<SystemTime>,
    pub statistics: PathStatistics,
    pub paths: Vec<StreamPath>,
    pub options: StreamOptions,
}

/// Per-stream statistics
#[derive(Debug, Default)]
pub struct StreamStatistics {
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    pub packets_sent: AtomicU64,
    pub packets_received: AtomicU64,
    pub retransmissions: AtomicU64,
    pub connection_errors: AtomicU32,
    pub timeout_errors: AtomicU32,
    pub last_error: Arc<RwLock<Option<String>>>,
    pub last_error_at: Arc<RwLock<Option<SystemTime>>>,
    pub rtt_samples: Arc<RwLock<Vec<Duration>>>,
    pub bandwidth_samples: Arc<RwLock<Vec<f64>>>,
}

/// Individual path information for multipath streams
#[derive(Debug, Clone)]
pub struct StreamPath {
    pub path_id: u8,
    pub hops: Vec<NodeId>,
    pub status: PathStatus,
    pub created_at: SystemTime,
    pub statistics: PathStatistics,
    pub socket_addr: Option<SocketAddr>,
    pub last_rtt: Option<Duration>,
    pub estimated_bandwidth: Option<f64>,
}

/// Path status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathStatus {
    Active,
    Backup,
    Failed,
    Validating,
}

/// Per-path statistics
#[derive(Debug, Default)]
pub struct PathStatistics {
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    pub packet_count: AtomicU32,
    pub success_count: AtomicU32,
    pub failure_count: AtomicU32,
}

impl Clone for PathStatistics {
    fn clone(&self) -> Self {
        Self {
            bytes_sent: AtomicU64::new(self.bytes_sent.load(Ordering::Relaxed)),
            bytes_received: AtomicU64::new(self.bytes_received.load(Ordering::Relaxed)),
            packet_count: AtomicU32::new(self.packet_count.load(Ordering::Relaxed)),
            success_count: AtomicU32::new(self.success_count.load(Ordering::Relaxed)),
            failure_count: AtomicU32::new(self.failure_count.load(Ordering::Relaxed)),
        }
    }
}

impl PathStatistics {
    pub fn success_rate(&self) -> f64 {
        let success = self.success_count.load(Ordering::Relaxed) as f64;
        let total = success + self.failure_count.load(Ordering::Relaxed) as f64;
        if total > 0.0 {
            success / total
        } else {
            1.0
        }
    }
}

/// Comprehensive stream manager with multipath support
pub struct StreamManager {
    // Core storage
    streams: Arc<DashMap<u32, StreamSession>>,
    
    // Transport layer
    transport: Arc<Transport>,
    
    // Mix network integration
    cmix_controller: Arc<CmixController>,
    
    // Metrics collection
    metrics: Arc<crate::metrics::MetricsCollector>,
    
    // Stream ID counter
    next_stream_id: std::sync::atomic::AtomicU32,
    
    // Event broadcasting
    event_tx: broadcast::Sender<Event>,
    
    // Configuration
    config: StreamManagerConfig,
    
    // Background tasks
    cleanup_task: Option<tokio::task::JoinHandle<()>>,
    monitoring_task: Option<tokio::task::JoinHandle<()>>,
}

/// Stream manager configuration
#[derive(Debug, Clone)]
pub struct StreamManagerConfig {
    pub max_concurrent_streams: u32,
    pub default_timeout_ms: u32,
    pub max_paths_per_stream: u8,
    pub path_validation_timeout_ms: u32,
    pub cleanup_interval_secs: u64,
    pub monitoring_interval_secs: u64,
    pub enable_multipath: bool,
    pub enable_path_redundancy: bool,
    pub latency_bias: f64, // 0.0-1.0, higher values prefer low latency
}

impl Default for StreamManagerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_streams: MAX_CONCURRENT_STREAMS,
            default_timeout_ms: DEFAULT_STREAM_TIMEOUT_MS,
            max_paths_per_stream: MAX_PATHS_PER_STREAM,
            path_validation_timeout_ms: 5000,
            cleanup_interval_secs: 30,
            monitoring_interval_secs: 5,
            enable_multipath: true,
            enable_path_redundancy: false,
            latency_bias: 0.7,
        }
    }
}

impl StreamManager {
    /// Create a new stream manager
    pub fn new(
        transport: Arc<Transport>,
        cmix_controller: Arc<CmixController>,
        config: StreamManagerConfig,
    ) -> Result<Self> {
        let (event_tx, _) = broadcast::channel(1000);
        let metrics = Arc::new(crate::metrics::MetricsCollector::new());

        Ok(Self {
            streams: Arc::new(DashMap::new()),
            transport,
            cmix_controller,
            metrics,
            next_stream_id: std::sync::atomic::AtomicU32::new(1),
            event_tx,
            config,
            cleanup_task: None,
            monitoring_task: None,
        })
    }
    
    /// Start background tasks
    pub async fn start(&mut self) -> anyhow::Result<()> {
        // Start cleanup task
        let cleanup_task = {
            let manager = self.clone();
            tokio::spawn(async move {
                manager.cleanup_loop().await;
            })
        };
        self.cleanup_task = Some(cleanup_task);
        
        // Start monitoring task
        let monitoring_task = {
            let manager = self.clone();
            tokio::spawn(async move {
                manager.monitoring_loop().await;
            })
        };
        self.monitoring_task = Some(monitoring_task);
        
        info!("Stream manager started with {} max concurrent streams", 
              self.config.max_concurrent_streams);
        Ok(())
    }
    
    /// Open a new stream
    #[instrument(skip(self), fields(target = %request.target_address))]
    pub async fn open_stream(&self, request: StreamRequest) -> Result<StreamResponse> {
        let stream_id = self.next_stream_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let cid = [0u8; 12]; // Generate proper CID
        let parsed_options = StreamOptions::default();
        let paths = Vec::new(); // Initialize empty paths

        let session = StreamSession {
            stream_id,
            session_id: cid,
            state: StreamState::Idle,
            created_at: SystemTime::now(),
            last_activity: SystemTime::now(),
            bytes_sent: 0,
            bytes_received: 0,
            error_count: 0,
            last_error: None,
            last_error_at: None,
            statistics: PathStatistics::default(),
            paths,
            options: parsed_options,
        };

        self.streams.insert(stream_id, session.clone());

        self.emit_stream_event(StreamEvent {
            stream_id,
            event_type: "opened".to_string(),
            timestamp: Some(proto::Timestamp {
                seconds: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default().as_secs() as i64,
                nanos: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default().subsec_nanos() as i32,
            }),
            data: Vec::new(),
        }).await;

        Ok(StreamResponse {
            stream_id,
            success: true,
            message: "Stream opened successfully".to_string(),
        })
    }

    /// Close a stream
    #[instrument(skip(self), fields(stream_id = stream_id))]
    pub async fn close_stream(&self, stream_id: u32) -> Result<()> {
        if let Some(mut session) = self.streams.get_mut(&stream_id) {
            session.state = StreamState::Closed;
            session.last_activity = SystemTime::now();

            self.emit_stream_event(StreamEvent {
                stream_id,
                event_type: "closed".to_string(),
                timestamp: Some(proto::Timestamp {
                    seconds: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default().as_secs() as i64,
                    nanos: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default().subsec_nanos() as i32,
                }),
                data: Vec::new(),
            }).await;
        }
        Ok(())
    }
    
    /// Get stream statistics
    pub async fn get_stream_stats(&self, stream_id: u32) -> Option<StreamStats> {
        if let Some(session) = self.streams.get(&stream_id) {
            let stats = &session.statistics;
            
            let stream_info = proto::StreamInfo {
                stream_id: session.stream_id,
                state: session.state as i32,
                created_at: Some(proto::Timestamp {
                    seconds: session.created_at.duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default().as_secs() as i64,
                    nanos: session.created_at.duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default().subsec_nanos() as i32,
                }),
                last_activity: Some(proto::Timestamp {
                    seconds: session.last_activity.duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default().as_secs() as i64,
                    nanos: session.last_activity.duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default().subsec_nanos() as i32,
                }),
                bytes_sent: session.bytes_sent,
                bytes_received: session.bytes_received,
                error_count: session.error_count,
                last_error: session.last_error.unwrap_or_default(),
                last_error_at: session.last_error_at.map(|t| proto::Timestamp {
                    seconds: t.duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default().as_secs() as i64,
                    nanos: t.duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default().subsec_nanos() as i32,
                }),
            };

            Some(StreamStats {
                stream_id: session.stream_id,
                target_address: "unknown".to_string(),
                state: format!("{:?}", session.state),
                created_at: Some(proto::Timestamp {
                    seconds: session.created_at.duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default().as_secs() as i64,
                    nanos: session.created_at.duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default().subsec_nanos() as i32,
                }),
                last_activity: Some(proto::Timestamp {
                    seconds: session.last_activity.duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default().as_secs() as i64,
                    nanos: session.last_activity.duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default().subsec_nanos() as i32,
                }),
                bytes_sent: stats.bytes_sent.load(Ordering::Relaxed),
                bytes_received: stats.bytes_received.load(Ordering::Relaxed),
                packets_sent: stats.packet_count.load(Ordering::Relaxed) as u64,
                packets_received: stats.packet_count.load(Ordering::Relaxed) as u64,
                retransmissions: 0,
                avg_rtt_ms: 0.0,
                min_rtt_ms: 0.0,
                max_rtt_ms: 0.0,
                bandwidth_mbps: 0.0,
                packet_loss_rate: 0.0,
                paths: Vec::new(),
                connection_errors: stats.failure_count.load(Ordering::Relaxed) as u64,
                timeout_errors: 0,
                last_error: session.last_error.unwrap_or_default(),
                last_error_at: session.last_error_at.map(|t| proto::Timestamp {
                    seconds: t.duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default().as_secs() as i64,
                    nanos: t.duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default().subsec_nanos() as i32,
                }),
            })
        } else {
            None
        }
    }
    
    /// List all active streams
    pub async fn list_streams(&self) -> Vec<StreamStats> {
        let mut stats = Vec::new();
        
        for entry in self.streams.iter() {
            let session = entry.value();
            stats.push(self.build_stream_stats(session).await);
        }
        
        stats
    }
    
    /// Subscribe to stream events
    pub fn subscribe_events(&self) -> broadcast::Receiver<Event> {
        self.event_tx.subscribe()
    }
    
    /// Build network paths for multipath routing
    async fn build_multipath_routes(&self, target: &str, max_paths: u32) -> Result<Vec<StreamPath>, StreamError> {
        let mut paths = Vec::new();
        let prober = self.prober.lock().await;
        let planner = LARMixPlanner::new(&prober, self.config.latency_bias);
        
        for path_id in 0..std::cmp::min(max_paths as u8, self.config.max_paths_per_stream) {
            match self.build_single_path_with_planner(&planner, target, path_id).await {
                Ok(path) => paths.push(path),
                Err(e) => {
                    warn!("Failed to build path {}: {}", path_id, e);
                    if paths.is_empty() {
                        return Err(e);
                    }
                }
            }
        }
        
        if paths.is_empty() {
            return Err(StreamError::PathBuildingFailed {
                reason: "No valid paths could be constructed".to_string(),
            });
        }
        
        info!("Built {} paths for multipath stream to {}", paths.len(), target);
        Ok(paths)
    }
    
    /// Build a single network path
    async fn build_single_path_route(&self, target: &str) -> Result<Vec<StreamPath>, StreamError> {
        let prober = self.prober.lock().await;
        let planner = LARMixPlanner::new(&prober, self.config.latency_bias);
        
        let path = self.build_single_path_with_planner(&planner, target, 0).await?;
        Ok(vec![path])
    }
    
    /// Build a single path using the LARMix planner
    async fn build_single_path_with_planner(
        &self,
        planner: &LARMixPlanner<'_>,
        target: &str,
        path_id: u8,
    ) -> Result<StreamPath, StreamError> {
        // Use adaptive hop count based on network conditions
        let hops = planner.build_path_dynamic();
        
        if hops.is_empty() {
            return Err(StreamError::PathBuildingFailed {
                reason: "No candidate nodes available".to_string(),
            });
        }
        
        // Resolve target address
        let socket_addr = self.parse_target_address(target)?;
        
        Ok(StreamPath {
            path_id,
            hops,
            status: PathStatus::Validating,
            created_at: SystemTime::now(),
            statistics: PathStatistics::default(),
            socket_addr: Some(socket_addr),
            last_rtt: None,
            estimated_bandwidth: None,
        })
    }
    
    /// Parse target address string to SocketAddr
    fn parse_target_address(&self, address: &str) -> Result<SocketAddr, StreamError> {
        address.parse().map_err(|_| StreamError::InvalidAddress(
            format!("Invalid address format: {}", address)
        ))
    }
    
    /// Parse stream options
    fn parse_stream_options(&self, options: StreamOptions) -> Result<StreamOptions, StreamError> {
        // Validate max paths
        if options.max_paths > self.config.max_paths_per_stream as u32 {
            return Err(StreamError::InvalidAddress(
                format!("Max paths {} exceeds limit {}", 
                    options.max_paths, self.config.max_paths_per_stream)
            ));
        }

        // Validate timeout
        if options.timeout_ms == 0 {
            return Ok(StreamOptions {
                timeout_ms: self.config.default_timeout_ms,
                ..options
            });
        }

        Ok(options)
    }
    
    /// Parse path strategy string
    fn parse_path_strategy(&self, strategy: &str) -> Result<PathStrategy, StreamError> {
        match strategy {
            "latency_weighted" | "" => Ok(PathStrategy::LatencyWeighted),
            "random" => Ok(PathStrategy::Random),
            "lowest_latency" => Ok(PathStrategy::LowestLatency),
            "load_balance" => Ok(PathStrategy::LoadBalance),
            _ => Err(StreamError::Configuration {
                message: format!("Unknown path strategy: {}", strategy),
            }),
        }
    }
    
    /// Generate a new 96-bit Connection ID
    fn generate_cid(&self) -> [u8; 12] {
        let mut cid = [0u8; 12];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut cid);
        cid
    }
    
    /// Initialize stream connection (perform handshake, path validation, etc.)
    async fn initialize_stream_connection(&self, stream_id: u32) -> Result<StreamStats, StreamError> {
        let session = self.streams.get(&stream_id)
            .ok_or(StreamError::StreamNotFound { stream_id })?;
        
        // Validate all paths
        for path in &session.paths {
            if let Err(e) = self.validate_path(stream_id, path.path_id).await {
                warn!("Path {} validation failed for stream {}: {}", 
                      path.path_id, stream_id, e);
            }
        }
        
        // Update stream state to open
        let mut session = session.clone();
        session.state = StreamState::Open;
        session.last_activity = SystemTime::now();
        self.streams.insert(stream_id, session.clone());
        
        Ok(self.build_stream_stats(&session).await)
    }
    
    /// Validate a network path
    async fn validate_path(&self, stream_id: u32, path_id: u8) -> Result<(), StreamError> {
        // This would perform actual path validation using PATH_CHALLENGE/PATH_RESPONSE frames
        // For now, we'll simulate validation
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Update path status to active
        if let Some(mut session) = self.streams.get_mut(&stream_id) {
            if let Some(path) = session.paths.iter_mut().find(|p| p.path_id == path_id) {
                path.status = PathStatus::Active;
                path.statistics.success_count.fetch_add(1, Ordering::Relaxed);
            }
        }
        
        Ok(())
    }
    
    /// Build StreamStats from session
    async fn build_stream_stats(&self, session: &StreamSession) -> StreamStats {
        let stats = &session.statistics;
        let last_error = stats.last_error.read().await.clone();
        let last_error_at = stats.last_error_at.read().await.clone();
        
        // Calculate RTT statistics
        let rtt_samples = stats.rtt_samples.lock().await;
        let (min_rtt, max_rtt, avg_rtt) = if rtt_samples.is_empty() {
            (0.0, 0.0, 0.0)
        } else {
            let min = rtt_samples.iter().min().unwrap().as_secs_f64() * 1000.0;
            let max = rtt_samples.iter().max().unwrap().as_secs_f64() * 1000.0;
            let avg = rtt_samples.iter().map(|d| d.as_secs_f64()).sum::<f64>() / rtt_samples.len() as f64 * 1000.0;
            (min, max, avg)
        };
        
        // Calculate bandwidth
        let bandwidth_samples = stats.bandwidth_samples.lock().await;
        let bandwidth_mbps = if bandwidth_samples.is_empty() {
            0.0
        } else {
            bandwidth_samples.iter().sum::<f64>() / bandwidth_samples.len() as f64
        };
        
        // Build path statistics
        let mut path_stats = Vec::new();
        for path in &session.paths {
            let path_stat = StreamPathStats {
                path_id: path.path_id as u32,
                status: format!("{:?}", path.status).to_lowercase(),
                rtt_ms: path.last_rtt.map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0),
                bandwidth_mbps: path.estimated_bandwidth.unwrap_or(0.0),
                bytes_sent: path.statistics.bytes_sent.load(Ordering::Relaxed),
                bytes_received: path.statistics.bytes_received.load(Ordering::Relaxed),
                packet_count: path.statistics.packet_count.load(Ordering::Relaxed),
                success_rate: path.statistics.success_rate(),
            };
            path_stats.push(path_stat);
        }
        
        let stream_info = proto::StreamInfo {
            stream_id: session.stream_id,
            state: session.state as i32,
            created_at: Some(proto::Timestamp {
                seconds: session.created_at.duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default().as_secs() as i64,
                nanos: session.created_at.duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default().subsec_nanos() as i32,
            }),
            last_activity: Some(proto::Timestamp {
                seconds: session.last_activity.duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default().as_secs() as i64,
                nanos: session.last_activity.duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default().subsec_nanos() as i32,
            }),
            bytes_sent: session.bytes_sent,
            bytes_received: session.bytes_received,
            error_count: session.error_count,
            last_error: session.last_error.unwrap_or_default(),
            last_error_at: session.last_error_at.map(|t| proto::Timestamp {
                seconds: t.duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default().as_secs() as i64,
                nanos: t.duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default().subsec_nanos() as i32,
            }),
        };
        
        StreamStats {
            stream_info: Some(stream_info),
            path_stats,
            timestamp: Some(proto::Timestamp {
                seconds: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default().as_secs() as i64,
                nanos: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default().subsec_nanos() as i32,
            }),
        }
    }
    
    /// Emit a stream event
    async fn emit_stream_event(&self, stream_event: StreamEvent) {
        let event = Event {
            r#type: "stream".to_string(),
            detail: format!("Stream {} {}", stream_event.stream_id, stream_event.action),
            timestamp: Some(prost_types::Timestamp::from(SystemTime::now())),
            severity: "info".to_string(),
            attributes: HashMap::new(),
            event_data: Some(crate::proto::event::EventData::StreamEvent(stream_event)),
        };
        
        if let Err(e) = self.event_tx.send(event) {
            debug!("No event subscribers: {}", e);
        }
    }
    
    /// Cleanup stream resources
    async fn cleanup_stream_resources(&self, stream_id: u32) {
        // This would clean up any allocated resources for the stream
        debug!("Cleaning up resources for stream {}", stream_id);
    }
    
    /// Background cleanup loop
    async fn cleanup_loop(&self) {
        let mut interval = interval(Duration::from_secs(self.config.cleanup_interval_secs));
        
        loop {
            interval.tick().await;
            
            let now = SystemTime::now();
            let mut expired_streams = Vec::new();
            
            // Find expired streams
            for entry in self.streams.iter() {
                let session = entry.value();
                let timeout = Duration::from_millis(session.options.timeout_ms as u64);
                
                if now.duration_since(session.last_activity).unwrap_or_default() > timeout {
                    expired_streams.push(session.stream_id);
                }
            }
            
            // Clean up expired streams
            for stream_id in expired_streams {
                if let Err(e) = self.close_stream(stream_id).await {
                    warn!("Failed to cleanup expired stream {}: {}", stream_id, e);
                }
            }
            
            debug!("Cleanup cycle completed, {} active streams", self.streams.len());
        }
    }
    
    /// Background monitoring loop
    async fn monitoring_loop(&self) {
        let mut interval = interval(Duration::from_secs(self.config.monitoring_interval_secs));
        
        loop {
            interval.tick().await;
            
            // Update metrics
            self.metrics.set_active_streams(self.streams.len());
            
            // Monitor stream health
            for entry in self.streams.iter() {
                let session = entry.value();
                // Perform health checks, update statistics, etc.
                self.update_stream_health(session).await;
            }
            
            debug!("Monitoring cycle completed");
        }
    }
    
    /// Update health metrics for a stream
    async fn update_stream_health(&self, session: &StreamSession) {
        // This would perform various health checks and update metrics
        // For now, we'll just update the last activity time if the stream is active
        
        if matches!(session.state, StreamState::Open) {
            // Update bandwidth utilization, latency measurements, etc.
        }
    }
}

/// Parsed stream options
#[derive(Debug, Clone)]
struct ParsedStreamOptions {
    pub buffer_size: u32,
    pub timeout_ms: u32,
    pub multipath: bool,
    pub max_paths: u32,
    pub path_strategy: PathStrategy,
    pub auto_reconnect: bool,
    pub max_retry_attempts: u32,
    pub compression: bool,
    pub cipher_suite: String,
}

/// Path selection strategy
#[derive(Debug, Clone, PartialEq, Eq)]
enum PathStrategy {
    LatencyWeighted,
    Random,
    LowestLatency,
    LoadBalance,
}

impl Clone for StreamManager {
    fn clone(&self) -> Self {
        Self {
            transport: Arc::clone(&self.transport),
            metrics: Arc::clone(&self.metrics),
            streams: Arc::clone(&self.streams),
            next_stream_id: AtomicU32::new(self.next_stream_id.load(Ordering::Relaxed)),
            path_builder: Arc::clone(&self.path_builder),
            prober: Arc::clone(&self.prober),
            scheduler: Arc::clone(&self.scheduler),
            known_peers: Arc::clone(&self.known_peers),
            active_paths: Arc::clone(&self.active_paths),
            event_tx: self.event_tx.clone(),
            config: self.config.clone(),
            cleanup_task: None,
            monitoring_task: None,
        }
    }
} 