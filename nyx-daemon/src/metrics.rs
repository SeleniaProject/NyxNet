use std::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, Instant};
use std::collections::HashMap;

use sysinfo::{System, Pid};
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn, error};
// Registry is handled internally by metrics-exporter-prometheus

// Import alert system types
use crate::alert_system::{AlertStatistics};

/// Enhanced metrics collector for comprehensive daemon performance monitoring
#[derive(Debug)]
pub struct MetricsCollector {
    // Basic counters
    pub total_requests: Arc<AtomicU64>,
    pub successful_requests: Arc<AtomicU64>,
    pub failed_requests: Arc<AtomicU64>,
    pub bytes_sent: Arc<AtomicU64>,
    pub bytes_received: Arc<AtomicU64>,
    pub packets_sent: Arc<AtomicU64>,
    pub packets_received: Arc<AtomicU64>,
    pub retransmissions: Arc<AtomicU64>,
    
    // Connection tracking
    pub active_streams: Arc<AtomicU32>,
    pub connected_peers: Arc<AtomicU32>,
    pub total_connections: Arc<AtomicU64>,
    pub failed_connections: Arc<AtomicU64>,
    
    // Performance metrics
    pub cover_traffic_rate: Arc<RwLock<f64>>,
    pub avg_latency_ms: Arc<RwLock<f64>>,
    pub packet_loss_rate: Arc<RwLock<f64>>,
    pub bandwidth_utilization: Arc<RwLock<f64>>,
    
    // System monitoring
    pub system: Arc<RwLock<System>>,
    pub start_time: SystemTime,
    pub last_update: Arc<RwLock<Instant>>,
    
    // Mix routes tracking
    pub mix_routes: Arc<RwLock<Vec<String>>>,
    
    // Latency tracking for averages
    pub latency_samples: Arc<RwLock<Vec<f64>>>,
    pub max_latency_samples: usize,
    
    // Additional performance tracking
    pub bandwidth_samples: Arc<RwLock<Vec<f64>>>,
    pub cpu_samples: Arc<RwLock<Vec<f64>>>,
    pub memory_samples: Arc<RwLock<Vec<f64>>>,
    pub connection_attempts: Arc<AtomicU64>,
    pub successful_connections: Arc<AtomicU64>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        
        Self {
            // Basic counters
            total_requests: Arc::new(AtomicU64::new(0)),
            successful_requests: Arc::new(AtomicU64::new(0)),
            failed_requests: Arc::new(AtomicU64::new(0)),
            bytes_sent: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            packets_sent: Arc::new(AtomicU64::new(0)),
            packets_received: Arc::new(AtomicU64::new(0)),
            retransmissions: Arc::new(AtomicU64::new(0)),
            
            // Connection tracking
            active_streams: Arc::new(AtomicU32::new(0)),
            connected_peers: Arc::new(AtomicU32::new(0)),
            total_connections: Arc::new(AtomicU64::new(0)),
            failed_connections: Arc::new(AtomicU64::new(0)),
            
            // Performance metrics
            cover_traffic_rate: Arc::new(RwLock::new(0.0)),
            avg_latency_ms: Arc::new(RwLock::new(0.0)),
            packet_loss_rate: Arc::new(RwLock::new(0.0)),
            bandwidth_utilization: Arc::new(RwLock::new(0.0)),
            
            // System monitoring
            system: Arc::new(RwLock::new(system)),
            start_time: SystemTime::now(),
            last_update: Arc::new(RwLock::new(Instant::now())),
            
            // Mix routes tracking
            mix_routes: Arc::new(RwLock::new(Vec::new())),
            
            // Latency tracking
            latency_samples: Arc::new(RwLock::new(Vec::new())),
            max_latency_samples: 1000, // Keep last 1000 samples for averaging
            
            // Additional performance tracking
            bandwidth_samples: Arc::new(RwLock::new(Vec::new())),
            cpu_samples: Arc::new(RwLock::new(Vec::new())),
            memory_samples: Arc::new(RwLock::new(Vec::new())),
            connection_attempts: Arc::new(AtomicU64::new(0)),
            successful_connections: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn start_collection(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let system = Arc::clone(&self.system);
        let last_update = Arc::clone(&self.last_update);
        let cover_traffic_rate = Arc::clone(&self.cover_traffic_rate);
        let bandwidth_utilization = Arc::clone(&self.bandwidth_utilization);
        let packet_loss_rate = Arc::clone(&self.packet_loss_rate);
        let bytes_sent = Arc::clone(&self.bytes_sent);
        let bytes_received = Arc::clone(&self.bytes_received);
        let packets_sent = Arc::clone(&self.packets_sent);
        let packets_received = Arc::clone(&self.packets_received);
        let retransmissions = Arc::clone(&self.retransmissions);
        let bandwidth_samples = Arc::clone(&self.bandwidth_samples);
        let cpu_samples = Arc::clone(&self.cpu_samples);
        let memory_samples = Arc::clone(&self.memory_samples);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            let mut last_bytes_sent = 0u64;
            let mut last_bytes_received = 0u64;
            let mut last_packets_sent = 0u64;
            let mut last_packets_received = 0u64;
            let mut last_retransmissions = 0u64;
            
            loop {
                interval.tick().await;
                
                // Update system information
                {
                    let mut sys = system.write().await;
                    sys.refresh_all();
                }
                
                // Calculate rates
                let current_bytes_sent = bytes_sent.load(Ordering::Relaxed);
                let current_bytes_received = bytes_received.load(Ordering::Relaxed);
                let current_packets_sent = packets_sent.load(Ordering::Relaxed);
                let current_packets_received = packets_received.load(Ordering::Relaxed);
                let current_retransmissions = retransmissions.load(Ordering::Relaxed);
                
                // Calculate cover traffic rate (packets per second)
                let packets_delta = (current_packets_sent - last_packets_sent) as f64;
                {
                    let mut rate = cover_traffic_rate.write().await;
                    *rate = packets_delta; // packets per second
                }
                
                // Calculate bandwidth utilization (simplified)
                let bytes_delta = (current_bytes_sent + current_bytes_received) - 
                                 (last_bytes_sent + last_bytes_received);
                let bandwidth_mbps = (bytes_delta as f64 * 8.0) / (1024.0 * 1024.0); // Convert to Mbps
                
                // Store bandwidth sample
                {
                    let mut samples = bandwidth_samples.write().await;
                    samples.push(bandwidth_mbps);
                    if samples.len() > 100 { // Keep last 100 samples
                        samples.remove(0);
                    }
                }
                
                {
                    let mut util = bandwidth_utilization.write().await;
                    *util = (bandwidth_mbps / 1000.0).min(1.0); // Assume 1Gbps max, normalize to 0-1
                }
                
                // Update system metrics and store samples
                {
                    let mut sys = system.write().await;
                    sys.refresh_cpu();
                    sys.refresh_memory();
                    
                    let cpu_usage = sys.global_cpu_info().cpu_usage() as f64;
                    let memory_usage = (sys.used_memory() as f64 / sys.total_memory() as f64) * 100.0;
                    
                    // Store CPU samples
                    {
                        let mut samples = cpu_samples.write().await;
                        samples.push(cpu_usage);
                        if samples.len() > 100 {
                            samples.remove(0);
                        }
                    }
                    
                    // Store memory samples
                    {
                        let mut samples = memory_samples.write().await;
                        samples.push(memory_usage);
                        if samples.len() > 100 {
                            samples.remove(0);
                        }
                    }
                }
                
                // Calculate packet loss rate
                let total_packets = current_packets_sent + current_packets_received;
                let loss_rate = if total_packets > 0 {
                    current_retransmissions as f64 / total_packets as f64
                } else {
                    0.0
                };
                {
                    let mut loss = packet_loss_rate.write().await;
                    *loss = loss_rate;
                }
                
                // Update last values
                last_bytes_sent = current_bytes_sent;
                last_bytes_received = current_bytes_received;
                last_packets_sent = current_packets_sent;
                last_packets_received = current_packets_received;
                last_retransmissions = current_retransmissions;
                
                // Update last update time
                {
                    let mut update_time = last_update.write().await;
                    *update_time = Instant::now();
                }
            }
        })
    }

    // Basic counter methods
    pub fn increment_requests(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_successful_requests(&self) {
        self.successful_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_failed_requests(&self) {
        self.failed_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_packets_sent(&self) {
        self.packets_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_packets_received(&self) {
        self.packets_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_retransmissions(&self) {
        self.retransmissions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_bytes_sent(&self, bytes: u64) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_bytes_sent(&self, bytes: u64) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_bytes_received(&self, bytes: u64) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }

    // Stream and connection management
    pub fn increment_active_streams(&self) {
        self.active_streams.fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement_active_streams(&self) {
        self.active_streams.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn set_active_streams(&self, count: u32) {
        self.active_streams.store(count, Ordering::Relaxed);
    }

    pub fn increment_connected_peers(&self) {
        self.connected_peers.fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement_connected_peers(&self) {
        self.connected_peers.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn set_connected_peers(&self, count: u32) {
        self.connected_peers.store(count, Ordering::Relaxed);
    }

    pub fn increment_connections(&self) {
        self.total_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_failed_connections(&self) {
        self.failed_connections.fetch_add(1, Ordering::Relaxed);
    }

    // Mix routes management
    pub async fn add_mix_route(&self, route: String) {
        let mut routes = self.mix_routes.write().await;
        if !routes.contains(&route) {
            routes.push(route);
        }
    }

    pub async fn remove_mix_route(&self, route: &str) {
        let mut routes = self.mix_routes.write().await;
        routes.retain(|r| r != route);
    }

    pub async fn get_mix_routes(&self) -> Vec<String> {
        self.mix_routes.read().await.clone()
    }

    // Connection tracking
    pub fn record_connection_attempt(&self) {
        self.connection_attempts.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_successful_connection(&self) {
        self.successful_connections.fetch_add(1, Ordering::Relaxed);
    }
    
    // Latency tracking
    pub async fn record_latency(&self, latency_ms: f64) {
        let mut samples = self.latency_samples.write().await;
        samples.push(latency_ms);
        
        // Keep only the most recent samples
        if samples.len() > self.max_latency_samples {
            samples.remove(0);
        }
        
        // Update average latency
        let avg = samples.iter().sum::<f64>() / samples.len() as f64;
        drop(samples); // Release the lock before acquiring another one
        
        let mut avg_latency = self.avg_latency_ms.write().await;
        *avg_latency = avg;
    }
    
    // Bandwidth tracking
    pub async fn record_bandwidth(&self, bandwidth_mbps: f64) {
        let mut samples = self.bandwidth_samples.write().await;
        samples.push(bandwidth_mbps);
        if samples.len() > 100 {
            samples.remove(0);
        }
    }
    
    // Get performance statistics
    pub async fn get_bandwidth_samples(&self) -> Vec<f64> {
        self.bandwidth_samples.read().await.clone()
    }
    
    pub async fn get_cpu_samples(&self) -> Vec<f64> {
        self.cpu_samples.read().await.clone()
    }
    
    pub async fn get_memory_samples(&self) -> Vec<f64> {
        self.memory_samples.read().await.clone()
    }

    pub fn get_uptime(&self) -> Duration {
        SystemTime::now().duration_since(self.start_time).unwrap_or_default()
    }

    pub async fn get_performance_metrics(&self) -> crate::proto::PerformanceMetrics {
        let cover_traffic_rate = *self.cover_traffic_rate.read().await;
        let avg_latency_ms = *self.avg_latency_ms.read().await;
        let packet_loss_rate = *self.packet_loss_rate.read().await;
        let bandwidth_utilization = *self.bandwidth_utilization.read().await;

        // Get CPU and memory usage from system
        let (cpu_usage, memory_usage_mb) = {
            let system = self.system.read().await;
            let current_pid = std::process::id();
            if let Some(process) = system.process(sysinfo::Pid::from(current_pid as usize)) {
                (
                    process.cpu_usage() as f64 / 100.0, // Convert to 0-1 range
                    process.memory() as f64 / (1024.0 * 1024.0), // Convert to MB
                )
            } else {
                (0.0, 0.0)
            }
        };

        // Calculate connection success rate using new tracking
        let total_attempts = self.connection_attempts.load(Ordering::Relaxed);
        let successful = self.successful_connections.load(Ordering::Relaxed);
        let success_rate = if total_attempts > 0 {
            successful as f64 / total_attempts as f64
        } else {
            1.0
        };

        crate::proto::PerformanceMetrics {
            cover_traffic_rate,
            avg_latency_ms,
            packet_loss_rate,
            bandwidth_utilization,
            cpu_usage,
            memory_usage_mb,
            total_packets_sent: self.packets_sent.load(Ordering::Relaxed),
            total_packets_received: self.packets_received.load(Ordering::Relaxed),
            retransmissions: self.retransmissions.load(Ordering::Relaxed),
            connection_success_rate: success_rate,
        }
    }

    pub async fn get_resource_usage(&self) -> Option<crate::proto::ResourceUsage> {
        {
            let system = self.system.read().await;
            let current_pid = std::process::id();
            if let Some(process) = system.process(Pid::from(current_pid as usize)) {
                Some(crate::proto::ResourceUsage {
                    memory_rss_bytes: process.memory() * 1024, // Convert from KB to bytes
                    memory_vms_bytes: process.virtual_memory() * 1024, // Convert from KB to bytes
                    cpu_percent: process.cpu_usage() as f64,
                    open_file_descriptors: 0, // Would need platform-specific implementation
                    thread_count: 0, // Would need platform-specific implementation
                    network_bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
                    network_bytes_received: self.bytes_received.load(Ordering::Relaxed),
                })
            } else {
                None
            }
        }
    }

    pub fn get_active_streams_count(&self) -> u64 {
        self.active_streams.load(Ordering::Relaxed) as u64
    }

    pub fn get_connected_peers_count(&self) -> u64 {
        self.connected_peers.load(Ordering::Relaxed) as u64
    }
}

/// Layer types for comprehensive metrics collection
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LayerType {
    Crypto,
    Stream,
    Mix,
    Fec,
    Transport,
}

/// Layer status for health monitoring
#[derive(Debug, Clone, PartialEq)]
pub enum LayerStatus {
    Initializing,
    Active,
    Degraded,
    Failed,
    Shutdown,
}

/// Detailed metrics for each protocol layer
#[derive(Debug, Clone)]
pub struct LayerMetrics {
    pub layer_type: LayerType,
    pub status: LayerStatus,
    pub throughput: f64,
    pub latency: Duration,
    pub error_rate: f64,
    pub resource_usage: ResourceUsage,
    pub packets_processed: u64,
    pub bytes_processed: u64,
    pub active_connections: u32,
    pub queue_depth: u32,
    pub last_updated: SystemTime,
}

/// System resource usage metrics
#[derive(Debug, Clone, Default)]
pub struct SystemMetrics {
    pub cpu_usage_percent: f64,
    pub memory_usage_bytes: u64,
    pub memory_total_bytes: u64,
    pub network_bytes_sent: u64,
    pub network_bytes_received: u64,
    pub open_file_descriptors: u32,
    pub thread_count: u32,
    pub disk_usage_bytes: u64,
    pub load_average: f64,
    pub uptime_seconds: u64,
}

/// Network-specific metrics
#[derive(Debug, Clone, Default)]
pub struct NetworkMetrics {
    pub active_connections: u32,
    pub total_connections: u64,
    pub failed_connections: u64,
    pub connection_success_rate: f64,
    pub average_latency: Duration,
    pub packet_loss_rate: f64,
    pub bandwidth_utilization: f64,
    pub peer_count: u32,
    pub route_count: u32,
}

/// Error classification and metrics
#[derive(Debug, Clone, Default)]
pub struct ErrorMetrics {
    pub total_errors: u64,
    pub errors_by_layer: HashMap<LayerType, u64>,
    pub errors_by_type: HashMap<String, u64>,
    pub error_rate: f64,
    pub recent_errors: Vec<ErrorInfo>,
    pub critical_errors: u64,
    pub warning_errors: u64,
}

/// Individual error information
#[derive(Debug, Clone)]
pub struct ErrorInfo {
    pub timestamp: SystemTime,
    pub layer: LayerType,
    pub error_type: String,
    pub message: String,
    pub context: HashMap<String, String>,
    pub severity: ErrorSeverity,
}

/// Error severity levels
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorSeverity {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

/// Resource usage details
#[derive(Debug, Clone, Default)]
pub struct ResourceUsage {
    pub cpu_percent: f64,
    pub memory_bytes: u64,
    pub network_bytes_sent: u64,
    pub network_bytes_received: u64,
    pub file_descriptors: u32,
    pub threads: u32,
}

/// Performance analysis results
#[derive(Debug, Clone)]
pub struct PerformanceAnalysis {
    pub overall_health: f64, // 0.0 to 1.0
    pub bottlenecks: Vec<String>,
    pub recommendations: Vec<String>,
    pub trends: HashMap<String, f64>,
    pub anomalies: Vec<String>,
}

/// Alert information
#[derive(Debug, Clone)]
pub struct Alert {
    pub id: String,
    pub timestamp: SystemTime,
    pub severity: AlertSeverity,
    pub title: String,
    pub description: String,
    pub metric: String,
    pub current_value: f64,
    pub threshold: f64,
    pub layer: Option<LayerType>,
    pub context: HashMap<String, String>,
    pub resolved: bool,
    pub resolved_at: Option<SystemTime>,
}

/// Alert severity levels
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

/// Alert configuration for threshold monitoring
#[derive(Debug, Clone)]
pub struct AlertThreshold {
    pub metric: String,
    pub threshold: f64,
    pub severity: AlertSeverity,
    pub comparison: ThresholdComparison,
    pub layer: Option<LayerType>,
    pub enabled: bool,
    pub cooldown_duration: Duration,
    pub last_triggered: Option<SystemTime>,
}

/// Threshold comparison types
#[derive(Debug, Clone, PartialEq)]
pub enum ThresholdComparison {
    GreaterThan,
    LessThan,
    Equal,
}

/// Alert routing configuration
#[derive(Debug, Clone)]
pub struct AlertRoute {
    pub severity_filter: Vec<AlertSeverity>,
    pub layer_filter: Vec<LayerType>,
    pub handler: AlertHandler,
}

/// Alert handler types
#[derive(Debug, Clone)]
pub enum AlertHandler {
    Log,
    Email(String),
    Webhook(String),
    Console,
}

/// Alert suppression rule
#[derive(Debug, Clone)]
pub struct SuppressionRule {
    pub id: String,
    pub metric_pattern: String,
    pub layer: Option<LayerType>,
    pub duration: Duration,
    pub max_alerts: u32,
    pub created_at: SystemTime,
}

/// Alert history entry
#[derive(Debug, Clone)]
pub struct AlertHistoryEntry {
    pub alert: Alert,
    pub action: AlertAction,
    pub timestamp: SystemTime,
}

/// Alert actions for history tracking
#[derive(Debug, Clone, PartialEq)]
pub enum AlertAction {
    Created,
    Resolved,
    Suppressed,
    Escalated,
}

/// Comprehensive metrics snapshot
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub timestamp: SystemTime,
    pub layer_metrics: HashMap<LayerType, LayerMetrics>,
    pub system_metrics: SystemMetrics,
    pub network_metrics: NetworkMetrics,
    pub error_metrics: ErrorMetrics,
    pub performance_metrics: crate::proto::PerformanceMetrics,
}

/// Comprehensive metrics collector that aggregates all layer metrics
pub struct ComprehensiveMetrics {
    layer_metrics: Arc<RwLock<HashMap<LayerType, LayerMetrics>>>,
    system_metrics: Arc<RwLock<SystemMetrics>>,
    network_metrics: Arc<RwLock<NetworkMetrics>>,
    error_metrics: Arc<RwLock<ErrorMetrics>>,
    
    // Historical data for trend analysis
    metrics_history: Arc<RwLock<Vec<MetricsSnapshot>>>,
    max_history_size: usize,
    
    // Alert system
    alert_thresholds: Arc<RwLock<HashMap<String, AlertThreshold>>>,
    active_alerts: Arc<RwLock<HashMap<String, Alert>>>,
    alert_history: Arc<RwLock<Vec<AlertHistoryEntry>>>,
    alert_routes: Arc<RwLock<Vec<AlertRoute>>>,
    suppression_rules: Arc<RwLock<Vec<SuppressionRule>>>,
    alert_sender: broadcast::Sender<Alert>,
    
    // System monitoring
    system: Arc<RwLock<System>>,
    start_time: SystemTime,
    
    // Base metrics collector for compatibility
    base_collector: Arc<MetricsCollector>,
}

impl Default for LayerType {
    fn default() -> Self {
        LayerType::Transport
    }
}

impl Default for LayerStatus {
    fn default() -> Self {
        LayerStatus::Initializing
    }
}

impl Default for LayerMetrics {
    fn default() -> Self {
        Self {
            layer_type: LayerType::default(),
            status: LayerStatus::default(),
            throughput: 0.0,
            latency: Duration::from_millis(0),
            error_rate: 0.0,
            resource_usage: ResourceUsage::default(),
            packets_processed: 0,
            bytes_processed: 0,
            active_connections: 0,
            queue_depth: 0,
            last_updated: SystemTime::UNIX_EPOCH,
        }
    }
}

impl ComprehensiveMetrics {
    /// Create a new comprehensive metrics collector
    pub fn new(base_collector: Arc<MetricsCollector>) -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        
        let (alert_sender, alert_receiver) = broadcast::channel(1000);
        
        Self {
            layer_metrics: Arc::new(RwLock::new(HashMap::new())),
            system_metrics: Arc::new(RwLock::new(SystemMetrics::default())),
            network_metrics: Arc::new(RwLock::new(NetworkMetrics::default())),
            error_metrics: Arc::new(RwLock::new(ErrorMetrics::default())),
            metrics_history: Arc::new(RwLock::new(Vec::new())),
            max_history_size: 1000,
            alert_thresholds: Arc::new(RwLock::new(HashMap::new())),
            active_alerts: Arc::new(RwLock::new(HashMap::new())),
            alert_history: Arc::new(RwLock::new(Vec::new())),
            alert_routes: Arc::new(RwLock::new(Vec::new())),
            suppression_rules: Arc::new(RwLock::new(Vec::new())),
            alert_sender,
            system: Arc::new(RwLock::new(system)),
            start_time: SystemTime::now(),
            base_collector,
        }
    }
    
    /// Collect all metrics from all layers
    pub async fn collect_all(&self) -> MetricsSnapshot {
        // Update system metrics
        self.update_system_metrics().await;
        
        // Update network metrics from base collector
        self.update_network_metrics().await;
        
        // Update layer metrics
        self.update_layer_metrics().await;
        
        // Create snapshot
        let snapshot = MetricsSnapshot {
            timestamp: SystemTime::now(),
            layer_metrics: self.layer_metrics.read().await.clone(),
            system_metrics: self.system_metrics.read().await.clone(),
            network_metrics: self.network_metrics.read().await.clone(),
            error_metrics: self.error_metrics.read().await.clone(),
            performance_metrics: self.base_collector.get_performance_metrics().await,
        };
        
        // Store in history
        let mut history = self.metrics_history.write().await;
        history.push(snapshot.clone());
        if history.len() > self.max_history_size {
            history.remove(0);
        }
        
        // Check for alerts
        self.check_alerts(&snapshot).await;
        
        snapshot
    }
    
    /// Update system metrics
    async fn update_system_metrics(&self) {
        {
            let mut system = self.system.write().await;
            system.refresh_all();
            
            let uptime = SystemTime::now()
                .duration_since(self.start_time)
                .unwrap_or_default()
                .as_secs();
            
            let mut metrics = self.system_metrics.write().await;
            metrics.cpu_usage_percent = system.global_cpu_info().cpu_usage() as f64;
            metrics.memory_usage_bytes = system.used_memory();
            metrics.memory_total_bytes = system.total_memory();
            metrics.uptime_seconds = uptime;
            
            // Get process-specific metrics
            let current_pid = std::process::id();
            if let Some(process) = system.process(Pid::from(current_pid as usize)) {
                metrics.open_file_descriptors = 0; // Would need platform-specific implementation
                metrics.thread_count = 0; // Would need platform-specific implementation
            }
            
            // Network metrics from base collector
            metrics.network_bytes_sent = self.base_collector.bytes_sent.load(Ordering::Relaxed);
            metrics.network_bytes_received = self.base_collector.bytes_received.load(Ordering::Relaxed);
        }
    }
    
    /// Update network metrics
    async fn update_network_metrics(&self) {
        let mut metrics = self.network_metrics.write().await;
        
        metrics.active_connections = self.base_collector.active_streams.load(Ordering::Relaxed);
        metrics.total_connections = self.base_collector.total_connections.load(Ordering::Relaxed);
        metrics.failed_connections = self.base_collector.failed_connections.load(Ordering::Relaxed);
        
        let total_attempts = self.base_collector.connection_attempts.load(Ordering::Relaxed);
        let successful = self.base_collector.successful_connections.load(Ordering::Relaxed);
        metrics.connection_success_rate = if total_attempts > 0 {
            successful as f64 / total_attempts as f64
        } else {
            1.0
        };
        
        metrics.average_latency = Duration::from_millis(
            *self.base_collector.avg_latency_ms.read().await as u64
        );
        metrics.packet_loss_rate = *self.base_collector.packet_loss_rate.read().await;
        metrics.bandwidth_utilization = *self.base_collector.bandwidth_utilization.read().await;
        metrics.peer_count = self.base_collector.connected_peers.load(Ordering::Relaxed);
        metrics.route_count = self.base_collector.get_mix_routes().await.len() as u32;
    }
    
    /// Update layer-specific metrics
    async fn update_layer_metrics(&self) {
        let mut layer_metrics = self.layer_metrics.write().await;
        
        // Update each layer's metrics
        for layer_type in [LayerType::Crypto, LayerType::Stream, LayerType::Mix, LayerType::Fec, LayerType::Transport] {
            let metrics = layer_metrics.entry(layer_type.clone()).or_insert_with(|| {
                LayerMetrics {
                    layer_type: layer_type.clone(),
                    status: LayerStatus::default(),
                    throughput: 0.0,
                    latency: Duration::from_millis(0),
                    error_rate: 0.0,
                    resource_usage: ResourceUsage::default(),
                    packets_processed: 0,
                    bytes_processed: 0,
                    active_connections: 0,
                    queue_depth: 0,
                    last_updated: SystemTime::UNIX_EPOCH,
                }
            });
            
            // Update common metrics from base collector
            metrics.bytes_processed = match layer_type {
                LayerType::Transport => {
                    self.base_collector.bytes_sent.load(Ordering::Relaxed) +
                    self.base_collector.bytes_received.load(Ordering::Relaxed)
                },
                _ => metrics.bytes_processed, // Would be updated by specific layer implementations
            };
            
            metrics.packets_processed = match layer_type {
                LayerType::Transport => {
                    self.base_collector.packets_sent.load(Ordering::Relaxed) +
                    self.base_collector.packets_received.load(Ordering::Relaxed)
                },
                _ => metrics.packets_processed,
            };
            
            metrics.active_connections = self.base_collector.active_streams.load(Ordering::Relaxed);
            metrics.latency = Duration::from_millis(
                *self.base_collector.avg_latency_ms.read().await as u64
            );
            metrics.last_updated = SystemTime::now();
            
            // Set status based on error rates and activity
            metrics.status = if metrics.error_rate > 10.0 {
                LayerStatus::Failed
            } else if metrics.error_rate > 5.0 {
                LayerStatus::Degraded
            } else if metrics.packets_processed > 0 || metrics.active_connections > 0 {
                LayerStatus::Active
            } else {
                LayerStatus::Initializing
            };
        }
    }
    
    /// Check for alert conditions using comprehensive alert system
    async fn check_alerts(&self, snapshot: &MetricsSnapshot) {
        use crate::alert_system::AlertSystem;
        
        // Create a temporary alert system for checking thresholds
        let alert_system = AlertSystem::new();
        
        // Check thresholds and generate alerts
        let new_alerts = alert_system.check_thresholds(snapshot).await;
        
        // Process each new alert
        for alert in new_alerts {
            // Add to active alerts
            {
                let mut active_alerts = self.active_alerts.write().await;
                active_alerts.insert(alert.id.clone(), alert.clone());
            }
            
            // Add to history
            {
                let mut history = self.alert_history.write().await;
                history.push(AlertHistoryEntry {
                    alert: alert.clone(),
                    action: AlertAction::Created,
                    timestamp: SystemTime::now(),
                });
                
                // Trim history if needed
                if history.len() > 10000 {
                    history.remove(0);
                }
            }
            
            // Route alert through configured handlers
            self.route_alert(&alert).await;
            
            // Broadcast alert
            let _ = self.alert_sender.send(alert);
        }
    }
    
    /// Route alert through configured handlers
    async fn route_alert(&self, alert: &Alert) {
        let routes = self.alert_routes.read().await;
        
        for route in routes.iter() {
            // Check severity filter
            if !route.severity_filter.is_empty() && !route.severity_filter.contains(&alert.severity) {
                continue;
            }
            
            // Check layer filter
            if !route.layer_filter.is_empty() {
                if let Some(alert_layer) = &alert.layer {
                    if !route.layer_filter.contains(alert_layer) {
                        continue;
                    }
                } else {
                    continue;
                }
            }
            
            // Handle alert based on handler type
            self.handle_alert(alert, &route.handler).await;
        }
        
        // Default console output if no routes configured
        if routes.is_empty() {
            println!("[ALERT] {} - {} - {}", 
                match alert.severity {
                    AlertSeverity::Info => "INFO",
                    AlertSeverity::Warning => "WARN",
                    AlertSeverity::Critical => "CRIT",
                },
                alert.title,
                alert.description
            );
        }
    }
    
    /// Handle alert based on handler type
    async fn handle_alert(&self, alert: &Alert, handler: &AlertHandler) {
        match handler {
            AlertHandler::Console => {
                println!("[ALERT] {} - {} - {}", 
                    match alert.severity {
                        AlertSeverity::Info => "INFO",
                        AlertSeverity::Warning => "WARN",
                        AlertSeverity::Critical => "CRIT",
                    },
                    alert.title,
                    alert.description
                );
            },
            AlertHandler::Log => {
                log::warn!("Alert: {} - {} - {}", alert.title, alert.description, alert.current_value);
            },
            AlertHandler::Email(email) => {
                log::info!("Would send email alert to {}: {}", email, alert.title);
            },
            AlertHandler::Webhook(url) => {
                log::info!("Would send webhook to {}: {}", url, alert.title);
            },
        }
    }
    
    /// Record an error for metrics tracking
    pub async fn record_error(&self, layer: LayerType, error_type: String, message: String, severity: ErrorSeverity) {
        let mut error_metrics = self.error_metrics.write().await;
        
        error_metrics.total_errors += 1;
        *error_metrics.errors_by_layer.entry(layer.clone()).or_insert(0) += 1;
        *error_metrics.errors_by_type.entry(error_type.clone()).or_insert(0) += 1;
        
        match severity {
            ErrorSeverity::Critical => error_metrics.critical_errors += 1,
            ErrorSeverity::Warning => error_metrics.warning_errors += 1,
            _ => {}
        }
        
        let error_info = ErrorInfo {
            timestamp: SystemTime::now(),
            layer,
            error_type,
            message,
            context: HashMap::new(),
            severity,
        };
        
        error_metrics.recent_errors.push(error_info);
        
        // Keep only recent errors (last 100)
        if error_metrics.recent_errors.len() > 100 {
            error_metrics.recent_errors.remove(0);
        }
        
        // Update error rate (errors per minute)
        let now = SystemTime::now();
        let one_minute_ago = now - Duration::from_secs(60);
        let recent_error_count = error_metrics.recent_errors.iter()
            .filter(|e| e.timestamp > one_minute_ago)
            .count();
        error_metrics.error_rate = recent_error_count as f64;
    }
    
    /// Generate performance analysis
    pub async fn analyze_performance(&self) -> PerformanceAnalysis {
        let snapshot = self.collect_all().await;
        let history = self.metrics_history.read().await;
        
        let mut bottlenecks = Vec::new();
        let mut recommendations = Vec::new();
        let mut trends = HashMap::new();
        let mut anomalies = Vec::new();
        
        // Analyze CPU usage
        if snapshot.system_metrics.cpu_usage_percent > 80.0 {
            bottlenecks.push("High CPU usage detected".to_string());
            recommendations.push("Consider optimizing CPU-intensive operations".to_string());
        }
        
        // Analyze memory usage
        let memory_percent = (snapshot.system_metrics.memory_usage_bytes as f64 / 
                            snapshot.system_metrics.memory_total_bytes as f64) * 100.0;
        if memory_percent > 85.0 {
            bottlenecks.push("High memory usage detected".to_string());
            recommendations.push("Consider implementing memory optimization strategies".to_string());
        }
        
        // Analyze network performance
        if snapshot.network_metrics.packet_loss_rate > 1.0 {
            bottlenecks.push("High packet loss rate detected".to_string());
            recommendations.push("Check network connectivity and congestion".to_string());
        }
        
        // Calculate trends if we have historical data
        if history.len() > 10 {
            let recent_snapshots = &history[history.len()-10..];
            
            // CPU trend
            let cpu_values: Vec<f64> = recent_snapshots.iter()
                .map(|s| s.system_metrics.cpu_usage_percent)
                .collect();
            let cpu_trend = self.calculate_trend(&cpu_values);
            trends.insert("cpu_usage_trend".to_string(), cpu_trend);
            
            // Memory trend
            let memory_values: Vec<f64> = recent_snapshots.iter()
                .map(|s| (s.system_metrics.memory_usage_bytes as f64 / 
                         s.system_metrics.memory_total_bytes as f64) * 100.0)
                .collect();
            let memory_trend = self.calculate_trend(&memory_values);
            trends.insert("memory_usage_trend".to_string(), memory_trend);
            
            // Latency trend
            let latency_values: Vec<f64> = recent_snapshots.iter()
                .map(|s| s.network_metrics.average_latency.as_millis() as f64)
                .collect();
            let latency_trend = self.calculate_trend(&latency_values);
            trends.insert("latency_trend".to_string(), latency_trend);
        }
        
        // Calculate overall health score
        let mut health_score = 1.0;
        
        // Deduct for high resource usage
        if snapshot.system_metrics.cpu_usage_percent > 80.0 {
            health_score -= 0.2;
        }
        if memory_percent > 85.0 {
            health_score -= 0.2;
        }
        if snapshot.network_metrics.packet_loss_rate > 1.0 {
            health_score -= 0.3;
        }
        if snapshot.error_metrics.error_rate > 5.0 {
            health_score -= 0.3;
        }
        
        health_score = f64::max(health_score, 0.0);
        
        PerformanceAnalysis {
            overall_health: health_score,
            bottlenecks,
            recommendations,
            trends,
            anomalies,
        }
    }
    
    /// Calculate trend from a series of values (positive = increasing, negative = decreasing)
    fn calculate_trend(&self, values: &[f64]) -> f64 {
        if values.len() < 2 {
            return 0.0;
        }
        
        let first_half = &values[0..values.len()/2];
        let second_half = &values[values.len()/2..];
        
        let first_avg = first_half.iter().sum::<f64>() / first_half.len() as f64;
        let second_avg = second_half.iter().sum::<f64>() / second_half.len() as f64;
        
        ((second_avg - first_avg) / first_avg) * 100.0
    }
    
    /// Generate alerts based on current metrics
    pub async fn generate_alerts(&self) -> Vec<Alert> {
        let active_alerts = self.active_alerts.read().await;
        active_alerts.values().cloned().collect()
    }
    
    /// Subscribe to alerts
    pub fn subscribe_alerts(&self) -> broadcast::Receiver<Alert> {
        self.alert_sender.subscribe()
    }
    
    /// Update alert threshold
    pub async fn set_alert_threshold(&self, metric: String, threshold: f64) {
        let mut thresholds = self.alert_thresholds.write().await;
        let threshold_item = AlertThreshold {
            metric: metric.clone(),
            threshold,
            severity: AlertSeverity::Warning,
            comparison: ThresholdComparison::GreaterThan,
            layer: None,
            enabled: true,
            cooldown_duration: Duration::from_secs(300),
            last_triggered: None,
        };
        thresholds.insert(metric, threshold_item);
    }
    
    /// Add alert threshold with full configuration
    pub async fn add_alert_threshold(&self, threshold_id: String, threshold: AlertThreshold) {
        let mut thresholds = self.alert_thresholds.write().await;
        thresholds.insert(threshold_id, threshold);
    }
    
    /// Remove alert threshold
    pub async fn remove_alert_threshold(&self, threshold_id: &str) -> bool {
        let mut thresholds = self.alert_thresholds.write().await;
        thresholds.remove(threshold_id).is_some()
    }
    
    /// Add alert route
    pub async fn add_alert_route(&self, route: AlertRoute) {
        let mut routes = self.alert_routes.write().await;
        routes.push(route);
    }
    
    /// Add suppression rule
    pub async fn add_suppression_rule(&self, rule: SuppressionRule) {
        let mut rules = self.suppression_rules.write().await;
        rules.push(rule);
    }
    
    /// Get active alerts
    pub async fn get_active_alerts(&self) -> HashMap<String, Alert> {
        self.active_alerts.read().await.clone()
    }
    
    /// Get alert history
    pub async fn get_alert_history(&self, limit: Option<usize>) -> Vec<AlertHistoryEntry> {
        let history = self.alert_history.read().await;
        if let Some(limit) = limit {
            history.iter().rev().take(limit).cloned().collect()
        } else {
            history.clone()
        }
    }
    
    /// Resolve an active alert
    pub async fn resolve_alert(&self, alert_id: &str) -> Result<(), String> {
        let mut active_alerts = self.active_alerts.write().await;
        
        if let Some(mut alert) = active_alerts.remove(alert_id) {
            alert.resolved = true;
            alert.resolved_at = Some(SystemTime::now());
            
            // Add to history
            let mut history = self.alert_history.write().await;
            history.push(AlertHistoryEntry {
                alert,
                action: AlertAction::Resolved,
                timestamp: SystemTime::now(),
            });
            
            Ok(())
        } else {
            Err(format!("Alert with ID {} not found", alert_id))
        }
    }
    
    /// Get alert statistics
    pub async fn get_alert_statistics(&self) -> AlertStatistics {
        use crate::alert_system::AlertStatistics;
        
        let active_alerts = self.active_alerts.read().await;
        let history = self.alert_history.read().await;
        
        let mut stats = AlertStatistics {
            total_active: active_alerts.len(),
            active_by_severity: HashMap::new(),
            total_resolved: 0,
            total_created_today: 0,
            most_frequent_metrics: HashMap::new(),
        };
        
        // Count active alerts by severity
        for alert in active_alerts.values() {
            *stats.active_by_severity.entry(alert.severity.clone()).or_insert(0) += 1;
        }
        
        // Analyze history
        let today = SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 86400;
        
        for entry in history.iter() {
            match entry.action {
                AlertAction::Resolved => stats.total_resolved += 1,
                AlertAction::Created => {
                    let entry_day = entry.timestamp.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 86400;
                    if entry_day == today {
                        stats.total_created_today += 1;
                    }
                    
                    *stats.most_frequent_metrics.entry(entry.alert.metric.clone()).or_insert(0) += 1;
                },
                _ => {},
            }
        }
        
        stats
    }
    
    /// Get metrics history
    pub async fn get_metrics_history(&self, limit: Option<usize>) -> Vec<MetricsSnapshot> {
        let history = self.metrics_history.read().await;
        match limit {
            Some(n) => {
                let start = if history.len() > n { history.len() - n } else { 0 };
                history[start..].to_vec()
            },
            None => history.clone(),
        }
    }
}

// System resource monitor for comprehensive resource tracking
pub struct SystemResourceMonitor {
    system: Arc<RwLock<System>>,
    resource_history: Arc<RwLock<Vec<SystemMetrics>>>,
    max_history_size: usize,
    monitoring_interval: Duration,
    alert_sender: broadcast::Sender<Alert>,
    thresholds: Arc<RwLock<ResourceThresholds>>,
    trend_analyzer: TrendAnalyzer,
}

/// Resource thresholds for alerting
#[derive(Debug, Clone)]
pub struct ResourceThresholds {
    pub cpu_warning: f64,
    pub cpu_critical: f64,
    pub memory_warning: f64,
    pub memory_critical: f64,
    pub disk_warning: f64,
    pub disk_critical: f64,
    pub network_warning: f64,
    pub network_critical: f64,
    pub file_descriptor_warning: u32,
    pub file_descriptor_critical: u32,
}

impl Default for ResourceThresholds {
    fn default() -> Self {
        Self {
            cpu_warning: 70.0,
            cpu_critical: 90.0,
            memory_warning: 80.0,
            memory_critical: 95.0,
            disk_warning: 85.0,
            disk_critical: 95.0,
            network_warning: 80.0,
            network_critical: 95.0,
            file_descriptor_warning: 8000,
            file_descriptor_critical: 9500,
        }
    }
}

/// Trend analysis for resource usage
pub struct TrendAnalyzer {
    window_size: usize,
}

impl TrendAnalyzer {
    pub fn new(window_size: usize) -> Self {
        Self { window_size }
    }
    
    /// Analyze trend and predict future values
    pub fn analyze_trend(&self, values: &[f64]) -> TrendAnalysis {
        if values.len() < 3 {
            return TrendAnalysis::default();
        }
        
        let recent_values = if values.len() > self.window_size {
            &values[values.len() - self.window_size..]
        } else {
            values
        };
        
        // Calculate linear regression
        let n = recent_values.len() as f64;
        let x_sum: f64 = (0..recent_values.len()).map(|i| i as f64).sum();
        let y_sum: f64 = recent_values.iter().sum();
        let xy_sum: f64 = recent_values.iter().enumerate()
            .map(|(i, &y)| i as f64 * y)
            .sum();
        let x2_sum: f64 = (0..recent_values.len()).map(|i| (i as f64).powi(2)).sum();
        
        let slope = (n * xy_sum - x_sum * y_sum) / (n * x2_sum - x_sum.powi(2));
        let intercept = (y_sum - slope * x_sum) / n;
        
        // Predict next few values
        let predictions: Vec<f64> = (recent_values.len()..recent_values.len() + 5)
            .map(|i| slope * i as f64 + intercept)
            .collect();
        
        // Calculate variance for confidence
        let mean = y_sum / n;
        let variance = recent_values.iter()
            .map(|&y| (y - mean).powi(2))
            .sum::<f64>() / n;
        
        TrendAnalysis {
            slope,
            predictions,
            confidence: 1.0 / (1.0 + variance), // Simple confidence metric
            is_increasing: slope > 0.1,
            is_decreasing: slope < -0.1,
            volatility: variance.sqrt(),
        }
    }
}

/// Trend analysis results
#[derive(Debug, Clone, Default)]
pub struct TrendAnalysis {
    pub slope: f64,
    pub predictions: Vec<f64>,
    pub confidence: f64,
    pub is_increasing: bool,
    pub is_decreasing: bool,
    pub volatility: f64,
}

/// System health assessment
#[derive(Debug, Clone)]
pub struct SystemHealthAssessment {
    pub overall_score: f64, // 0.0 to 1.0
    pub cpu_health: f64,
    pub memory_health: f64,
    pub network_health: f64,
    pub disk_health: f64,
    pub stability_score: f64,
    pub performance_score: f64,
    pub recommendations: Vec<String>,
    pub warnings: Vec<String>,
    pub critical_issues: Vec<String>,
}

impl SystemResourceMonitor {
    /// Create a new system resource monitor
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        
        let (alert_sender, _) = broadcast::channel(1000);
        
        Self {
            system: Arc::new(RwLock::new(system)),
            resource_history: Arc::new(RwLock::new(Vec::new())),
            max_history_size: 1000,
            monitoring_interval: Duration::from_secs(5),
            alert_sender,
            thresholds: Arc::new(RwLock::new(ResourceThresholds::default())),
            trend_analyzer: TrendAnalyzer::new(20),
        }
    }
    
    /// Start continuous resource monitoring
    pub fn start_monitoring(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let monitor = Arc::clone(&self);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(monitor.monitoring_interval);
            
            loop {
                interval.tick().await;
                
                if let Err(e) = monitor.collect_system_metrics().await {
                    eprintln!("Error collecting system metrics: {}", e);
                }
                
                monitor.check_resource_alerts().await;
            }
        })
    }
    
    /// Collect comprehensive system metrics
    pub async fn collect_system_metrics(&self) -> Result<SystemMetrics, Box<dyn std::error::Error + Send + Sync>> {
        let mut system = self.system.write().await;
        system.refresh_all();
        
        let current_pid = std::process::id();
        let process = system.process(Pid::from(current_pid as usize));
        
        let metrics = SystemMetrics {
            cpu_usage_percent: system.global_cpu_info().cpu_usage() as f64,
            memory_usage_bytes: system.used_memory(),
            memory_total_bytes: system.total_memory(),
            network_bytes_sent: 0, // Would be updated from network interfaces
            network_bytes_received: 0, // Would be updated from network interfaces
            open_file_descriptors: self.get_file_descriptor_count().await,
            thread_count: self.get_thread_count().await,
            disk_usage_bytes: self.get_disk_usage().await,
            load_average: self.get_load_average().await,
            uptime_seconds: sysinfo::System::uptime(),
        };
        
        // Store in history
        let mut history = self.resource_history.write().await;
        history.push(metrics.clone());
        if history.len() > self.max_history_size {
            history.remove(0);
        }
        
        Ok(metrics)
    }
    
    /// Get file descriptor count (platform-specific implementation needed)
    async fn get_file_descriptor_count(&self) -> u32 {
        // This would need platform-specific implementation
        // For now, return a placeholder value
        #[cfg(unix)]
        {
            // On Unix systems, we could read from /proc/self/fd
            if let Ok(entries) = std::fs::read_dir("/proc/self/fd") {
                return entries.count() as u32;
            }
        }
        
        0 // Placeholder
    }
    
    /// Get thread count (platform-specific implementation needed)
    async fn get_thread_count(&self) -> u32 {
        // This would need platform-specific implementation
        #[cfg(unix)]
        {
            // On Unix systems, we could read from /proc/self/status
            if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
                for line in status.lines() {
                    if line.starts_with("Threads:") {
                        if let Some(count_str) = line.split_whitespace().nth(1) {
                            if let Ok(count) = count_str.parse::<u32>() {
                                return count;
                            }
                        }
                    }
                }
            }
        }
        
        0 // Placeholder
    }
    
    /// Get disk usage
    async fn get_disk_usage(&self) -> u64 {
        let system = self.system.read().await;
        let mut total_used = 0;
        for disk in sysinfo::System::disks(&*system) {
            total_used += disk.total_space() - disk.available_space();
        }
        total_used
    }
    
    /// Get load average (Unix-specific)
    async fn get_load_average(&self) -> f64 {
        #[cfg(unix)]
        {
            let system = self.system.read().await;
            system.load_average().one
        }
        #[cfg(not(unix))]
        {
            0.0 // Not available on Windows
        }
    }
    
    /// Check for resource-based alerts
    async fn check_resource_alerts(&self) {
        if let Ok(metrics) = self.collect_system_metrics().await {
            let thresholds = self.thresholds.read().await;
            
            // Check CPU usage
            if metrics.cpu_usage_percent > thresholds.cpu_critical {
                self.send_alert(
                    "cpu_critical",
                    AlertSeverity::Critical,
                    "Critical CPU Usage",
                    &format!("CPU usage is {:.1}%, exceeding critical threshold of {:.1}%", 
                            metrics.cpu_usage_percent, thresholds.cpu_critical),
                    metrics.cpu_usage_percent,
                    thresholds.cpu_critical,
                ).await;
            } else if metrics.cpu_usage_percent > thresholds.cpu_warning {
                self.send_alert(
                    "cpu_warning",
                    AlertSeverity::Warning,
                    "High CPU Usage",
                    &format!("CPU usage is {:.1}%, exceeding warning threshold of {:.1}%", 
                            metrics.cpu_usage_percent, thresholds.cpu_warning),
                    metrics.cpu_usage_percent,
                    thresholds.cpu_warning,
                ).await;
            }
            
            // Check memory usage
            let memory_percent = (metrics.memory_usage_bytes as f64 / metrics.memory_total_bytes as f64) * 100.0;
            if memory_percent > thresholds.memory_critical {
                self.send_alert(
                    "memory_critical",
                    AlertSeverity::Critical,
                    "Critical Memory Usage",
                    &format!("Memory usage is {:.1}%, exceeding critical threshold of {:.1}%", 
                            memory_percent, thresholds.memory_critical),
                    memory_percent,
                    thresholds.memory_critical,
                ).await;
            } else if memory_percent > thresholds.memory_warning {
                self.send_alert(
                    "memory_warning",
                    AlertSeverity::Warning,
                    "High Memory Usage",
                    &format!("Memory usage is {:.1}%, exceeding warning threshold of {:.1}%", 
                            memory_percent, thresholds.memory_warning),
                    memory_percent,
                    thresholds.memory_warning,
                ).await;
            }
            
            // Check file descriptors
            if metrics.open_file_descriptors > thresholds.file_descriptor_critical {
                self.send_alert(
                    "fd_critical",
                    AlertSeverity::Critical,
                    "Critical File Descriptor Usage",
                    &format!("Open file descriptors: {}, exceeding critical threshold of {}", 
                            metrics.open_file_descriptors, thresholds.file_descriptor_critical),
                    metrics.open_file_descriptors as f64,
                    thresholds.file_descriptor_critical as f64,
                ).await;
            } else if metrics.open_file_descriptors > thresholds.file_descriptor_warning {
                self.send_alert(
                    "fd_warning",
                    AlertSeverity::Warning,
                    "High File Descriptor Usage",
                    &format!("Open file descriptors: {}, exceeding warning threshold of {}", 
                            metrics.open_file_descriptors, thresholds.file_descriptor_warning),
                    metrics.open_file_descriptors as f64,
                    thresholds.file_descriptor_warning as f64,
                ).await;
            }
        }
    }
    
    /// Send an alert
    async fn send_alert(&self, id: &str, severity: AlertSeverity, title: &str, description: &str, 
                       current_value: f64, threshold: f64) {
        let alert = Alert {
            id: id.to_string(),
            timestamp: SystemTime::now(),
            severity,
            title: title.to_string(),
            description: description.to_string(),
            metric: id.to_string(),
            current_value,
            threshold,
            layer: None,
            context: HashMap::new(),
            resolved: false,
            resolved_at: None,
        };
        
        let _ = self.alert_sender.send(alert);
    }
    
    /// Analyze resource usage trends
    pub async fn analyze_resource_trends(&self) -> HashMap<String, TrendAnalysis> {
        let history = self.resource_history.read().await;
        let mut trends = HashMap::new();
        
        if history.len() < 3 {
            return trends;
        }
        
        // CPU trend
        let cpu_values: Vec<f64> = history.iter().map(|m| m.cpu_usage_percent).collect();
        trends.insert("cpu_usage".to_string(), self.trend_analyzer.analyze_trend(&cpu_values));
        
        // Memory trend
        let memory_values: Vec<f64> = history.iter()
            .map(|m| (m.memory_usage_bytes as f64 / m.memory_total_bytes as f64) * 100.0)
            .collect();
        trends.insert("memory_usage".to_string(), self.trend_analyzer.analyze_trend(&memory_values));
        
        // File descriptor trend
        let fd_values: Vec<f64> = history.iter().map(|m| m.open_file_descriptors as f64).collect();
        trends.insert("file_descriptors".to_string(), self.trend_analyzer.analyze_trend(&fd_values));
        
        // Thread count trend
        let thread_values: Vec<f64> = history.iter().map(|m| m.thread_count as f64).collect();
        trends.insert("thread_count".to_string(), self.trend_analyzer.analyze_trend(&thread_values));
        
        trends
    }
    
    /// Predict future resource usage
    pub async fn predict_resource_usage(&self, minutes_ahead: u32) -> HashMap<String, Vec<f64>> {
        let trends = self.analyze_resource_trends().await;
        let mut predictions = HashMap::new();
        
        for (metric, trend) in trends {
            if trend.confidence > 0.5 {
                // Extend predictions to the requested time
                let extended_predictions: Vec<f64> = (0..minutes_ahead)
                    .map(|i| {
                        let base_prediction = trend.predictions.get(0).unwrap_or(&0.0);
                        base_prediction + (trend.slope * i as f64)
                    })
                    .collect();
                predictions.insert(metric, extended_predictions);
            }
        }
        
        predictions
    }
    
    /// Assess overall system health
    pub async fn assess_system_health(&self) -> SystemHealthAssessment {
        let metrics = match self.collect_system_metrics().await {
            Ok(m) => m,
            Err(_) => return SystemHealthAssessment {
                overall_score: 0.0,
                cpu_health: 0.0,
                memory_health: 0.0,
                network_health: 0.0,
                disk_health: 0.0,
                stability_score: 0.0,
                performance_score: 0.0,
                recommendations: vec!["Unable to collect system metrics".to_string()],
                warnings: vec!["System monitoring unavailable".to_string()],
                critical_issues: vec!["Cannot assess system health".to_string()],
            },
        };
        
        let thresholds = self.thresholds.read().await;
        let mut recommendations = Vec::new();
        let mut warnings = Vec::new();
        let mut critical_issues = Vec::new();
        
        // Assess CPU health
        let cpu_health = if metrics.cpu_usage_percent > thresholds.cpu_critical {
            critical_issues.push("CPU usage is critically high".to_string());
            recommendations.push("Investigate CPU-intensive processes".to_string());
            0.2
        } else if metrics.cpu_usage_percent > thresholds.cpu_warning {
            warnings.push("CPU usage is elevated".to_string());
            recommendations.push("Monitor CPU usage trends".to_string());
            0.6
        } else {
            1.0
        };
        
        // Assess memory health
        let memory_percent = (metrics.memory_usage_bytes as f64 / metrics.memory_total_bytes as f64) * 100.0;
        let memory_health = if memory_percent > thresholds.memory_critical {
            critical_issues.push("Memory usage is critically high".to_string());
            recommendations.push("Free up memory or add more RAM".to_string());
            0.2
        } else if memory_percent > thresholds.memory_warning {
            warnings.push("Memory usage is elevated".to_string());
            recommendations.push("Monitor memory usage patterns".to_string());
            0.6
        } else {
            1.0
        };
        
        // Assess network health (simplified)
        let network_health = 0.8; // Placeholder - would need actual network metrics
        
        // Assess disk health
        let disk_health = 0.9; // Placeholder - would need actual disk metrics
        
        // Calculate stability score based on trends
        let trends = self.analyze_resource_trends().await;
        let stability_score = trends.values()
            .map(|trend| if trend.volatility < 10.0 { 1.0 } else { 1.0 - (trend.volatility / 100.0) })
            .fold(0.0, |acc, score| acc + score) / trends.len() as f64;
        
        // Calculate performance score
        let performance_score = (cpu_health + memory_health + network_health + disk_health) / 4.0;
        
        // Calculate overall score
        let overall_score = (performance_score + stability_score) / 2.0;
        
        SystemHealthAssessment {
            overall_score,
            cpu_health,
            memory_health,
            network_health,
            disk_health,
            stability_score,
            performance_score,
            recommendations,
            warnings,
            critical_issues,
        }
    }
    
    /// Get resource usage history
    pub async fn get_resource_history(&self, limit: Option<usize>) -> Vec<SystemMetrics> {
        let history = self.resource_history.read().await;
        match limit {
            Some(n) => {
                let start = if history.len() > n { history.len() - n } else { 0 };
                history[start..].to_vec()
            },
            None => history.clone(),
        }
    }
    
    /// Subscribe to resource alerts
    pub fn subscribe_alerts(&self) -> broadcast::Receiver<Alert> {
        self.alert_sender.subscribe()
    }
    
    /// Update resource thresholds
    pub async fn update_thresholds(&self, thresholds: ResourceThresholds) {
        let mut current_thresholds = self.thresholds.write().await;
        *current_thresholds = thresholds;
    }
}

/// Error classification and analysis system
pub struct ErrorClassificationSystem {
    error_history: Arc<RwLock<Vec<ClassifiedError>>>,
    error_patterns: Arc<RwLock<HashMap<String, ErrorPattern>>>,
    root_cause_analyzer: RootCauseAnalyzer,
    max_history_size: usize,
    alert_sender: broadcast::Sender<Alert>,
    error_thresholds: Arc<RwLock<ErrorThresholds>>,
}

/// Classified error with additional context
#[derive(Debug, Clone)]
pub struct ClassifiedError {
    pub id: String,
    pub timestamp: SystemTime,
    pub layer: LayerType,
    pub error_type: ErrorType,
    pub severity: ErrorSeverity,
    pub message: String,
    pub context: HashMap<String, String>,
    pub stack_trace: Option<String>,
    pub correlation_id: Option<String>,
    pub user_impact: UserImpact,
    pub recovery_action: Option<String>,
    pub root_cause: Option<String>,
}

/// Error type classification
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ErrorType {
    // Network errors
    ConnectionTimeout,
    ConnectionRefused,
    NetworkUnreachable,
    PacketLoss,
    
    // Crypto errors
    KeyDerivationFailure,
    EncryptionFailure,
    DecryptionFailure,
    HandshakeFailure,
    
    // Stream errors
    StreamClosed,
    FlowControlViolation,
    BufferOverflow,
    FrameCorruption,
    
    // System errors
    OutOfMemory,
    FileSystemError,
    PermissionDenied,
    ResourceExhausted,
    
    // Protocol errors
    ProtocolViolation,
    InvalidMessage,
    SequenceError,
    VersionMismatch,
    
    // Configuration errors
    InvalidConfiguration,
    MissingConfiguration,
    ConfigurationConflict,
    
    // Unknown/Other
    Unknown(String),
}

/// User impact assessment
#[derive(Debug, Clone, PartialEq)]
pub enum UserImpact {
    None,
    Low,
    Medium,
    High,
    Critical,
}

/// Unified error pattern for pattern recognition and root cause analysis
#[derive(Debug, Clone)]
pub struct ErrorPattern {
    pub pattern_id: String,
    pub description: String,
    pub error_types: Vec<ErrorType>,
    pub frequency_threshold: u32,
    pub time_window: Duration,
    pub likely_causes: Vec<String>,
    pub recommended_actions: Vec<String>,
    // Additional fields for root cause analysis
    pub layers: Vec<LayerType>,
    pub severity_escalation: bool,
    pub root_causes: Vec<String>,
    pub mitigation_steps: Vec<String>,
    pub occurrences: u32,
    pub last_occurrence: SystemTime,
}

/// Error thresholds for alerting
#[derive(Debug, Clone)]
pub struct ErrorThresholds {
    pub error_rate_warning: f64,    // errors per minute
    pub error_rate_critical: f64,
    pub critical_error_threshold: u32,  // number of critical errors
    pub pattern_match_threshold: u32,   // pattern occurrences
    pub cascade_detection_window: Duration,
}

impl Default for ErrorThresholds {
    fn default() -> Self {
        Self {
            error_rate_warning: 10.0,
            error_rate_critical: 50.0,
            critical_error_threshold: 5,
            pattern_match_threshold: 3,
            cascade_detection_window: Duration::from_secs(300),
        }
    }
}

/// Root cause analyzer
pub struct RootCauseAnalyzer {
    correlation_rules: Vec<CorrelationRule>,
    dependency_graph: DependencyGraph,
}

/// Correlation rule for root cause analysis
#[derive(Debug, Clone)]
pub struct CorrelationRule {
    pub rule_id: String,
    pub trigger_errors: Vec<ErrorType>,
    pub time_window: Duration,
    pub confidence: f64,
    pub root_cause: String,
    pub explanation: String,
}

/// Dependency graph for understanding error propagation
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    pub dependencies: HashMap<LayerType, Vec<LayerType>>,
}

impl Default for DependencyGraph {
    fn default() -> Self {
        let mut dependencies = HashMap::new();
        
        // Define layer dependencies
        dependencies.insert(LayerType::Stream, vec![LayerType::Crypto, LayerType::Transport]);
        dependencies.insert(LayerType::Mix, vec![LayerType::Stream, LayerType::Crypto]);
        dependencies.insert(LayerType::Fec, vec![LayerType::Transport]);
        dependencies.insert(LayerType::Transport, vec![]);
        dependencies.insert(LayerType::Crypto, vec![]);
        
        Self { dependencies }
    }
}

impl ErrorClassificationSystem {
    /// Create a new error classification system
    pub fn new() -> Self {
        let (alert_sender, _) = broadcast::channel(1000);
        
        let mut error_patterns = HashMap::new();
        
        // Define common error patterns
        error_patterns.insert("connection_cascade".to_string(), ErrorPattern {
            pattern_id: "connection_cascade".to_string(),
            description: "Multiple connection failures in short time".to_string(),
            error_types: vec![ErrorType::ConnectionTimeout, ErrorType::ConnectionRefused],
            frequency_threshold: 5,
            time_window: Duration::from_secs(120),
            likely_causes: vec![
                "Network connectivity issues".to_string(),
                "Server overload".to_string(),
                "Firewall blocking connections".to_string(),
            ],
            recommended_actions: vec![
                "Check network connectivity".to_string(),
                "Verify server status".to_string(),
                "Review firewall rules".to_string(),
            ],
            layers: vec![LayerType::Transport],
            severity_escalation: true,
            root_causes: vec![
                "Network connectivity issues".to_string(),
                "Server overload".to_string(),
                "Firewall blocking connections".to_string(),
            ],
            mitigation_steps: vec![
                "Check network connectivity".to_string(),
                "Verify server status".to_string(),
                "Review firewall rules".to_string(),
            ],
            occurrences: 0,
            last_occurrence: SystemTime::UNIX_EPOCH,
        });
        
        error_patterns.insert("crypto_failure_cascade".to_string(), ErrorPattern {
            pattern_id: "crypto_failure_cascade".to_string(),
            description: "Cryptographic operation failures".to_string(),
            error_types: vec![ErrorType::KeyDerivationFailure, ErrorType::HandshakeFailure],
            frequency_threshold: 3,
            time_window: Duration::from_secs(60),
            likely_causes: vec![
                "Key synchronization issues".to_string(),
                "Clock skew".to_string(),
                "Corrupted key material".to_string(),
            ],
            recommended_actions: vec![
                "Verify time synchronization".to_string(),
                "Check key rotation status".to_string(),
                "Restart crypto layer".to_string(),
            ],
            layers: vec![LayerType::Crypto],
            severity_escalation: true,
            root_causes: vec![
                "Key synchronization issues".to_string(),
                "Clock skew".to_string(),
                "Corrupted key material".to_string(),
            ],
            mitigation_steps: vec![
                "Verify time synchronization".to_string(),
                "Check key rotation status".to_string(),
                "Restart crypto layer".to_string(),
            ],
            occurrences: 0,
            last_occurrence: SystemTime::UNIX_EPOCH,
        });
        
        // Initialize correlation rules
        let correlation_rules = vec![
            CorrelationRule {
                rule_id: "memory_cascade".to_string(),
                trigger_errors: vec![ErrorType::OutOfMemory, ErrorType::BufferOverflow],
                time_window: Duration::from_secs(300),
                confidence: 0.9,
                root_cause: "Memory pressure".to_string(),
                explanation: "Memory exhaustion causing cascading failures".to_string(),
            },
            CorrelationRule {
                rule_id: "network_partition".to_string(),
                trigger_errors: vec![ErrorType::NetworkUnreachable, ErrorType::ConnectionTimeout],
                time_window: Duration::from_secs(180),
                confidence: 0.8,
                root_cause: "Network partition".to_string(),
                explanation: "Network connectivity issues affecting multiple connections".to_string(),
            },
        ];
        
        Self {
            error_history: Arc::new(RwLock::new(Vec::new())),
            error_patterns: Arc::new(RwLock::new(error_patterns)),
            root_cause_analyzer: RootCauseAnalyzer {
                correlation_rules,
                dependency_graph: DependencyGraph::default(),
            },
            max_history_size: 10000,
            alert_sender,
            error_thresholds: Arc::new(RwLock::new(ErrorThresholds::default())),
        }
    }
    
    /// Classify and record an error
    pub async fn classify_error(
        &self,
        layer: LayerType,
        error_message: String,
        context: HashMap<String, String>,
        stack_trace: Option<String>,
    ) -> ClassifiedError {
        let error_type = self.classify_error_type(&error_message, &context);
        let severity = self.assess_severity(&error_type, &context);
        let user_impact = self.assess_user_impact(&error_type, &severity);
        
        let classified_error = ClassifiedError {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            layer,
            error_type: error_type.clone(),
            severity: severity.clone(),
            message: error_message,
            context: context.clone(),
            stack_trace,
            correlation_id: context.get("correlation_id").cloned(),
            user_impact,
            recovery_action: self.suggest_recovery_action(&error_type),
            root_cause: None, // Will be filled by root cause analysis
        };
        
        // Store in history
        let mut history = self.error_history.write().await;
        history.push(classified_error.clone());
        if history.len() > self.max_history_size {
            history.remove(0);
        }
        
        // Perform pattern analysis
        self.analyze_error_patterns().await;
        
        // Perform root cause analysis
        let root_cause = self.analyze_root_cause(&classified_error).await;
        
        // Update the error with root cause if found
        let mut updated_error = classified_error;
        updated_error.root_cause = root_cause;
        
        // Check for alerts
        self.check_error_alerts(&updated_error).await;
        
        updated_error
    }
    
    /// Classify error type based on message and context
    fn classify_error_type(&self, message: &str, context: &HashMap<String, String>) -> ErrorType {
        let message_lower = message.to_lowercase();
        
        // Network errors
        if message_lower.contains("timeout") || message_lower.contains("timed out") {
            return ErrorType::ConnectionTimeout;
        }
        if message_lower.contains("connection refused") || message_lower.contains("refused") {
            return ErrorType::ConnectionRefused;
        }
        if message_lower.contains("unreachable") || message_lower.contains("no route") {
            return ErrorType::NetworkUnreachable;
        }
        if message_lower.contains("packet loss") || message_lower.contains("lost packets") {
            return ErrorType::PacketLoss;
        }
        
        // Crypto errors
        if message_lower.contains("key derivation") || message_lower.contains("hpke") {
            return ErrorType::KeyDerivationFailure;
        }
        if message_lower.contains("encryption failed") || message_lower.contains("encrypt") {
            return ErrorType::EncryptionFailure;
        }
        if message_lower.contains("decryption failed") || message_lower.contains("decrypt") {
            return ErrorType::DecryptionFailure;
        }
        if message_lower.contains("handshake") || message_lower.contains("noise") {
            return ErrorType::HandshakeFailure;
        }
        
        // Stream errors
        if message_lower.contains("stream closed") || message_lower.contains("connection closed") {
            return ErrorType::StreamClosed;
        }
        if message_lower.contains("flow control") || message_lower.contains("backpressure") {
            return ErrorType::FlowControlViolation;
        }
        if message_lower.contains("buffer overflow") || message_lower.contains("buffer full") {
            return ErrorType::BufferOverflow;
        }
        if message_lower.contains("frame") && message_lower.contains("corrupt") {
            return ErrorType::FrameCorruption;
        }
        
        // System errors
        if message_lower.contains("out of memory") || message_lower.contains("oom") {
            return ErrorType::OutOfMemory;
        }
        if message_lower.contains("permission denied") || message_lower.contains("access denied") {
            return ErrorType::PermissionDenied;
        }
        if message_lower.contains("resource") && message_lower.contains("exhausted") {
            return ErrorType::ResourceExhausted;
        }
        
        // Protocol errors
        if message_lower.contains("protocol") && message_lower.contains("violation") {
            return ErrorType::ProtocolViolation;
        }
        if message_lower.contains("invalid message") || message_lower.contains("malformed") {
            return ErrorType::InvalidMessage;
        }
        if message_lower.contains("sequence") || message_lower.contains("order") {
            return ErrorType::SequenceError;
        }
        if message_lower.contains("version") && message_lower.contains("mismatch") {
            return ErrorType::VersionMismatch;
        }
        
        // Configuration errors
        if message_lower.contains("configuration") || message_lower.contains("config") {
            if message_lower.contains("invalid") {
                return ErrorType::InvalidConfiguration;
            }
            if message_lower.contains("missing") {
                return ErrorType::MissingConfiguration;
            }
            if message_lower.contains("conflict") {
                return ErrorType::ConfigurationConflict;
            }
        }
        
        // Check context for additional clues
        if let Some(error_code) = context.get("error_code") {
            match error_code.as_str() {
                "ETIMEDOUT" => return ErrorType::ConnectionTimeout,
                "ECONNREFUSED" => return ErrorType::ConnectionRefused,
                "ENETUNREACH" => return ErrorType::NetworkUnreachable,
                "ENOMEM" => return ErrorType::OutOfMemory,
                "EACCES" => return ErrorType::PermissionDenied,
                _ => {}
            }
        }
        
        ErrorType::Unknown(message.to_string())
    }
    
    /// Assess error severity
    fn assess_severity(&self, error_type: &ErrorType, context: &HashMap<String, String>) -> ErrorSeverity {
        match error_type {
            // Critical errors that can cause system failure
            ErrorType::OutOfMemory | 
            ErrorType::ResourceExhausted => ErrorSeverity::Critical,
            
            // High severity errors affecting functionality
            ErrorType::KeyDerivationFailure |
            ErrorType::HandshakeFailure |
            ErrorType::ProtocolViolation => ErrorSeverity::Error,
            
            // Medium severity errors with potential workarounds
            ErrorType::ConnectionTimeout |
            ErrorType::ConnectionRefused |
            ErrorType::StreamClosed => ErrorSeverity::Warning,
            
            // Low severity errors that are recoverable
            ErrorType::PacketLoss |
            ErrorType::FlowControlViolation => ErrorSeverity::Info,
            
            // Configuration errors vary by context
            ErrorType::InvalidConfiguration |
            ErrorType::MissingConfiguration => ErrorSeverity::Error,
            ErrorType::ConfigurationConflict => ErrorSeverity::Warning,
            
            // Unknown errors default to warning
            ErrorType::Unknown(_) => ErrorSeverity::Warning,
            
            _ => ErrorSeverity::Info,
        }
    }
    
    /// Assess user impact
    fn assess_user_impact(&self, error_type: &ErrorType, severity: &ErrorSeverity) -> UserImpact {
        match (error_type, severity) {
            (ErrorType::OutOfMemory, _) |
            (ErrorType::ResourceExhausted, _) => UserImpact::Critical,
            
            (ErrorType::KeyDerivationFailure, _) |
            (ErrorType::HandshakeFailure, _) => UserImpact::High,
            
            (ErrorType::ConnectionTimeout, ErrorSeverity::Error) |
            (ErrorType::ConnectionRefused, ErrorSeverity::Error) => UserImpact::Medium,
            
            (ErrorType::PacketLoss, _) |
            (ErrorType::FlowControlViolation, _) => UserImpact::Low,
            
            (_, ErrorSeverity::Critical) => UserImpact::Critical,
            (_, ErrorSeverity::Error) => UserImpact::High,
            (_, ErrorSeverity::Warning) => UserImpact::Medium,
            (_, ErrorSeverity::Info) => UserImpact::Low,
            
            _ => UserImpact::None,
        }
    }
    
    /// Suggest recovery action
    fn suggest_recovery_action(&self, error_type: &ErrorType) -> Option<String> {
        match error_type {
            ErrorType::ConnectionTimeout => Some("Retry connection with exponential backoff".to_string()),
            ErrorType::ConnectionRefused => Some("Check service availability and retry".to_string()),
            ErrorType::NetworkUnreachable => Some("Verify network connectivity".to_string()),
            ErrorType::KeyDerivationFailure => Some("Regenerate keys and retry handshake".to_string()),
            ErrorType::HandshakeFailure => Some("Reset connection and retry handshake".to_string()),
            ErrorType::StreamClosed => Some("Reestablish stream connection".to_string()),
            ErrorType::BufferOverflow => Some("Increase buffer size or implement backpressure".to_string()),
            ErrorType::OutOfMemory => Some("Free memory and restart affected components".to_string()),
            ErrorType::InvalidConfiguration => Some("Validate and correct configuration".to_string()),
            _ => None,
        }
    }
    
    /// Analyze error patterns
    async fn analyze_error_patterns(&self) {
        let history = self.error_history.read().await;
        let patterns = self.error_patterns.read().await;
        
        let now = SystemTime::now();
        
        for pattern in patterns.values() {
            let recent_errors: Vec<&ClassifiedError> = history.iter()
                .filter(|e| {
                    now.duration_since(e.timestamp).unwrap_or_default() <= pattern.time_window &&
                    pattern.error_types.contains(&e.error_type)
                })
                .collect();
            
            if recent_errors.len() >= pattern.frequency_threshold as usize {
                // Pattern detected - send alert
                let alert = Alert {
                    id: format!("pattern_{}", pattern.pattern_id),
                    timestamp: now,
                    severity: AlertSeverity::Warning,
                    title: format!("Error Pattern Detected: {}", pattern.description),
                    description: format!(
                        "Detected {} occurrences of pattern '{}' in the last {:?}. Likely causes: {}",
                        recent_errors.len(),
                        pattern.description,
                        pattern.time_window,
                        pattern.likely_causes.join(", ")
                    ),
                    metric: "error_pattern".to_string(),
                    current_value: recent_errors.len() as f64,
                    threshold: pattern.frequency_threshold as f64,
                    layer: None,
                    context: {
                        let mut context = HashMap::new();
                        context.insert("pattern_id".to_string(), pattern.pattern_id.clone());
                        context.insert("recommended_actions".to_string(), pattern.recommended_actions.join("; "));
                        context
                    },
                    resolved: false,
                    resolved_at: None,
                };
                
                let _ = self.alert_sender.send(alert);
            }
        }
    }
    
    /// Analyze root cause
    async fn analyze_root_cause(&self, error: &ClassifiedError) -> Option<String> {
        let history = self.error_history.read().await;
        
        for rule in &self.root_cause_analyzer.correlation_rules {
            if rule.trigger_errors.contains(&error.error_type) {
                // Look for correlated errors in the time window
                let window_start = error.timestamp - rule.time_window;
                let correlated_errors: Vec<&ClassifiedError> = history.iter()
                    .filter(|e| {
                        e.timestamp >= window_start &&
                        e.timestamp <= error.timestamp &&
                        rule.trigger_errors.contains(&e.error_type)
                    })
                    .collect();
                
                if correlated_errors.len() >= 2 {
                    return Some(format!("{}: {}", rule.root_cause, rule.explanation));
                }
            }
        }
        
        // Check for dependency-based root causes
        if let Some(dependencies) = self.root_cause_analyzer.dependency_graph.dependencies.get(&error.layer) {
            for dep_layer in dependencies {
                let dep_errors: Vec<&ClassifiedError> = history.iter()
                    .filter(|e| {
                        e.layer == *dep_layer &&
                        error.timestamp.duration_since(e.timestamp).unwrap_or_default() <= Duration::from_secs(300)
                    })
                    .collect();
                
                if !dep_errors.is_empty() {
                    return Some(format!("Dependency failure in {:?} layer", dep_layer));
                }
            }
        }
        
        None
    }
    
    /// Check for error-based alerts
    async fn check_error_alerts(&self, error: &ClassifiedError) {
        let thresholds = self.error_thresholds.read().await;
        
        // Check for critical error threshold
        if error.severity == ErrorSeverity::Critical {
            let history = self.error_history.read().await;
            let recent_critical = history.iter()
                .filter(|e| {
                    e.severity == ErrorSeverity::Critical &&
                    SystemTime::now().duration_since(e.timestamp).unwrap_or_default() <= Duration::from_secs(600)
                })
                .count();
            
            if recent_critical >= thresholds.critical_error_threshold as usize {
                let alert = Alert {
                    id: "critical_error_threshold".to_string(),
                    timestamp: SystemTime::now(),
                    severity: AlertSeverity::Critical,
                    title: "Critical Error Threshold Exceeded".to_string(),
                    description: format!(
                        "Detected {} critical errors in the last 10 minutes, exceeding threshold of {}",
                        recent_critical, thresholds.critical_error_threshold
                    ),
                    metric: "critical_errors".to_string(),
                    current_value: recent_critical as f64,
                    threshold: thresholds.critical_error_threshold as f64,
                    layer: Some(error.layer.clone()),
                    context: HashMap::new(),
                    resolved: false,
                    resolved_at: None,
                };
                
                let _ = self.alert_sender.send(alert);
            }
        }
        
        // Check error rate
        let history = self.error_history.read().await;
        let recent_errors = history.iter()
            .filter(|e| {
                SystemTime::now().duration_since(e.timestamp).unwrap_or_default() <= Duration::from_secs(60)
            })
            .count();
        
        let error_rate = recent_errors as f64;
        
        if error_rate > thresholds.error_rate_critical {
            let alert = Alert {
                id: "error_rate_critical".to_string(),
                timestamp: SystemTime::now(),
                severity: AlertSeverity::Critical,
                title: "Critical Error Rate".to_string(),
                description: format!(
                    "Error rate is {:.1} errors/minute, exceeding critical threshold of {:.1}",
                    error_rate, thresholds.error_rate_critical
                ),
                metric: "error_rate".to_string(),
                current_value: error_rate,
                threshold: thresholds.error_rate_critical,
                layer: Some(error.layer.clone()),
                context: HashMap::new(),
                resolved: false,
                resolved_at: None,
            };
            
            let _ = self.alert_sender.send(alert);
        } else if error_rate > thresholds.error_rate_warning {
            let alert = Alert {
                id: "error_rate_warning".to_string(),
                timestamp: SystemTime::now(),
                severity: AlertSeverity::Warning,
                title: "High Error Rate".to_string(),
                description: format!(
                    "Error rate is {:.1} errors/minute, exceeding warning threshold of {:.1}",
                    error_rate, thresholds.error_rate_warning
                ),
                metric: "error_rate".to_string(),
                current_value: error_rate,
                threshold: thresholds.error_rate_warning,
                layer: Some(error.layer.clone()),
                context: HashMap::new(),
                resolved: false,
                resolved_at: None,
            };
            
            let _ = self.alert_sender.send(alert);
        }
    }
    
    /// Get error statistics
    pub async fn get_error_statistics(&self) -> ErrorStatistics {
        let history = self.error_history.read().await;
        let now = SystemTime::now();
        
        let mut stats = ErrorStatistics::default();
        
        for error in history.iter() {
            stats.total_errors += 1;
            
            // Count by layer
            *stats.errors_by_layer.entry(error.layer.clone()).or_insert(0) += 1;
            
            // Count by type
            *stats.errors_by_type.entry(error.error_type.clone()).or_insert(0) += 1;
            
            // Count by severity
            match error.severity {
                ErrorSeverity::Critical => stats.critical_errors += 1,
                ErrorSeverity::Error => stats.error_count += 1,
                ErrorSeverity::Warning => stats.warning_count += 1,
                ErrorSeverity::Info => stats.info_count += 1,
                ErrorSeverity::Debug => stats.debug_count += 1,
            }
            
            // Recent errors (last hour)
            if now.duration_since(error.timestamp).unwrap_or_default() <= Duration::from_secs(3600) {
                stats.recent_errors += 1;
            }
        }
        
        // Calculate error rate (errors per minute in last hour)
        stats.error_rate = stats.recent_errors as f64 / 60.0;
        
        stats
    }
    
    /// Get error trends
    pub async fn get_error_trends(&self, time_window: Duration) -> HashMap<ErrorType, Vec<u32>> {
        let history = self.error_history.read().await;
        let now = SystemTime::now();
        let window_start = now - time_window;
        
        let mut trends = HashMap::new();
        
        // Create time buckets (e.g., hourly buckets)
        let bucket_duration = Duration::from_secs(3600);
        let bucket_count = (time_window.as_secs() / bucket_duration.as_secs()) as usize;
        
        for error in history.iter() {
            if error.timestamp >= window_start {
                let bucket_index = ((now.duration_since(error.timestamp).unwrap_or_default().as_secs() / bucket_duration.as_secs()) as usize).min(bucket_count - 1);
                
                let trend = trends.entry(error.error_type.clone()).or_insert_with(|| vec![0; bucket_count]);
                trend[bucket_count - 1 - bucket_index] += 1;
            }
        }
        
        trends
    }
    
    /// Subscribe to error alerts
    pub fn subscribe_alerts(&self) -> broadcast::Receiver<Alert> {
        self.alert_sender.subscribe()
    }
    
    /// Get error history
    pub async fn get_error_history(&self, limit: Option<usize>) -> Vec<ClassifiedError> {
        let history = self.error_history.read().await;
        match limit {
            Some(n) => {
                let start = if history.len() > n { history.len() - n } else { 0 };
                history[start..].to_vec()
            },
            None => history.clone(),
        }
    }
    
    /// Add custom error pattern
    pub async fn add_error_pattern(&self, pattern: ErrorPattern) {
        let mut patterns = self.error_patterns.write().await;
        patterns.insert(pattern.pattern_id.clone(), pattern);
    }
    
    /// Update error thresholds
    pub async fn update_thresholds(&self, thresholds: ErrorThresholds) {
        let mut current_thresholds = self.error_thresholds.write().await;
        *current_thresholds = thresholds;
    }
}

/// Error statistics summary
#[derive(Debug, Clone, Default)]
pub struct ErrorStatistics {
    pub total_errors: u64,
    pub recent_errors: u64,
    pub error_rate: f64,
    pub critical_errors: u64,
    pub error_count: u64,
    pub warning_count: u64,
    pub info_count: u64,
    pub debug_count: u64,
    pub errors_by_layer: HashMap<LayerType, u64>,
    pub errors_by_type: HashMap<ErrorType, u64>,
}

/// Comprehensive alert system with priority routing and suppression
// Error classification and analysis system
pub struct ErrorAnalysisSystem {
    error_history: Arc<RwLock<Vec<ErrorInfo>>>,
    error_patterns: Arc<RwLock<HashMap<String, ErrorPattern>>>,
    classification_rules: Arc<RwLock<Vec<ClassificationRule>>>,
    alert_sender: broadcast::Sender<Alert>,
    max_history_size: usize,
    analysis_window: Duration,
}



/// Classification rule for automatic error categorization
#[derive(Debug, Clone)]
pub struct ClassificationRule {
    pub rule_id: String,
    pub name: String,
    pub conditions: Vec<ErrorCondition>,
    pub action: ClassificationAction,
    pub priority: u32,
}

/// Error condition for classification rules
#[derive(Debug, Clone)]
pub struct ErrorCondition {
    pub field: ErrorField,
    pub operator: ComparisonOperator,
    pub value: String,
}

/// Fields that can be used in error conditions
#[derive(Debug, Clone)]
pub enum ErrorField {
    ErrorType,
    Message,
    Layer,
    Severity,
    Context(String),
}

/// Comparison operators for error conditions
#[derive(Debug, Clone)]
pub enum ComparisonOperator {
    Equals,
    Contains,
    StartsWith,
    EndsWith,
    Regex,
}

/// Actions to take when classification rules match
#[derive(Debug, Clone)]
pub enum ClassificationAction {
    SetCategory(String),
    SetSeverity(ErrorSeverity),
    AddTag(String),
    TriggerAlert(AlertSeverity),
    ExecuteScript(String),
}

/// Error analysis results
#[derive(Debug, Clone)]
pub struct ErrorAnalysisResult {
    pub total_errors: u64,
    pub error_rate: f64,
    pub error_distribution: HashMap<LayerType, u64>,
    pub severity_distribution: HashMap<ErrorSeverity, u64>,
    pub top_error_types: Vec<(String, u64)>,
    pub detected_patterns: Vec<ErrorPattern>,
    pub root_cause_analysis: Vec<RootCauseAnalysis>,
    pub trend_analysis: ErrorTrendAnalysis,
    pub recommendations: Vec<String>,
    pub preventive_measures: Vec<String>,
}

/// Root cause analysis for specific error patterns
#[derive(Debug, Clone)]
pub struct RootCauseAnalysis {
    pub pattern_id: String,
    pub confidence: f64,
    pub contributing_factors: Vec<String>,
    pub timeline: Vec<ErrorEvent>,
    pub impact_assessment: ImpactAssessment,
    pub resolution_steps: Vec<String>,
}

/// Error event for timeline analysis
#[derive(Debug, Clone)]
pub struct ErrorEvent {
    pub timestamp: SystemTime,
    pub event_type: String,
    pub description: String,
    pub context: HashMap<String, String>,
}

/// Impact assessment for error patterns
#[derive(Debug, Clone)]
pub struct ImpactAssessment {
    pub affected_layers: Vec<LayerType>,
    pub performance_impact: f64, // 0.0 to 1.0
    pub availability_impact: f64, // 0.0 to 1.0
    pub user_impact: f64, // 0.0 to 1.0
    pub business_impact: String,
}

/// Error trend analysis
#[derive(Debug, Clone)]
pub struct ErrorTrendAnalysis {
    pub error_rate_trend: TrendAnalysis,
    pub severity_trends: HashMap<ErrorSeverity, TrendAnalysis>,
    pub layer_trends: HashMap<LayerType, TrendAnalysis>,
}

/// Prometheus metrics exporter for comprehensive monitoring integration
pub struct PrometheusExporter {
    /// HTTP server for metrics endpoint
    server_handle: Option<tokio::task::JoinHandle<()>>,
    
    /// Metrics collection interval
    collection_interval: Duration,
    
    /// Reference to comprehensive metrics collector
    metrics_collector: Arc<ComprehensiveMetrics>,
    
    /// Server address for metrics endpoint
    server_addr: std::net::SocketAddr,
    
    /// Metrics labels for consistent labeling
    default_labels: HashMap<String, String>,
}

impl PrometheusExporter {
    /// Create a new Prometheus exporter
    pub fn new(
        metrics_collector: Arc<ComprehensiveMetrics>,
        server_addr: std::net::SocketAddr,
    ) -> Self {
        let mut default_labels = HashMap::new();
        default_labels.insert("service".to_string(), "nyx-daemon".to_string());
        default_labels.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());
        
        Self {
            server_handle: None,
            collection_interval: Duration::from_secs(15),
            metrics_collector,
            server_addr,
            default_labels,
        }
    }
    
    /// Start the Prometheus metrics server with enhanced features
    pub async fn start_server(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use axum::{
            routing::get,
            Router,
            response::{Response, IntoResponse},
            http::{StatusCode, HeaderMap, header},
            extract::Query,
        };
        use std::sync::Arc;
        use std::collections::HashMap;
        
        let metrics_collector = Arc::clone(&self.metrics_collector);
        let default_labels = self.default_labels.clone();
        
        // Enhanced metrics endpoint with caching and compression
        let metrics_handler = move |Query(params): Query<HashMap<String, String>>| {
            let collector = Arc::clone(&metrics_collector);
            let labels = default_labels.clone();
            async move {
                let start_time = std::time::Instant::now();
                
                match Self::export_metrics_with_options(collector, labels, params).await {
                    Ok(metrics_data) => {
                        let mut headers = HeaderMap::new();
                        headers.insert(
                            header::CONTENT_TYPE,
                            "text/plain; version=0.0.4; charset=utf-8".parse().unwrap(),
                        );
                        headers.insert(
                            "X-Metrics-Generation-Time",
                            format!("{}ms", start_time.elapsed().as_millis()).parse().unwrap(),
                        );
                        
                        (StatusCode::OK, headers, metrics_data).into_response()
                    }
                    Err(e) => {
                        error!("Error exporting metrics: {}", e);
                        let error_response = format!(
                            "# Error exporting metrics: {}\n# Timestamp: {}\n",
                            e,
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs()
                        );
                        (StatusCode::INTERNAL_SERVER_ERROR, error_response).into_response()
                    }
                }
            }
        };
        
        // Enhanced health check endpoint
        let health_handler = move || {
            let health_collector = Arc::clone(&metrics_collector);
            async move {
                // Perform basic health checks
                let health_status = Self::check_system_health(health_collector).await;
                
                match health_status {
                    Ok(status) => {
                        let mut headers = HeaderMap::new();
                        headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
                        
                        let response = serde_json::json!({
                            "status": "healthy",
                            "timestamp": std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs(),
                            "checks": status
                        });
                        
                        (StatusCode::OK, headers, response.to_string()).into_response()
                    }
                    Err(e) => {
                        let response = serde_json::json!({
                            "status": "unhealthy",
                            "error": e.to_string(),
                            "timestamp": std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs()
                        });
                        
                        (StatusCode::SERVICE_UNAVAILABLE, response.to_string()).into_response()
                    }
                }
            }
        };
        
        // Create the application with enhanced routing
        let app = Router::new()
            .route("/metrics", get(metrics_handler))
            .route("/health", get(health_handler))
            .route("/ready", get(|| async { 
                (StatusCode::OK, "Ready").into_response()
            }))
            .route("/", get(|| async {
                let html = r#"
                <html>
                <head><title>Nyx Metrics Server</title></head>
                <body>
                    <h1>Nyx Metrics Server</h1>
                    <ul>
                        <li><a href="/metrics">Prometheus Metrics</a></li>
                        <li><a href="/health">Health Check</a></li>
                        <li><a href="/ready">Readiness Check</a></li>
                    </ul>
                </body>
                </html>
                "#;
                (StatusCode::OK, [("content-type", "text/html")], html).into_response()
            }));
        
        // Start the server with enhanced error handling
        let listener = tokio::net::TcpListener::bind(self.server_addr).await
            .map_err(|e| format!("Failed to bind to {}: {}", self.server_addr, e))?;
        
        let actual_addr = listener.local_addr()
            .map_err(|e| format!("Failed to get local address: {}", e))?;
        
        let server_handle = tokio::spawn(async move {
            info!("Prometheus metrics server starting on http://{}", actual_addr);
            info!("Metrics endpoint: http://{}/metrics", actual_addr);
            info!("Health endpoint: http://{}/health", actual_addr);
            
            if let Err(e) = axum::serve(listener, app).await {
                error!("Prometheus server error: {}", e);
            } else {
                info!("Prometheus server stopped gracefully");
            }
        });
        
        self.server_handle = Some(server_handle);
        
        // Start periodic metrics collection
        self.start_metrics_collection().await;
        
        info!("Prometheus exporter started successfully on {}", actual_addr);
        Ok(())
    }
    
    /// Start periodic metrics collection and registration
    async fn start_metrics_collection(&self) {
        let metrics_collector = Arc::clone(&self.metrics_collector);
        let interval = self.collection_interval;
        
        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            
            loop {
                interval_timer.tick().await;
                
                // Collect current metrics snapshot
                let _snapshot = metrics_collector.collect_all().await;
                
                // Metrics are exported on-demand via HTTP endpoint
                // This just ensures regular collection happens
            }
        });
    }
    
    /// Export metrics in Prometheus format with enhanced options
    async fn export_metrics_with_options(
        metrics_collector: Arc<ComprehensiveMetrics>,
        default_labels: HashMap<String, String>,
        params: HashMap<String, String>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let snapshot = metrics_collector.collect_all().await;
        let mut output = String::new();
        
        // Check for format parameter (future extensibility)
        let _format = params.get("format").unwrap_or(&"prometheus".to_string());
        
        // Add metadata with enhanced information
        output.push_str("# HELP nyx_info Information about the Nyx daemon\n");
        output.push_str("# TYPE nyx_info gauge\n");
        output.push_str(&format!(
            "nyx_info{{version=\"{}\",build_time=\"{}\",git_commit=\"{}\"}} 1\n", 
            env!("CARGO_PKG_VERSION"),
            option_env!("BUILD_TIME").unwrap_or("unknown"),
            option_env!("GIT_COMMIT").unwrap_or("unknown")
        ));
        
        // Add collection timestamp
        output.push_str("# HELP nyx_metrics_collection_timestamp_seconds Timestamp when metrics were collected\n");
        output.push_str("# TYPE nyx_metrics_collection_timestamp_seconds gauge\n");
        output.push_str(&format!(
            "nyx_metrics_collection_timestamp_seconds{{}} {}\n",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        ));
        output.push_str("\n");
        
        // Export metrics based on requested categories
        let include_system = params.get("include").map_or(true, |v| v.contains("system"));
        let include_network = params.get("include").map_or(true, |v| v.contains("network"));
        let include_layers = params.get("include").map_or(true, |v| v.contains("layers"));
        let include_performance = params.get("include").map_or(true, |v| v.contains("performance"));
        let include_errors = params.get("include").map_or(true, |v| v.contains("errors"));
        let include_alerts = params.get("include").map_or(true, |v| v.contains("alerts"));
        
        if include_system {
            Self::export_system_metrics(&mut output, &snapshot.system_metrics, &default_labels)?;
        }
        
        if include_network {
            Self::export_network_metrics(&mut output, &snapshot.network_metrics, &default_labels)?;
        }
        
        if include_layers {
            Self::export_layer_metrics(&mut output, &snapshot.layer_metrics, &default_labels)?;
        }
        
        if include_performance {
            Self::export_performance_metrics(&mut output, &snapshot.performance_metrics, &default_labels)?;
        }
        
        if include_errors {
            Self::export_error_metrics(&mut output, &snapshot.error_metrics, &default_labels)?;
        }
        
        if include_alerts {
            let active_alerts = metrics_collector.get_active_alerts().await;
            Self::export_alert_metrics(&mut output, &active_alerts, &default_labels)?;
        }
        
        Ok(output)
    }
    
    /// Export metrics in Prometheus format (legacy method for backward compatibility)
    async fn export_metrics(
        metrics_collector: Arc<ComprehensiveMetrics>,
        default_labels: HashMap<String, String>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Self::export_metrics_with_options(metrics_collector, default_labels, HashMap::new()).await
    }
    
    /// Perform system health checks
    async fn check_system_health(
        metrics_collector: Arc<ComprehensiveMetrics>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let snapshot = metrics_collector.collect_all().await;
        let mut checks = serde_json::Map::new();
        
        // Check system resources
        let cpu_healthy = snapshot.system_metrics.cpu_usage_percent < 90.0;
        checks.insert("cpu".to_string(), serde_json::json!({
            "healthy": cpu_healthy,
            "usage_percent": snapshot.system_metrics.cpu_usage_percent,
            "threshold": 90.0
        }));
        
        let memory_usage_percent = (snapshot.system_metrics.memory_usage_bytes as f64 / 
                                   snapshot.system_metrics.memory_total_bytes as f64) * 100.0;
        let memory_healthy = memory_usage_percent < 85.0;
        checks.insert("memory".to_string(), serde_json::json!({
            "healthy": memory_healthy,
            "usage_percent": memory_usage_percent,
            "threshold": 85.0
        }));
        
        // Check file descriptors
        let fd_healthy = snapshot.system_metrics.open_file_descriptors < 8192; // Reasonable limit
        checks.insert("file_descriptors".to_string(), serde_json::json!({
            "healthy": fd_healthy,
            "count": snapshot.system_metrics.open_file_descriptors,
            "threshold": 8192
        }));
        
        // Check network connectivity
        let network_healthy = snapshot.network_metrics.connection_success_rate > 0.8;
        checks.insert("network".to_string(), serde_json::json!({
            "healthy": network_healthy,
            "success_rate": snapshot.network_metrics.connection_success_rate,
            "threshold": 0.8
        }));
        
        // Check layer health
        let mut layer_health = true;
        let mut layer_statuses = serde_json::Map::new();
        
        for (layer_type, metrics) in &snapshot.layer_metrics {
            let layer_healthy = !matches!(metrics.status, LayerStatus::Failed);
            layer_health &= layer_healthy;
            
            layer_statuses.insert(format!("{:?}", layer_type).to_lowercase(), serde_json::json!({
                "healthy": layer_healthy,
                "status": format!("{:?}", metrics.status),
                "error_rate": metrics.error_rate
            }));
        }
        
        checks.insert("layers".to_string(), serde_json::json!({
            "healthy": layer_health,
            "details": layer_statuses
        }));
        
        Ok(serde_json::Value::Object(checks))
    }
    
    /// Export system metrics in Prometheus format
    fn export_system_metrics(
        output: &mut String,
        metrics: &SystemMetrics,
        labels: &HashMap<String, String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let label_str = Self::format_labels(labels);
        
        // CPU usage
        output.push_str("# HELP nyx_cpu_usage_percent CPU usage percentage\n");
        output.push_str("# TYPE nyx_cpu_usage_percent gauge\n");
        output.push_str(&format!("nyx_cpu_usage_percent{{{label_str}}} {}\n", metrics.cpu_usage_percent));
        
        // Memory metrics
        output.push_str("# HELP nyx_memory_usage_bytes Memory usage in bytes\n");
        output.push_str("# TYPE nyx_memory_usage_bytes gauge\n");
        output.push_str(&format!("nyx_memory_usage_bytes{{{label_str}}} {}\n", metrics.memory_usage_bytes));
        
        output.push_str("# HELP nyx_memory_total_bytes Total memory in bytes\n");
        output.push_str("# TYPE nyx_memory_total_bytes gauge\n");
        output.push_str(&format!("nyx_memory_total_bytes{{{label_str}}} {}\n", metrics.memory_total_bytes));
        
        // Network I/O
        output.push_str("# HELP nyx_network_bytes_sent_total Total bytes sent over network\n");
        output.push_str("# TYPE nyx_network_bytes_sent_total counter\n");
        output.push_str(&format!("nyx_network_bytes_sent_total{{{label_str}}} {}\n", metrics.network_bytes_sent));
        
        output.push_str("# HELP nyx_network_bytes_received_total Total bytes received over network\n");
        output.push_str("# TYPE nyx_network_bytes_received_total counter\n");
        output.push_str(&format!("nyx_network_bytes_received_total{{{label_str}}} {}\n", metrics.network_bytes_received));
        
        // File descriptors
        output.push_str("# HELP nyx_open_file_descriptors Number of open file descriptors\n");
        output.push_str("# TYPE nyx_open_file_descriptors gauge\n");
        output.push_str(&format!("nyx_open_file_descriptors{{{label_str}}} {}\n", metrics.open_file_descriptors));
        
        // Threads
        output.push_str("# HELP nyx_thread_count Number of threads\n");
        output.push_str("# TYPE nyx_thread_count gauge\n");
        output.push_str(&format!("nyx_thread_count{{{label_str}}} {}\n", metrics.thread_count));
        
        // Uptime
        output.push_str("# HELP nyx_uptime_seconds Uptime in seconds\n");
        output.push_str("# TYPE nyx_uptime_seconds counter\n");
        output.push_str(&format!("nyx_uptime_seconds{{{label_str}}} {}\n", metrics.uptime_seconds));
        
        output.push_str("\n");
        Ok(())
    }
    
    /// Export network metrics in Prometheus format
    fn export_network_metrics(
        output: &mut String,
        metrics: &NetworkMetrics,
        labels: &HashMap<String, String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let label_str = Self::format_labels(labels);
        
        // Connection metrics
        output.push_str("# HELP nyx_active_connections Number of active connections\n");
        output.push_str("# TYPE nyx_active_connections gauge\n");
        output.push_str(&format!("nyx_active_connections{{{label_str}}} {}\n", metrics.active_connections));
        
        output.push_str("# HELP nyx_total_connections_total Total connections established\n");
        output.push_str("# TYPE nyx_total_connections_total counter\n");
        output.push_str(&format!("nyx_total_connections_total{{{label_str}}} {}\n", metrics.total_connections));
        
        output.push_str("# HELP nyx_failed_connections_total Total failed connections\n");
        output.push_str("# TYPE nyx_failed_connections_total counter\n");
        output.push_str(&format!("nyx_failed_connections_total{{{label_str}}} {}\n", metrics.failed_connections));
        
        output.push_str("# HELP nyx_connection_success_rate Connection success rate\n");
        output.push_str("# TYPE nyx_connection_success_rate gauge\n");
        output.push_str(&format!("nyx_connection_success_rate{{{label_str}}} {}\n", metrics.connection_success_rate));
        
        // Latency
        output.push_str("# HELP nyx_average_latency_seconds Average latency in seconds\n");
        output.push_str("# TYPE nyx_average_latency_seconds gauge\n");
        output.push_str(&format!("nyx_average_latency_seconds{{{label_str}}} {}\n", metrics.average_latency.as_secs_f64()));
        
        // Packet loss
        output.push_str("# HELP nyx_packet_loss_rate Packet loss rate\n");
        output.push_str("# TYPE nyx_packet_loss_rate gauge\n");
        output.push_str(&format!("nyx_packet_loss_rate{{{label_str}}} {}\n", metrics.packet_loss_rate));
        
        // Bandwidth utilization
        output.push_str("# HELP nyx_bandwidth_utilization Bandwidth utilization ratio\n");
        output.push_str("# TYPE nyx_bandwidth_utilization gauge\n");
        output.push_str(&format!("nyx_bandwidth_utilization{{{label_str}}} {}\n", metrics.bandwidth_utilization));
        
        // Peer and route counts
        output.push_str("# HELP nyx_peer_count Number of connected peers\n");
        output.push_str("# TYPE nyx_peer_count gauge\n");
        output.push_str(&format!("nyx_peer_count{{{label_str}}} {}\n", metrics.peer_count));
        
        output.push_str("# HELP nyx_route_count Number of available routes\n");
        output.push_str("# TYPE nyx_route_count gauge\n");
        output.push_str(&format!("nyx_route_count{{{label_str}}} {}\n", metrics.route_count));
        
        output.push_str("\n");
        Ok(())
    }
    
    /// Export layer-specific metrics in Prometheus format
    fn export_layer_metrics(
        output: &mut String,
        layer_metrics: &HashMap<LayerType, LayerMetrics>,
        labels: &HashMap<String, String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for (layer_type, metrics) in layer_metrics {
            let layer_name = format!("{:?}", layer_type).to_lowercase();
            let mut layer_labels = labels.clone();
            layer_labels.insert("layer".to_string(), layer_name.clone());
            let label_str = Self::format_labels(&layer_labels);
            
            // Layer status
            output.push_str(&format!("# HELP nyx_layer_status Layer status (0=Initializing, 1=Active, 2=Degraded, 3=Failed, 4=Shutdown)\n"));
            output.push_str(&format!("# TYPE nyx_layer_status gauge\n"));
            let status_value = match metrics.status {
                LayerStatus::Initializing => 0,
                LayerStatus::Active => 1,
                LayerStatus::Degraded => 2,
                LayerStatus::Failed => 3,
                LayerStatus::Shutdown => 4,
            };
            output.push_str(&format!("nyx_layer_status{{{label_str}}} {}\n", status_value));
            
            // Throughput
            output.push_str(&format!("# HELP nyx_layer_throughput_bytes_per_second Layer throughput in bytes per second\n"));
            output.push_str(&format!("# TYPE nyx_layer_throughput_bytes_per_second gauge\n"));
            output.push_str(&format!("nyx_layer_throughput_bytes_per_second{{{label_str}}} {}\n", metrics.throughput));
            
            // Latency
            output.push_str(&format!("# HELP nyx_layer_latency_seconds Layer latency in seconds\n"));
            output.push_str(&format!("# TYPE nyx_layer_latency_seconds gauge\n"));
            output.push_str(&format!("nyx_layer_latency_seconds{{{label_str}}} {}\n", metrics.latency.as_secs_f64()));
            
            // Error rate
            output.push_str(&format!("# HELP nyx_layer_error_rate Layer error rate\n"));
            output.push_str(&format!("# TYPE nyx_layer_error_rate gauge\n"));
            output.push_str(&format!("nyx_layer_error_rate{{{label_str}}} {}\n", metrics.error_rate));
            
            // Packets processed
            output.push_str(&format!("# HELP nyx_layer_packets_processed_total Total packets processed by layer\n"));
            output.push_str(&format!("# TYPE nyx_layer_packets_processed_total counter\n"));
            output.push_str(&format!("nyx_layer_packets_processed_total{{{label_str}}} {}\n", metrics.packets_processed));
            
            // Bytes processed
            output.push_str(&format!("# HELP nyx_layer_bytes_processed_total Total bytes processed by layer\n"));
            output.push_str(&format!("# TYPE nyx_layer_bytes_processed_total counter\n"));
            output.push_str(&format!("nyx_layer_bytes_processed_total{{{label_str}}} {}\n", metrics.bytes_processed));
            
            // Active connections
            output.push_str(&format!("# HELP nyx_layer_active_connections Active connections for layer\n"));
            output.push_str(&format!("# TYPE nyx_layer_active_connections gauge\n"));
            output.push_str(&format!("nyx_layer_active_connections{{{label_str}}} {}\n", metrics.active_connections));
            
            // Queue depth
            output.push_str(&format!("# HELP nyx_layer_queue_depth Queue depth for layer\n"));
            output.push_str(&format!("# TYPE nyx_layer_queue_depth gauge\n"));
            output.push_str(&format!("nyx_layer_queue_depth{{{label_str}}} {}\n", metrics.queue_depth));
        }
        
        output.push_str("\n");
        Ok(())
    }
    
    /// Export performance metrics in Prometheus format
    fn export_performance_metrics(
        output: &mut String,
        metrics: &crate::proto::PerformanceMetrics,
        labels: &HashMap<String, String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let label_str = Self::format_labels(labels);
        
        // Cover traffic rate
        output.push_str("# HELP nyx_cover_traffic_rate Cover traffic rate in packets per second\n");
        output.push_str("# TYPE nyx_cover_traffic_rate gauge\n");
        output.push_str(&format!("nyx_cover_traffic_rate{{{label_str}}} {}\n", metrics.cover_traffic_rate));
        
        // Average latency
        output.push_str("# HELP nyx_avg_latency_milliseconds Average latency in milliseconds\n");
        output.push_str("# TYPE nyx_avg_latency_milliseconds gauge\n");
        output.push_str(&format!("nyx_avg_latency_milliseconds{{{label_str}}} {}\n", metrics.avg_latency_ms));
        
        // Packet loss rate
        output.push_str("# HELP nyx_packet_loss_rate_percent Packet loss rate as percentage\n");
        output.push_str("# TYPE nyx_packet_loss_rate_percent gauge\n");
        output.push_str(&format!("nyx_packet_loss_rate_percent{{{label_str}}} {}\n", metrics.packet_loss_rate * 100.0));
        
        // Bandwidth utilization
        output.push_str("# HELP nyx_bandwidth_utilization_percent Bandwidth utilization as percentage\n");
        output.push_str("# TYPE nyx_bandwidth_utilization_percent gauge\n");
        output.push_str(&format!("nyx_bandwidth_utilization_percent{{{label_str}}} {}\n", metrics.bandwidth_utilization * 100.0));
        
        // CPU usage
        output.push_str("# HELP nyx_process_cpu_usage_percent Process CPU usage percentage\n");
        output.push_str("# TYPE nyx_process_cpu_usage_percent gauge\n");
        output.push_str(&format!("nyx_process_cpu_usage_percent{{{label_str}}} {}\n", metrics.cpu_usage * 100.0));
        
        // Memory usage
        output.push_str("# HELP nyx_process_memory_usage_megabytes Process memory usage in megabytes\n");
        output.push_str("# TYPE nyx_process_memory_usage_megabytes gauge\n");
        output.push_str(&format!("nyx_process_memory_usage_megabytes{{{label_str}}} {}\n", metrics.memory_usage_mb));
        
        // Packet counters
        output.push_str("# HELP nyx_packets_sent_total Total packets sent\n");
        output.push_str("# TYPE nyx_packets_sent_total counter\n");
        output.push_str(&format!("nyx_packets_sent_total{{{label_str}}} {}\n", metrics.total_packets_sent));
        
        output.push_str("# HELP nyx_packets_received_total Total packets received\n");
        output.push_str("# TYPE nyx_packets_received_total counter\n");
        output.push_str(&format!("nyx_packets_received_total{{{label_str}}} {}\n", metrics.total_packets_received));
        
        output.push_str("# HELP nyx_retransmissions_total Total retransmissions\n");
        output.push_str("# TYPE nyx_retransmissions_total counter\n");
        output.push_str(&format!("nyx_retransmissions_total{{{label_str}}} {}\n", metrics.retransmissions));
        
        output.push_str("\n");
        Ok(())
    }
    
    /// Export error metrics in Prometheus format
    fn export_error_metrics(
        output: &mut String,
        metrics: &ErrorMetrics,
        labels: &HashMap<String, String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let label_str = Self::format_labels(labels);
        
        // Total errors
        output.push_str("# HELP nyx_errors_total Total number of errors\n");
        output.push_str("# TYPE nyx_errors_total counter\n");
        output.push_str(&format!("nyx_errors_total{{{label_str}}} {}\n", metrics.total_errors));
        
        // Error rate
        output.push_str("# HELP nyx_error_rate Errors per minute\n");
        output.push_str("# TYPE nyx_error_rate gauge\n");
        output.push_str(&format!("nyx_error_rate{{{label_str}}} {}\n", metrics.error_rate));
        
        // Critical errors
        output.push_str("# HELP nyx_critical_errors_total Total critical errors\n");
        output.push_str("# TYPE nyx_critical_errors_total counter\n");
        output.push_str(&format!("nyx_critical_errors_total{{{label_str}}} {}\n", metrics.critical_errors));
        
        // Warning errors
        output.push_str("# HELP nyx_warning_errors_total Total warning errors\n");
        output.push_str("# TYPE nyx_warning_errors_total counter\n");
        output.push_str(&format!("nyx_warning_errors_total{{{label_str}}} {}\n", metrics.warning_errors));
        
        // Errors by layer
        for (layer, count) in &metrics.errors_by_layer {
            let layer_name = format!("{:?}", layer).to_lowercase();
            let mut layer_labels = labels.clone();
            layer_labels.insert("layer".to_string(), layer_name);
            let layer_label_str = Self::format_labels(&layer_labels);
            
            output.push_str("# HELP nyx_errors_by_layer_total Errors by layer\n");
            output.push_str("# TYPE nyx_errors_by_layer_total counter\n");
            output.push_str(&format!("nyx_errors_by_layer_total{{{layer_label_str}}} {}\n", count));
        }
        
        // Errors by type
        for (error_type, count) in &metrics.errors_by_type {
            let mut type_labels = labels.clone();
            type_labels.insert("error_type".to_string(), error_type.clone());
            let type_label_str = Self::format_labels(&type_labels);
            
            output.push_str("# HELP nyx_errors_by_type_total Errors by type\n");
            output.push_str("# TYPE nyx_errors_by_type_total counter\n");
            output.push_str(&format!("nyx_errors_by_type_total{{{type_label_str}}} {}\n", count));
        }
        
        output.push_str("\n");
        Ok(())
    }
    
    /// Export alert metrics in Prometheus format
    fn export_alert_metrics(
        output: &mut String,
        active_alerts: &HashMap<String, Alert>,
        labels: &HashMap<String, String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let label_str = Self::format_labels(labels);
        
        // Total active alerts
        output.push_str("# HELP nyx_active_alerts Total number of active alerts\n");
        output.push_str("# TYPE nyx_active_alerts gauge\n");
        output.push_str(&format!("nyx_active_alerts{{{label_str}}} {}\n", active_alerts.len()));
        
        // Alerts by severity
        let mut severity_counts = HashMap::new();
        for alert in active_alerts.values() {
            *severity_counts.entry(&alert.severity).or_insert(0) += 1;
        }
        
        for (severity, count) in severity_counts {
            let severity_name = format!("{:?}", severity).to_lowercase();
            let mut severity_labels = labels.clone();
            severity_labels.insert("severity".to_string(), severity_name);
            let severity_label_str = Self::format_labels(&severity_labels);
            
            output.push_str("# HELP nyx_alerts_by_severity Active alerts by severity\n");
            output.push_str("# TYPE nyx_alerts_by_severity gauge\n");
            output.push_str(&format!("nyx_alerts_by_severity{{{severity_label_str}}} {}\n", count));
        }
        
        output.push_str("\n");
        Ok(())
    }
    
    /// Format labels for Prometheus metrics
    fn format_labels(labels: &HashMap<String, String>) -> String {
        if labels.is_empty() {
            return String::new();
        }
        
        let mut label_pairs: Vec<String> = labels
            .iter()
            .map(|(k, v)| format!("{}=\"{}\"", k, v))
            .collect();
        label_pairs.sort();
        label_pairs.join(",")
    }
    
    /// Stop the Prometheus server
    pub async fn stop(&mut self) {
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
        }
    }
    
    /// Get the server address
    pub fn server_addr(&self) -> std::net::SocketAddr {
        self.server_addr
    }
    
    /// Set collection interval
    pub fn set_collection_interval(&mut self, interval: Duration) {
        self.collection_interval = interval;
    }
    
    /// Add default label
    pub fn add_default_label(&mut self, key: String, value: String) {
        self.default_labels.insert(key, value);
    }
    
    /// Get metrics endpoint URL
    pub fn metrics_url(&self) -> String {
        format!("http://{}/metrics", self.server_addr)
    }
    
    /// Get health endpoint URL
    pub fn health_url(&self) -> String {
        format!("http://{}/health", self.server_addr)
    }
    
    /// Check if server is running
    pub fn is_running(&self) -> bool {
        self.server_handle.as_ref().map_or(false, |h| !h.is_finished())
    }
    
    /// Get server statistics
    pub async fn get_server_stats(&self) -> ServerStats {
        let is_running = self.is_running();
        let uptime = if is_running {
            // This would need to be tracked from server start time
            Some(Duration::from_secs(0)) // Placeholder
        } else {
            None
        };
        
        ServerStats {
            is_running,
            server_addr: self.server_addr,
            uptime,
            collection_interval: self.collection_interval,
            default_labels: self.default_labels.clone(),
        }
    }
    
    /// Restart the server with new configuration
    pub async fn restart(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Restarting Prometheus metrics server");
        self.stop().await;
        tokio::time::sleep(Duration::from_millis(100)).await; // Brief pause
        self.start_server().await
    }
    
    /// Graceful shutdown with timeout
    pub async fn shutdown_gracefully(&mut self, timeout: Duration) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(handle) = self.server_handle.take() {
            info!("Shutting down Prometheus metrics server gracefully");
            
            // Try graceful shutdown first
            handle.abort();
            
            // Wait for shutdown with timeout
            match tokio::time::timeout(timeout, handle).await {
                Ok(_) => {
                    info!("Prometheus server shut down gracefully");
                    Ok(())
                }
                Err(_) => {
                    warn!("Prometheus server shutdown timed out after {:?}", timeout);
                    Err("Shutdown timeout".into())
                }
            }
        } else {
            Ok(())
        }
    }
}

/// Server statistics for monitoring
#[derive(Debug, Clone)]
pub struct ServerStats {
    pub is_running: bool,
    pub server_addr: std::net::SocketAddr,
    pub uptime: Option<Duration>,
    pub collection_interval: Duration,
    pub default_labels: HashMap<String, String>,
}