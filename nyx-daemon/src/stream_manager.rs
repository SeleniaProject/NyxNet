#![forbid(unsafe_code)]

//! Comprehensive stream management system for Nyx daemon.
//!
//! This module manages the complete lifecycle of Nyx streams including:
//! - Stream creation and destruction
//! - Multipath routing and path selection
//! - Real-time statistics collection
//! - Error handling and recovery
//! - Session management with Connection IDs (CID)

use crate::metrics::MetricsCollector;
use crate::proto::{
    OpenRequest, StreamResponse, StreamStats, StreamPathStats, StreamOptions,
    StreamEvent, Event, PeerInfo, PathInfo,
};

use dashmap::DashMap;
use nyx_core::NodeId;
use nyx_stream::{Stream, StreamState, StreamLayer, WeightedRrScheduler};
use nyx_mix::{WeightedPathBuilder, Candidate, larmix::{Prober, LARMixPlanner}};
use nyx_transport::Transport;
use nyx_crypto::aead::FrameCrypter;
use nyx_fec::timing::TimingConfig;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{mpsc, RwLock, Mutex, oneshot, broadcast};
use tokio::time::{interval, timeout};
use tracing::{debug, error, info, warn, instrument};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use parking_lot::Mutex as ParkingMutex;
use crossbeam_channel::{Receiver, Sender};

/// Maximum number of concurrent streams per daemon instance
const MAX_CONCURRENT_STREAMS: u32 = 10000;

/// Default stream timeout in milliseconds
const DEFAULT_STREAM_TIMEOUT_MS: u32 = 30000;

/// Maximum number of paths per multipath stream
const MAX_PATHS_PER_STREAM: u8 = 8;

/// Stream management errors
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("Stream limit exceeded: {current}/{max}")]
    StreamLimitExceeded { current: u32, max: u32 },
    
    #[error("Stream not found: {stream_id}")]
    StreamNotFound { stream_id: u32 },
    
    #[error("Invalid target address: {address}")]
    InvalidTargetAddress { address: String },
    
    #[error("Path building failed: {reason}")]
    PathBuildingFailed { reason: String },
    
    #[error("Connection timeout after {timeout_ms}ms")]
    ConnectionTimeout { timeout_ms: u32 },
    
    #[error("Transport error: {source}")]
    Transport { source: anyhow::Error },
    
    #[error("Cryptographic error: {source}")]
    Crypto { source: anyhow::Error },
    
    #[error("Configuration error: {message}")]
    Configuration { message: String },
}

/// Stream session information
#[derive(Debug, Clone)]
pub struct StreamSession {
    pub stream_id: u32,
    pub cid: [u8; 12], // 96-bit Connection ID
    pub target_address: String,
    pub state: StreamState,
    pub created_at: SystemTime,
    pub last_activity: SystemTime,
    pub options: StreamOptions,
    pub statistics: StreamStatistics,
    pub paths: Vec<StreamPath>,
    pub crypter: Option<Arc<FrameCrypter>>,
}

/// Per-stream statistics
#[derive(Debug, Clone, Default)]
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
    pub rtt_samples: Arc<Mutex<Vec<Duration>>>,
    pub bandwidth_samples: Arc<Mutex<Vec<f64>>>,
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
#[derive(Debug, Clone, Default)]
pub struct PathStatistics {
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    pub packet_count: AtomicU32,
    pub success_count: AtomicU32,
    pub failure_count: AtomicU32,
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

/// Comprehensive stream manager
pub struct StreamManager {
    // Core components
    transport: Arc<Transport>,
    metrics: Arc<MetricsCollector>,
    
    // Stream tracking
    streams: Arc<DashMap<u32, StreamSession>>,
    next_stream_id: AtomicU32,
    
    // Path management
    path_builder: Arc<RwLock<WeightedPathBuilder<'static>>>,
    prober: Arc<Mutex<Prober>>,
    scheduler: Arc<Mutex<WeightedRrScheduler>>,
    
    // Network topology
    known_peers: Arc<DashMap<NodeId, PeerInfo>>,
    active_paths: Arc<DashMap<u8, PathInfo>>,
    
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
        metrics: Arc<MetricsCollector>,
        config: StreamManagerConfig,
    ) -> anyhow::Result<Self> {
        let (event_tx, _) = broadcast::channel(1000);
        
        // Initialize with empty candidates - will be populated by peer discovery
        let candidates: Vec<Candidate> = Vec::new();
        let path_builder = WeightedPathBuilder::new(&candidates, config.latency_bias);
        
        Ok(Self {
            transport,
            metrics,
            streams: Arc::new(DashMap::new()),
            next_stream_id: AtomicU32::new(1),
            path_builder: Arc::new(RwLock::new(path_builder)),
            prober: Arc::new(Mutex::new(Prober::new())),
            scheduler: Arc::new(Mutex::new(WeightedRrScheduler::new())),
            known_peers: Arc::new(DashMap::new()),
            active_paths: Arc::new(DashMap::new()),
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
    pub async fn open_stream(&self, request: OpenRequest) -> Result<StreamResponse, StreamError> {
        // Check stream limit
        let current_streams = self.streams.len() as u32;
        if current_streams >= self.config.max_concurrent_streams {
            return Err(StreamError::StreamLimitExceeded {
                current: current_streams,
                max: self.config.max_concurrent_streams,
            });
        }
        
        // Validate target address
        let target_addr = self.parse_target_address(&request.target_address)?;
        
        // Generate new stream ID and CID
        let stream_id = self.next_stream_id.fetch_add(1, Ordering::Relaxed);
        let cid = self.generate_cid();
        
        // Parse stream options
        let options = request.options.unwrap_or_default();
        let parsed_options = self.parse_stream_options(options)?;
        
        // Build network paths
        let paths = if parsed_options.multipath && self.config.enable_multipath {
            self.build_multipath_routes(&request.target_address, parsed_options.max_paths).await?
        } else {
            self.build_single_path_route(&request.target_address).await?
        };
        
        // Create stream session
        let session = StreamSession {
            stream_id,
            cid,
            target_address: request.target_address.clone(),
            state: StreamState::Idle,
            created_at: SystemTime::now(),
            last_activity: SystemTime::now(),
            options: parsed_options.clone(),
            statistics: StreamStatistics::default(),
            paths,
            crypter: None, // Will be initialized during handshake
        };
        
        // Store stream session
        self.streams.insert(stream_id, session.clone());
        
        // Update metrics
        self.metrics.set_active_streams(self.streams.len());
        
        // Emit stream opened event
        self.emit_stream_event(StreamEvent {
            stream_id,
            action: "opened".to_string(),
            target_address: request.target_address.clone(),
            stats: Some(self.build_stream_stats(&session).await),
        }).await;
        
        // Initialize stream connection
        let initial_stats = self.initialize_stream_connection(stream_id).await?;
        
        info!("Stream {} opened to {} with {} paths", 
              stream_id, request.target_address, session.paths.len());
        
        Ok(StreamResponse {
            stream_id,
            status: "opened".to_string(),
            target_address: request.target_address,
            initial_stats: Some(initial_stats),
        })
    }
    
    /// Close a stream
    #[instrument(skip(self), fields(stream_id = stream_id))]
    pub async fn close_stream(&self, stream_id: u32) -> Result<(), StreamError> {
        let session = self.streams.get(&stream_id)
            .ok_or(StreamError::StreamNotFound { stream_id })?;
        
        // Update stream state
        let mut session = session.clone();
        session.state = StreamState::Closed;
        session.last_activity = SystemTime::now();
        
        // Perform cleanup
        self.cleanup_stream_resources(stream_id).await;
        
        // Remove from active streams
        self.streams.remove(&stream_id);
        
        // Update metrics
        self.metrics.set_active_streams(self.streams.len());
        
        // Emit stream closed event
        self.emit_stream_event(StreamEvent {
            stream_id,
            action: "closed".to_string(),
            target_address: session.target_address.clone(),
            stats: Some(self.build_stream_stats(&session).await),
        }).await;
        
        info!("Stream {} closed", stream_id);
        Ok(())
    }
    
    /// Get stream statistics
    pub async fn get_stream_stats(&self, stream_id: u32) -> Result<StreamStats, StreamError> {
        let session = self.streams.get(&stream_id)
            .ok_or(StreamError::StreamNotFound { stream_id })?;
        
        Ok(self.build_stream_stats(&session).await)
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
        address.parse().map_err(|_| StreamError::InvalidTargetAddress {
            address: address.to_string(),
        })
    }
    
    /// Parse stream options from protobuf
    fn parse_stream_options(&self, options: crate::proto::StreamOptions) -> Result<ParsedStreamOptions, StreamError> {
        Ok(ParsedStreamOptions {
            buffer_size: if options.buffer_size > 0 { options.buffer_size } else { 65536 },
            timeout_ms: if options.timeout_ms > 0 { options.timeout_ms } else { self.config.default_timeout_ms },
            multipath: options.multipath,
            max_paths: std::cmp::min(options.max_paths, self.config.max_paths_per_stream as u32),
            path_strategy: self.parse_path_strategy(&options.path_strategy)?,
            auto_reconnect: options.auto_reconnect,
            max_retry_attempts: if options.max_retry_attempts > 0 { options.max_retry_attempts } else { 3 },
            compression: options.compression,
            cipher_suite: if options.cipher_suite.is_empty() { 
                "ChaCha20Poly1305".to_string() 
            } else { 
                options.cipher_suite 
            },
        })
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
        
        StreamStats {
            stream_id: session.stream_id,
            target_address: session.target_address.clone(),
            state: format!("{:?}", session.state).to_lowercase(),
            created_at: Some(prost_types::Timestamp::from(session.created_at)),
            last_activity: Some(prost_types::Timestamp::from(session.last_activity)),
            bytes_sent: stats.bytes_sent.load(Ordering::Relaxed),
            bytes_received: stats.bytes_received.load(Ordering::Relaxed),
            packets_sent: stats.packets_sent.load(Ordering::Relaxed),
            packets_received: stats.packets_received.load(Ordering::Relaxed),
            retransmissions: stats.retransmissions.load(Ordering::Relaxed),
            avg_rtt_ms: avg_rtt,
            min_rtt_ms: min_rtt,
            max_rtt_ms: max_rtt,
            bandwidth_mbps,
            packet_loss_rate: 0.0, // Would be calculated from actual data
            paths: path_stats,
            connection_errors: stats.connection_errors.load(Ordering::Relaxed),
            timeout_errors: stats.timeout_errors.load(Ordering::Relaxed),
            last_error: last_error.unwrap_or_default(),
            last_error_at: last_error_at.map(prost_types::Timestamp::from),
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