use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// Metrics collector for daemon performance monitoring
#[derive(Debug)]
pub struct MetricsCollector {
    pub total_requests: AtomicU64,
    pub successful_requests: AtomicU64,
    pub failed_requests: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    pub active_connections: AtomicU64,
    pub start_time: SystemTime,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            successful_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            active_connections: AtomicU64::new(0),
            start_time: SystemTime::now(),
        }
    }

    pub fn start_collection(&self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async {
            // Background metrics collection task
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                // Collect metrics here
            }
        })
    }

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
        self.successful_requests.fetch_add(1, Ordering::Relaxed);
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

    pub fn increment_connections(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement_connections(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn get_uptime(&self) -> Duration {
        SystemTime::now().duration_since(self.start_time).unwrap_or_default()
    }

    pub fn get_performance_metrics(&self) -> crate::proto::PerformanceMetrics {
        crate::proto::PerformanceMetrics {
            cover_traffic_rate: 0.0,
            avg_latency_ms: 0.0,
            packet_loss_rate: 0.0,
            bandwidth_utilization: 0.0,
            cpu_usage: 0.0,
            memory_usage_mb: 0.0,
            total_packets_sent: self.successful_requests.load(Ordering::Relaxed),
            total_packets_received: self.bytes_received.load(Ordering::Relaxed),
            retransmissions: 0,
            connection_success_rate: 1.0,
        }
    }

    pub fn get_resource_usage(&self) -> Option<crate::proto::ResourceUsage> {
        Some(crate::proto::ResourceUsage {
            memory_rss_bytes: 0,
            memory_vms_bytes: 0,
            cpu_percent: 0.0,
            open_file_descriptors: 0,
            thread_count: 0,
            network_bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            network_bytes_received: self.bytes_received.load(Ordering::Relaxed),
        })
    }

    pub fn get_active_streams_count(&self) -> u64 {
        self.active_connections.load(Ordering::Relaxed)
    }

    pub fn get_connected_peers_count(&self) -> u64 {
        self.active_connections.load(Ordering::Relaxed)
    }
    
    pub fn set_active_streams(&self, count: usize) {
        self.active_connections.store(count as u64, Ordering::Relaxed);
    }
} 