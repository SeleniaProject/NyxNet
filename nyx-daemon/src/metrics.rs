#![forbid(unsafe_code)]

//! Comprehensive metrics collection and monitoring system for Nyx daemon.
//!
//! This module provides real-time monitoring of:
//! - System resources (CPU, memory, network, file descriptors)
//! - Network performance (latency, bandwidth, packet loss)
//! - Stream statistics (active streams, throughput, errors)
//! - Mix routing metrics (cover traffic, path utilization)
//! - Security metrics (handshake success rate, authentication failures)

use crate::proto::{PerformanceMetrics, ResourceUsage};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{System, SystemExt, CpuExt, ProcessExt, NetworkExt, DiskExt};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Exponential moving average smoothing factor
const EMA_ALPHA: f64 = 0.1;

/// Network interface statistics
#[derive(Debug, Clone, Default)]
pub struct NetworkStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub errors: u64,
    pub dropped: u64,
}

/// Stream performance metrics
#[derive(Debug, Clone, Default)]
pub struct StreamMetrics {
    pub active_count: usize,
    pub total_opened: u64,
    pub total_closed: u64,
    pub total_errors: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub avg_latency_ms: f64,
    pub packet_loss_rate: f64,
    pub connection_success_rate: f64,
}

/// Mix routing metrics
#[derive(Debug, Clone, Default)]
pub struct MixMetrics {
    pub cover_traffic_rate: f64,
    pub total_mix_packets: u64,
    pub active_paths: usize,
    pub path_failures: u64,
    pub batch_processing_time_ms: f64,
    pub anonymity_set_size: usize,
}

/// Security metrics
#[derive(Debug, Clone, Default)]
pub struct SecurityMetrics {
    pub handshake_attempts: u64,
    pub handshake_successes: u64,
    pub authentication_failures: u64,
    pub invalid_packets: u64,
    pub replay_attacks_detected: u64,
    pub post_quantum_handshakes: u64,
}

/// Comprehensive metrics collector
pub struct MetricsCollector {
    // System monitoring
    system: Arc<RwLock<System>>,
    process_id: u32,
    
    // Atomic counters for high-frequency updates
    total_packets_sent: AtomicU64,
    total_packets_received: AtomicU64,
    total_bytes_sent: AtomicU64,
    total_bytes_received: AtomicU64,
    retransmissions: AtomicU64,
    active_streams: AtomicUsize,
    connected_peers: AtomicUsize,
    
    // Smoothed metrics using exponential moving averages
    smoothed_metrics: Arc<RwLock<SmoothedMetrics>>,
    
    // Per-stream metrics
    stream_metrics: Arc<DashMap<u32, StreamMetrics>>,
    
    // Network interface statistics
    network_stats: Arc<RwLock<NetworkStats>>,
    
    // Mix routing metrics
    mix_metrics: Arc<RwLock<MixMetrics>>,
    
    // Security metrics
    security_metrics: Arc<RwLock<SecurityMetrics>>,
    
    // Historical data for trend analysis
    history: Arc<RwLock<MetricsHistory>>,
}

#[derive(Debug, Clone, Default)]
struct SmoothedMetrics {
    cpu_usage: f64,
    memory_usage_mb: f64,
    avg_latency_ms: f64,
    bandwidth_utilization: f64,
    packet_loss_rate: f64,
    cover_traffic_rate: f64,
    connection_success_rate: f64,
}

#[derive(Debug, Default)]
struct MetricsHistory {
    cpu_samples: Vec<(SystemTime, f64)>,
    memory_samples: Vec<(SystemTime, f64)>,
    latency_samples: Vec<(SystemTime, f64)>,
    bandwidth_samples: Vec<(SystemTime, f64)>,
    max_samples: usize,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        
        let process_id = std::process::id();
        
        Self {
            system: Arc::new(RwLock::new(system)),
            process_id,
            total_packets_sent: AtomicU64::new(0),
            total_packets_received: AtomicU64::new(0),
            total_bytes_sent: AtomicU64::new(0),
            total_bytes_received: AtomicU64::new(0),
            retransmissions: AtomicU64::new(0),
            active_streams: AtomicUsize::new(0),
            connected_peers: AtomicUsize::new(0),
            smoothed_metrics: Arc::new(RwLock::new(SmoothedMetrics::default())),
            stream_metrics: Arc::new(DashMap::new()),
            network_stats: Arc::new(RwLock::new(NetworkStats::default())),
            mix_metrics: Arc::new(RwLock::new(MixMetrics::default())),
            security_metrics: Arc::new(RwLock::new(SecurityMetrics::default())),
            history: Arc::new(RwLock::new(MetricsHistory {
                cpu_samples: Vec::new(),
                memory_samples: Vec::new(),
                latency_samples: Vec::new(),
                bandwidth_samples: Vec::new(),
                max_samples: 1000, // Keep last 1000 samples
            })),
        }
    }
    
    /// Start the metrics collection background task
    pub fn start_collection(&self) -> tokio::task::JoinHandle<()> {
        let collector = self.clone();
        tokio::spawn(async move {
            collector.collection_loop().await;
        })
    }
    
    /// Main collection loop
    async fn collection_loop(&self) {
        let mut interval = interval(Duration::from_secs(1));
        
        loop {
            interval.tick().await;
            
            if let Err(e) = self.collect_system_metrics().await {
                error!("Failed to collect system metrics: {}", e);
            }
            
            if let Err(e) = self.collect_network_metrics().await {
                error!("Failed to collect network metrics: {}", e);
            }
            
            self.update_smoothed_metrics().await;
            self.cleanup_old_data().await;
            
            debug!("Metrics collection cycle completed");
        }
    }
    
    /// Collect system resource metrics
    async fn collect_system_metrics(&self) -> anyhow::Result<()> {
        let mut system = self.system.write();
        system.refresh_all();
        
        // CPU usage
        let cpu_usage = system.global_cpu_info().cpu_usage() as f64;
        
        // Memory usage
        let total_memory = system.total_memory() as f64;
        let used_memory = system.used_memory() as f64;
        let memory_usage_mb = used_memory / 1_048_576.0; // Convert to MB
        
        // Process-specific metrics
        let process_memory = if let Some(process) = system.process(sysinfo::Pid::from(self.process_id as usize)) {
            process.memory() as f64 / 1_048_576.0
        } else {
            0.0
        };
        
        // Update smoothed metrics
        {
            let mut smoothed = self.smoothed_metrics.write();
            smoothed.cpu_usage = Self::update_ema(smoothed.cpu_usage, cpu_usage, EMA_ALPHA);
            smoothed.memory_usage_mb = Self::update_ema(smoothed.memory_usage_mb, process_memory, EMA_ALPHA);
        }
        
        // Store historical data
        let now = SystemTime::now();
        {
            let mut history = self.history.write();
            
            // Add new samples
            history.cpu_samples.push((now, cpu_usage));
            history.memory_samples.push((now, process_memory));
            
            // Trim old samples
            if history.cpu_samples.len() > history.max_samples {
                history.cpu_samples.remove(0);
            }
            if history.memory_samples.len() > history.max_samples {
                history.memory_samples.remove(0);
            }
        }
        
        Ok(())
    }
    
    /// Collect network interface metrics
    async fn collect_network_metrics(&self) -> anyhow::Result<()> {
        let system = self.system.read();
        let networks = system.networks();
        
        let mut total_bytes_sent = 0u64;
        let mut total_bytes_received = 0u64;
        let mut total_packets_sent = 0u64;
        let mut total_packets_received = 0u64;
        let mut total_errors = 0u64;
        
        for (_interface_name, network) in networks {
            total_bytes_sent += network.transmitted();
            total_bytes_received += network.received();
            total_packets_sent += network.packets_transmitted();
            total_packets_received += network.packets_received();
            total_errors += network.errors_on_transmitted() + network.errors_on_received();
        }
        
        let mut network_stats = self.network_stats.write();
        network_stats.bytes_sent = total_bytes_sent;
        network_stats.bytes_received = total_bytes_received;
        network_stats.packets_sent = total_packets_sent;
        network_stats.packets_received = total_packets_received;
        network_stats.errors = total_errors;
        
        Ok(())
    }
    
    /// Update exponential moving average
    fn update_ema(current: f64, new_value: f64, alpha: f64) -> f64 {
        if current == 0.0 {
            new_value
        } else {
            alpha * new_value + (1.0 - alpha) * current
        }
    }
    
    /// Update all smoothed metrics
    async fn update_smoothed_metrics(&self) {
        // This would be called by other components to update performance metrics
        // For now, we'll implement basic updates
        
        let active_streams = self.active_streams.load(Ordering::Relaxed);
        let total_packets_sent = self.total_packets_sent.load(Ordering::Relaxed);
        let total_packets_received = self.total_packets_received.load(Ordering::Relaxed);
        
        // Calculate packet loss rate if we have data
        let packet_loss_rate = if total_packets_sent > 0 {
            let retransmissions = self.retransmissions.load(Ordering::Relaxed);
            retransmissions as f64 / total_packets_sent as f64
        } else {
            0.0
        };
        
        let mut smoothed = self.smoothed_metrics.write();
        smoothed.packet_loss_rate = Self::update_ema(
            smoothed.packet_loss_rate,
            packet_loss_rate,
            EMA_ALPHA
        );
    }
    
    /// Clean up old historical data
    async fn cleanup_old_data(&self) {
        let cutoff_time = SystemTime::now() - Duration::from_secs(3600); // Keep 1 hour of data
        
        let mut history = self.history.write();
        
        history.cpu_samples.retain(|(time, _)| *time > cutoff_time);
        history.memory_samples.retain(|(time, _)| *time > cutoff_time);
        history.latency_samples.retain(|(time, _)| *time > cutoff_time);
        history.bandwidth_samples.retain(|(time, _)| *time > cutoff_time);
    }
    
    /// Get current performance metrics
    pub fn get_performance_metrics(&self) -> PerformanceMetrics {
        let smoothed = self.smoothed_metrics.read();
        let mix_metrics = self.mix_metrics.read();
        let security_metrics = self.security_metrics.read();
        
        let connection_success_rate = if security_metrics.handshake_attempts > 0 {
            security_metrics.handshake_successes as f64 / security_metrics.handshake_attempts as f64
        } else {
            1.0 // Default to 100% if no attempts yet
        };
        
        PerformanceMetrics {
            cover_traffic_rate: smoothed.cover_traffic_rate,
            avg_latency_ms: smoothed.avg_latency_ms,
            packet_loss_rate: smoothed.packet_loss_rate,
            bandwidth_utilization: smoothed.bandwidth_utilization,
            cpu_usage: smoothed.cpu_usage / 100.0, // Convert to 0.0-1.0 range
            memory_usage_mb: smoothed.memory_usage_mb,
            total_packets_sent: self.total_packets_sent.load(Ordering::Relaxed),
            total_packets_received: self.total_packets_received.load(Ordering::Relaxed),
            retransmissions: self.retransmissions.load(Ordering::Relaxed),
            connection_success_rate,
        }
    }
    
    /// Get current resource usage
    pub fn get_resource_usage(&self) -> anyhow::Result<ResourceUsage> {
        let system = self.system.read();
        
        let process = system.process(sysinfo::Pid::from(self.process_id as usize))
            .ok_or_else(|| anyhow::anyhow!("Process not found"))?;
        
        let network_stats = self.network_stats.read();
        
        Ok(ResourceUsage {
            memory_rss_bytes: process.memory() * 1024, // Convert from KB to bytes
            memory_vms_bytes: process.virtual_memory() * 1024,
            cpu_percent: process.cpu_usage() as f64,
            open_file_descriptors: 0, // Would need platform-specific implementation
            thread_count: 0, // Would need platform-specific implementation
            network_bytes_sent: network_stats.bytes_sent,
            network_bytes_received: network_stats.bytes_received,
        })
    }
    
    // Metric update methods for other components to call
    
    pub fn increment_packets_sent(&self, count: u64) {
        self.total_packets_sent.fetch_add(count, Ordering::Relaxed);
    }
    
    pub fn increment_packets_received(&self, count: u64) {
        self.total_packets_received.fetch_add(count, Ordering::Relaxed);
    }
    
    pub fn increment_bytes_sent(&self, bytes: u64) {
        self.total_bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }
    
    pub fn increment_bytes_received(&self, bytes: u64) {
        self.total_bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }
    
    pub fn increment_retransmissions(&self, count: u64) {
        self.retransmissions.fetch_add(count, Ordering::Relaxed);
    }
    
    pub fn set_active_streams(&self, count: usize) {
        self.active_streams.store(count, Ordering::Relaxed);
    }
    
    pub fn set_connected_peers(&self, count: usize) {
        self.connected_peers.store(count, Ordering::Relaxed);
    }
    
    pub fn update_latency(&self, latency_ms: f64) {
        let mut smoothed = self.smoothed_metrics.write();
        smoothed.avg_latency_ms = Self::update_ema(
            smoothed.avg_latency_ms,
            latency_ms,
            EMA_ALPHA
        );
        
        // Store in history
        let now = SystemTime::now();
        let mut history = self.history.write();
        history.latency_samples.push((now, latency_ms));
        if history.latency_samples.len() > history.max_samples {
            history.latency_samples.remove(0);
        }
    }
    
    pub fn update_bandwidth_utilization(&self, utilization: f64) {
        let mut smoothed = self.smoothed_metrics.write();
        smoothed.bandwidth_utilization = Self::update_ema(
            smoothed.bandwidth_utilization,
            utilization,
            EMA_ALPHA
        );
        
        // Store in history
        let now = SystemTime::now();
        let mut history = self.history.write();
        history.bandwidth_samples.push((now, utilization));
        if history.bandwidth_samples.len() > history.max_samples {
            history.bandwidth_samples.remove(0);
        }
    }
    
    pub fn update_cover_traffic_rate(&self, rate: f64) {
        let mut mix_metrics = self.mix_metrics.write();
        mix_metrics.cover_traffic_rate = rate;
        
        let mut smoothed = self.smoothed_metrics.write();
        smoothed.cover_traffic_rate = Self::update_ema(
            smoothed.cover_traffic_rate,
            rate,
            EMA_ALPHA
        );
    }
    
    pub fn record_handshake_attempt(&self, success: bool) {
        let mut security_metrics = self.security_metrics.write();
        security_metrics.handshake_attempts += 1;
        if success {
            security_metrics.handshake_successes += 1;
        }
    }
    
    pub fn record_authentication_failure(&self) {
        let mut security_metrics = self.security_metrics.write();
        security_metrics.authentication_failures += 1;
    }
    
    pub fn record_invalid_packet(&self) {
        let mut security_metrics = self.security_metrics.write();
        security_metrics.invalid_packets += 1;
    }
    
    pub fn record_replay_attack(&self) {
        let mut security_metrics = self.security_metrics.write();
        security_metrics.replay_attacks_detected += 1;
    }
    
    pub fn get_active_streams_count(&self) -> usize {
        self.active_streams.load(Ordering::Relaxed)
    }
    
    pub fn get_connected_peers_count(&self) -> usize {
        self.connected_peers.load(Ordering::Relaxed)
    }
}

impl Clone for MetricsCollector {
    fn clone(&self) -> Self {
        Self {
            system: Arc::clone(&self.system),
            process_id: self.process_id,
            total_packets_sent: AtomicU64::new(self.total_packets_sent.load(Ordering::Relaxed)),
            total_packets_received: AtomicU64::new(self.total_packets_received.load(Ordering::Relaxed)),
            total_bytes_sent: AtomicU64::new(self.total_bytes_sent.load(Ordering::Relaxed)),
            total_bytes_received: AtomicU64::new(self.total_bytes_received.load(Ordering::Relaxed)),
            retransmissions: AtomicU64::new(self.retransmissions.load(Ordering::Relaxed)),
            active_streams: AtomicUsize::new(self.active_streams.load(Ordering::Relaxed)),
            connected_peers: AtomicUsize::new(self.connected_peers.load(Ordering::Relaxed)),
            smoothed_metrics: Arc::clone(&self.smoothed_metrics),
            stream_metrics: Arc::clone(&self.stream_metrics),
            network_stats: Arc::clone(&self.network_stats),
            mix_metrics: Arc::clone(&self.mix_metrics),
            security_metrics: Arc::clone(&self.security_metrics),
            history: Arc::clone(&self.history),
        }
    }
} 