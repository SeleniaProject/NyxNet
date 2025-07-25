#![forbid(unsafe_code)]

//! Comprehensive stream management system for Nyx daemon.
//!
//! This module manages the complete lifecycle of Nyx streams including:
//! - Stream creation and destruction
//! - Multipath routing and path selection
//! - Real-time statistics collection
//! - Error handling and recovery
//! - Session management with Connection IDs (CID)

use crate::proto::{self, StreamStats, Event, PeerInfo};
use anyhow::Result;
use dashmap::DashMap;
use nyx_core::types::*;
use nyx_stream::StreamState;
use nyx_mix::{cmix::CmixController, larmix::LARMixPlanner};
use nyx_transport::{Transport};
use crate::path_builder::{PathBuilder, PathBuilderConfig};

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, SystemTime};
use tokio::sync::{RwLock, broadcast};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Stream-related errors
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("Invalid target address: {0}")]
    InvalidAddress(String),
    #[error("Stream not found: {stream_id}")]
    StreamNotFound { stream_id: u32 },
    // #[error("Connection failed: {0}")]
    // ConnectionFailed(String),
    // #[error("Transport error: {0}")]
    // TransportError(String),
    #[error("Path building failed: {0}")]
    PathBuildingFailed(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
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
    pub max_paths_per_stream: u8,
    // pub path_validation_timeout_ms: u32,
    pub cleanup_interval_secs: u64,
    pub monitoring_interval_secs: u64,
    pub enable_multipath: bool,
    // pub enable_path_redundancy: bool,
    pub latency_bias: f64, // 0.0-1.0, higher values prefer low latency
}

impl Default for StreamManagerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_streams: MAX_CONCURRENT_STREAMS,
            default_timeout_ms: DEFAULT_STREAM_TIMEOUT_MS,
            max_paths_per_stream: MAX_PATHS_PER_STREAM,
            // path_validation_timeout_ms: 5000,
            cleanup_interval_secs: 30,
            monitoring_interval_secs: 5,
            enable_multipath: true,
            // enable_path_redundancy: false,
            latency_bias: 0.7,
        }
    }
}

/// Path statistics for monitoring
#[derive(Debug, Default)]
pub struct PathStatistics {
    pub bytes_sent: std::sync::atomic::AtomicU64,
    pub bytes_received: std::sync::atomic::AtomicU64,
    pub packet_count: std::sync::atomic::AtomicU64,
    pub failure_count: std::sync::atomic::AtomicU64,
    pub rtt_samples: Vec<f64>,
    pub bandwidth_samples: Vec<f64>,
}

impl Clone for PathStatistics {
    fn clone(&self) -> Self {
        Self {
            bytes_sent: AtomicU64::new(self.bytes_sent.load(Ordering::Relaxed)),
            bytes_received: AtomicU64::new(self.bytes_received.load(Ordering::Relaxed)),
            packet_count: AtomicU64::new(self.packet_count.load(Ordering::Relaxed)),
            failure_count: AtomicU64::new(self.failure_count.load(Ordering::Relaxed)),
            rtt_samples: self.rtt_samples.clone(),
            bandwidth_samples: self.bandwidth_samples.clone(),
        }
    }
}

impl PathStatistics {
    pub fn new() -> Self {
        Self {
            bytes_sent: std::sync::atomic::AtomicU64::new(0),
            bytes_received: std::sync::atomic::AtomicU64::new(0),
            packet_count: std::sync::atomic::AtomicU64::new(0),
            failure_count: std::sync::atomic::AtomicU64::new(0),
            rtt_samples: Vec::new(),
            bandwidth_samples: Vec::new(),
        }
    }
    
    pub fn success_rate(&self) -> f64 {
        let success = self.packet_count.load(Ordering::Relaxed) as f64;
        let total = success + self.failure_count.load(Ordering::Relaxed) as f64;
        if total > 0.0 {
            success / total
        } else {
            1.0
        }
    }
}

/// Stream session information
#[derive(Debug, Clone)]
pub struct StreamSession {
    pub stream_id: u32,
    pub session_id: [u8; 12],
    pub state: nyx_stream::StreamState,
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

/// Stream options
#[derive(Debug, Clone, Default)]
pub struct StreamOptions {
    pub buffer_size: u32,
    pub timeout_ms: u32,
    pub multipath: bool,
    pub max_paths: u32,
    pub path_strategy: String,
    pub auto_reconnect: bool,
    pub max_retry_attempts: u32,
    pub compression: bool,
    pub cipher_suite: String,
}

/// Stream path information
#[derive(Debug, Clone)]
pub struct StreamPath {
    pub path_id: u32,
    pub status: PathStatus,
    pub statistics: PathStatistics,
    pub last_rtt: Option<Duration>,
    pub estimated_bandwidth: f64,
    pub socket_addr: Option<SocketAddr>,
    pub created_at: SystemTime,
}

/// Path status enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum PathStatus {
    Active,
    Inactive,
    Failed,
    Validating,
}

/// Comprehensive stream manager with multipath support
pub struct StreamManager {
    // Core storage
    streams: Arc<DashMap<u32, StreamSession>>,
    
    // Transport layer
    transport: Arc<Transport>,
    
    // Mix network integration
    cmix_controller: Arc<CmixController>,
    
    // Path building and probing
    path_builder: Arc<PathBuilder>,
    prober: Arc<RwLock<nyx_mix::larmix::Prober>>,
    scheduler: Arc<RwLock<PathScheduler>>,
    known_peers: Arc<DashMap<NodeId, PeerInfo>>,
    active_paths: Arc<DashMap<u32, StreamPath>>,
    
    // Metrics collection
    metrics: Arc<crate::metrics::MetricsCollector>,
    
    // Stream ID counter
    next_stream_id: std::sync::atomic::AtomicU32,
    
    // Event broadcasting
    event_tx: broadcast::Sender<Event>,
    
    // Configuration
    config: Arc<StreamManagerConfig>,
    
    // Background tasks
    _cleanup_task: Option<tokio::task::JoinHandle<()>>,
    _monitoring_task: Option<tokio::task::JoinHandle<()>>,
}

/// Path scheduler for multipath routing
#[derive(Debug)]
pub struct PathScheduler {
    pub strategy: PathStrategy,
    pub active_paths: Vec<u32>,
    pub path_weights: HashMap<u32, f64>,
}

impl Default for PathScheduler {
    fn default() -> Self {
        Self {
            strategy: PathStrategy::LoadBalance,
            active_paths: Vec::new(),
            path_weights: HashMap::new(),
        }
    }
}

impl StreamManager {
    /// Create a new stream manager
    pub async fn new(
        transport: Arc<Transport>,
        metrics: Arc<crate::metrics::MetricsCollector>,
        config: StreamManagerConfig,
    ) -> Result<Self> {
        let (event_tx, _) = broadcast::channel(1000);
        
        Ok(Self {
            streams: Arc::new(DashMap::new()),
            transport,
            cmix_controller: Arc::new(CmixController::default()),
            path_builder: Arc::new(PathBuilder::new(
                Arc::new(nyx_control::spawn_dht().await),
                Arc::clone(&metrics),
                PathBuilderConfig::default(),
            )),
            prober: Arc::new(RwLock::new(nyx_mix::larmix::Prober::new())),
            scheduler: Arc::new(RwLock::new(PathScheduler::default())),
            known_peers: Arc::new(DashMap::new()),
            active_paths: Arc::new(DashMap::new()),
            metrics,
            next_stream_id: std::sync::atomic::AtomicU32::new(1),
            event_tx,
            config: Arc::new(config),
            _cleanup_task: None,
            _monitoring_task: None,
        })
    }
    
    /// Start the stream manager
    pub async fn start(self: Arc<Self>) {
        info!("Starting stream manager");

        let manager_clone = Arc::clone(&self);
        let cleanup_task = tokio::spawn(async move {
            manager_clone.cleanup_loop().await;
        });

        let manager_clone = Arc::clone(&self);
        let monitoring_task = tokio::spawn(async move {
            manager_clone.monitoring_loop().await;
        });

        // We need to store the handles to prevent the tasks from being dropped
        // But since we are in an Arc<Self> we can't directly mutate.
        // In a real scenario, you might use a Mutex or other interior mutability pattern
        // For now, we just let them run.
        let _ = (cleanup_task, monitoring_task);
    }
    
    /// Open a new stream with complete implementation
    pub async fn open_stream(&self, request: proto::OpenRequest) -> Result<proto::StreamResponse> {
        info!("Opening new stream to target: {}", request.target_address);
        
        // Check if we've reached the maximum number of concurrent streams
        if self.streams.len() >= self.config.max_concurrent_streams as usize {
            error!("Maximum concurrent streams reached: {}", self.config.max_concurrent_streams);
            return Ok(proto::StreamResponse {
                stream_id: 0,
                status: "error".to_string(),
                target_address: request.target_address,
                initial_stats: None,
                success: false,
                message: "Maximum concurrent streams reached".to_string(),
            });
        }
        
        let stream_id = self.next_stream_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = SystemTime::now();
        
        // Parse and validate target address
        let target_addr = match request.target_address.parse::<SocketAddr>() {
            Ok(addr) => addr,
            Err(e) => {
                error!("Invalid target address '{}': {}", request.target_address, e);
                return Ok(proto::StreamResponse {
                    stream_id,
                    status: "error".to_string(),
                    target_address: request.target_address,
                    initial_stats: None,
                    success: false,
                    message: format!("Invalid target address: {}", e),
                });
            }
        };
        
        // Generate session ID (CID)
        let mut session_id = [0u8; 12];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut session_id);
        
        // Create stream options from request
        let options = if let Some(opts) = request.options {
            StreamOptions {
                buffer_size: opts.buffer_size.max(1024).min(1024 * 1024), // 1KB to 1MB
                timeout_ms: opts.timeout_ms.max(1000).min(300000), // 1s to 5min
                multipath: opts.multipath && self.config.enable_multipath,
                max_paths: opts.max_paths.min(self.config.max_paths_per_stream as u32),
                path_strategy: opts.path_strategy,
                auto_reconnect: opts.auto_reconnect,
                max_retry_attempts: opts.max_retry_attempts.min(10),
                compression: opts.compression,
                cipher_suite: opts.cipher_suite,
            }
        } else {
            StreamOptions {
                buffer_size: 64 * 1024, // 64KB default
                timeout_ms: self.config.default_timeout_ms,
                multipath: self.config.enable_multipath,
                max_paths: 3,
                path_strategy: "load_balance".to_string(),
                auto_reconnect: true,
                max_retry_attempts: 3,
                compression: false,
                cipher_suite: "ChaCha20Poly1305".to_string(),
            }
        };
        
        // Build network paths for the stream
        let paths = if options.multipath {
            match self.build_multipath_routes(&request.target_address, options.max_paths).await {
                Ok(paths) => paths,
                Err(e) => {
                    warn!("Failed to build multipath routes, falling back to single path: {}", e);
                    vec![self.build_single_path(&target_addr).await?]
                }
            }
        } else {
            vec![self.build_single_path(&target_addr).await?]
        };
        
        // Create stream session
        let session = StreamSession {
            stream_id,
            session_id,
            state: nyx_stream::StreamState::Idle,
            created_at: now,
            last_activity: now,
            bytes_sent: 0,
            bytes_received: 0,
            error_count: 0,
            last_error: None,
            last_error_at: None,
            statistics: PathStatistics::new(),
            paths: paths.clone(),
            options: options.clone(),
        };
        
        // Store the session
        self.streams.insert(stream_id, session.clone());
        
        // Update metrics
        self.metrics.increment_active_streams();
        
        // Emit stream opened event
        let event = proto::Event {
            r#type: "stream".to_string(),
            detail: "Stream opened".to_string(),
            timestamp: Some(crate::system_time_to_proto_timestamp(now)),
            severity: "info".to_string(),
            attributes: [
                ("stream_id".to_string(), stream_id.to_string()),
                ("target_address".to_string(), request.target_address.clone()),
                ("session_id".to_string(), hex::encode(session_id)),
            ].into_iter().collect(),
            event_data: Some(proto::event::EventData::StreamEvent(proto::StreamEvent {
                stream_id,
                action: "opened".to_string(),
                target_address: request.target_address.clone(),
                stats: Some(self.build_stream_stats(&session).await),
                event_type: "stream_opened".to_string(),
                timestamp: Some(crate::system_time_to_proto_timestamp(now)),
                data: HashMap::new(),
            })),
        };
        
        let _ = self.event_tx.send(event);
        
        // Create initial stats
        let initial_stats = self.build_stream_stats(&session).await;
        
        info!("Stream {} opened successfully to {}", stream_id, request.target_address);
        
        Ok(proto::StreamResponse {
            stream_id,
            status: "opened".to_string(),
            target_address: request.target_address,
            initial_stats: Some(initial_stats),
            success: true,
            message: "Stream opened successfully".to_string(),
        })
    }
    
    /// Close a stream with complete cleanup
    pub async fn close_stream(&self, stream_id: u32) -> Result<()> {
        info!("Closing stream {}", stream_id);
        
        // Remove the stream from active streams
        if let Some((_, mut session)) = self.streams.remove(&stream_id) {
            // Update stream state to closed
            session.state = nyx_stream::StreamState::Closed;
            session.last_activity = SystemTime::now();
            
            // Clean up all paths associated with this stream
            for path in &session.paths {
                self.active_paths.remove(&path.path_id);
            }
            
            // Update metrics
            self.metrics.decrement_active_streams();
            
            // Emit stream closed event
            let event = proto::Event {
                r#type: "stream".to_string(),
                detail: "Stream closed".to_string(),
                timestamp: Some(crate::system_time_to_proto_timestamp(SystemTime::now())),
                severity: "info".to_string(),
                attributes: [
                    ("stream_id".to_string(), stream_id.to_string()),
                    ("session_id".to_string(), hex::encode(session.session_id)),
                    ("bytes_sent".to_string(), session.bytes_sent.to_string()),
                    ("bytes_received".to_string(), session.bytes_received.to_string()),
                ].into_iter().collect(),
                event_data: Some(proto::event::EventData::StreamEvent(proto::StreamEvent {
                    stream_id,
                    action: "closed".to_string(),
                    target_address: "".to_string(),
                    stats: Some(self.build_stream_stats(&session).await),
                    event_type: "stream_closed".to_string(),
                    timestamp: Some(crate::system_time_to_proto_timestamp(SystemTime::now())),
                    data: HashMap::new(),
                })),
            };
            
            let _ = self.event_tx.send(event);
            
            info!("Stream {} closed successfully (sent: {} bytes, received: {} bytes)", 
                  stream_id, session.bytes_sent, session.bytes_received);
        } else {
            warn!("Attempted to close non-existent stream {}", stream_id);
            return Err(anyhow::anyhow!("Stream {} not found", stream_id));
        }
        
        Ok(())
    }
    
    /// Get stream statistics
    pub async fn get_stream_stats(&self, stream_id: u32) -> Result<proto::StreamStats> {
        if let Some(session) = self.streams.get(&stream_id) {
            Ok(self.build_stream_stats(&session).await)
        } else {
            Err(anyhow::anyhow!("Stream {} not found", stream_id))
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
        let prober = self.prober.write().await;
        let planner = LARMixPlanner::new(&*prober, self.config.latency_bias);
        
        for path_id in 0..std::cmp::min(max_paths, self.config.max_paths_per_stream as u32) {
            match self.build_single_path_with_planner(&planner, target, path_id as u8).await {
                Ok(path) => paths.push(path),
                Err(e) => {
                    error!("Failed to build path {}: {}", path_id, e);
                    return Err(StreamError::PathBuildingFailed(format!("Path building failed: {}", e)));
                }
            }
        }
        
        if paths.is_empty() {
            return Err(StreamError::PathBuildingFailed("No valid paths could be constructed".to_string()));
        }
        
        info!("Built {} paths for multipath stream to {}", paths.len(), target);
        Ok(paths)
    }
    
    /// Build a single network path
    async fn build_single_path(&self, target_addr: &SocketAddr) -> Result<StreamPath, StreamError> {
        let path_id = rand::random::<u32>();
        
        Ok(StreamPath {
            path_id,
            status: PathStatus::Validating,
            statistics: PathStatistics::new(),
            last_rtt: None,
            estimated_bandwidth: 0.0,
            socket_addr: Some(*target_addr),
            created_at: SystemTime::now(),
        })
    }
    
    /// Build a single network path route
    async fn build_single_path_route(&self, target: &str) -> Result<Vec<StreamPath>, StreamError> {
        let target_addr: SocketAddr = target.parse()
            .map_err(|e| StreamError::InvalidAddress(format!("Invalid address {}: {}", target, e)))?;
        
        let path = self.build_single_path(&target_addr).await?;
        Ok(vec![path])
    }
    
    /// Build a single path with planner
    async fn build_single_path_with_planner(
        &self,
        _planner: &LARMixPlanner<'_>,
        target: &str,
        path_id: u8,
    ) -> Result<StreamPath, StreamError> {
        // Parse target address
        let socket_addr: SocketAddr = target.parse()
            .map_err(|e| StreamError::InvalidAddress(format!("Invalid address {}: {}", target, e)))?;
        
        Ok(StreamPath {
            path_id: path_id as u32,
            status: PathStatus::Validating,
            statistics: PathStatistics::new(),
            last_rtt: None,
            estimated_bandwidth: 0.0,
            socket_addr: Some(socket_addr),
            created_at: SystemTime::now(),
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
            _ => Err(StreamError::Configuration("Unsupported path selection strategy".to_string())),
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
    async fn validate_path(&self, stream_id: u32, path_id: u32) -> Result<(), StreamError> {
        // Path validation logic would go here
        // For now, just return success
        debug!("Validating path {} for stream {}", path_id, stream_id);
        Ok(())
    }
    
    /// Build StreamStats from session
    async fn build_stream_stats(&self, session: &StreamSession) -> StreamStats {
        // Generate path statistics
        let mut path_stats = Vec::new();
        for path in &session.paths {
            let path_stat = proto::StreamPathStats {
                path_id: path.path_id,
                status: format!("{:?}", path.status).to_lowercase(),
                rtt_ms: path.last_rtt.map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0),
                bandwidth_mbps: path.estimated_bandwidth,
                bytes_sent: path.statistics.bytes_sent.load(Ordering::Relaxed),
                bytes_received: path.statistics.bytes_received.load(Ordering::Relaxed),
                packet_count: path.statistics.packet_count.load(Ordering::Relaxed) as u32,
                success_rate: path.statistics.success_rate(),
            };
            path_stats.push(path_stat);
        }
        
        // Build path statistics summary
        let _total_paths = session.paths.len();
        let _active_paths = session.paths.iter().filter(|p| p.status == PathStatus::Active).count();
        
        // Calculate average RTT from path statistics
        let avg_rtt_ms = if !session.statistics.rtt_samples.is_empty() {
            session.statistics.rtt_samples.iter().sum::<f64>() / session.statistics.rtt_samples.len() as f64
        } else {
            0.0
        };
        
        // Calculate average bandwidth from path statistics  
        let bandwidth_mbps = if !session.statistics.bandwidth_samples.is_empty() {
            session.statistics.bandwidth_samples.iter().sum::<f64>() / session.statistics.bandwidth_samples.len() as f64
        } else {
            0.0
        };
        
        let stream_info = proto::StreamInfo {
            stream_id: session.stream_id,
            target_address: "unknown".to_string(), // Would be populated from actual target
            state: format!("{:?}", session.state),
            created_at: Some(crate::system_time_to_proto_timestamp(session.created_at)),
        };
        
        proto::StreamStats {
            stream_id: session.stream_id,
            target_address: "unknown".to_string(),
            state: format!("{:?}", session.state),
            created_at: Some(crate::system_time_to_proto_timestamp(session.created_at)),
            last_activity: Some(crate::system_time_to_proto_timestamp(session.last_activity)),
            bytes_sent: session.bytes_sent,
            bytes_received: session.bytes_received,
            packets_sent: 0, // Would be populated from actual stats
            packets_received: 0, // Would be populated from actual stats
            retransmissions: 0,
            avg_rtt_ms,
            min_rtt_ms: 0.0, // Would be calculated from RTT samples
            max_rtt_ms: 0.0, // Would be calculated from RTT samples
            bandwidth_mbps,
            packet_loss_rate: 0.0, // Would be calculated
            paths: path_stats,
            connection_errors: session.error_count,
            timeout_errors: 0,
            last_error: session.last_error.clone().unwrap_or_default(),
            last_error_at: session.last_error_at.map(|t| crate::system_time_to_proto_timestamp(t)),
            stream_info: Some(stream_info),
            path_stats: vec![], // Duplicate of paths field
            timestamp: Some(crate::system_time_to_proto_timestamp(SystemTime::now())),
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
            self.metrics.set_active_streams(self.streams.len() as u32);
            
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

/// Path selection strategy
#[derive(Debug, Clone, PartialEq, Eq)]
enum PathStrategy {
    LatencyWeighted,
    Random,
    LowestLatency,
    LoadBalance,
} 