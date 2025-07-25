use std::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, Instant};

use sysinfo::{System, Pid};

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
                if let Ok(mut sys) = system.write() {
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
                if let Ok(mut rate) = cover_traffic_rate.write() {
                    *rate = packets_delta; // packets per second
                }
                
                // Calculate bandwidth utilization (simplified)
                let bytes_delta = (current_bytes_sent + current_bytes_received) - 
                                 (last_bytes_sent + last_bytes_received);
                let bandwidth_mbps = (bytes_delta as f64 * 8.0) / (1024.0 * 1024.0); // Convert to Mbps
                
                // Store bandwidth sample
                if let Ok(mut samples) = bandwidth_samples.write() {
                    samples.push(bandwidth_mbps);
                    if samples.len() > 100 { // Keep last 100 samples
                        samples.remove(0);
                    }
                }
                
                if let Ok(mut util) = bandwidth_utilization.write() {
                    *util = (bandwidth_mbps / 1000.0).min(1.0); // Assume 1Gbps max, normalize to 0-1
                }
                
                // Update system metrics and store samples
                if let Ok(mut sys) = system.write() {
                    sys.refresh_cpu();
                    sys.refresh_memory();
                    
                    let cpu_usage = sys.global_cpu_info().cpu_usage() as f64;
                    let memory_usage = (sys.used_memory() as f64 / sys.total_memory() as f64) * 100.0;
                    
                    // Store CPU samples
                    if let Ok(mut samples) = cpu_samples.write() {
                        samples.push(cpu_usage);
                        if samples.len() > 100 {
                            samples.remove(0);
                        }
                    }
                    
                    // Store memory samples
                    if let Ok(mut samples) = memory_samples.write() {
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
                if let Ok(mut loss) = packet_loss_rate.write() {
                    *loss = loss_rate;
                }
                
                // Update last values
                last_bytes_sent = current_bytes_sent;
                last_bytes_received = current_bytes_received;
                last_packets_sent = current_packets_sent;
                last_packets_received = current_packets_received;
                last_retransmissions = current_retransmissions;
                
                // Update last update time
                if let Ok(mut update_time) = last_update.write() {
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
    pub fn add_mix_route(&self, route: String) {
        if let Ok(mut routes) = self.mix_routes.write() {
            if !routes.contains(&route) {
                routes.push(route);
            }
        }
    }

    pub fn remove_mix_route(&self, route: &str) {
        if let Ok(mut routes) = self.mix_routes.write() {
            routes.retain(|r| r != route);
        }
    }

    pub fn get_mix_routes(&self) -> Vec<String> {
        self.mix_routes.read().unwrap().clone()
    }

    // Connection tracking
    pub fn record_connection_attempt(&self) {
        self.connection_attempts.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_successful_connection(&self) {
        self.successful_connections.fetch_add(1, Ordering::Relaxed);
    }
    
    // Latency tracking
    pub fn record_latency(&self, latency_ms: f64) {
        if let Ok(mut samples) = self.latency_samples.write() {
            samples.push(latency_ms);
            
            // Keep only the most recent samples
            if samples.len() > self.max_latency_samples {
                samples.remove(0);
            }
            
            // Update average latency
            let avg = samples.iter().sum::<f64>() / samples.len() as f64;
            if let Ok(mut avg_latency) = self.avg_latency_ms.write() {
                *avg_latency = avg;
            }
        }
    }
    
    // Bandwidth tracking
    pub fn record_bandwidth(&self, bandwidth_mbps: f64) {
        if let Ok(mut samples) = self.bandwidth_samples.write() {
            samples.push(bandwidth_mbps);
            if samples.len() > 100 {
                samples.remove(0);
            }
        }
    }
    
    // Get performance statistics
    pub fn get_bandwidth_samples(&self) -> Vec<f64> {
        self.bandwidth_samples.read().unwrap().clone()
    }
    
    pub fn get_cpu_samples(&self) -> Vec<f64> {
        self.cpu_samples.read().unwrap().clone()
    }
    
    pub fn get_memory_samples(&self) -> Vec<f64> {
        self.memory_samples.read().unwrap().clone()
    }

    pub fn get_uptime(&self) -> Duration {
        SystemTime::now().duration_since(self.start_time).unwrap_or_default()
    }

    pub fn get_performance_metrics(&self) -> crate::proto::PerformanceMetrics {
        let cover_traffic_rate = *self.cover_traffic_rate.read().unwrap();
        let avg_latency_ms = *self.avg_latency_ms.read().unwrap();
        let packet_loss_rate = *self.packet_loss_rate.read().unwrap();
        let bandwidth_utilization = *self.bandwidth_utilization.read().unwrap();

        // Get CPU and memory usage from system
        let (cpu_usage, memory_usage_mb) = if let Ok(system) = self.system.read() {
            let current_pid = std::process::id();
            if let Some(process) = system.process(sysinfo::Pid::from(current_pid as usize)) {
                (
                    process.cpu_usage() as f64 / 100.0, // Convert to 0-1 range
                    process.memory() as f64 / (1024.0 * 1024.0), // Convert to MB
                )
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
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

    pub fn get_resource_usage(&self) -> Option<crate::proto::ResourceUsage> {
        if let Ok(system) = self.system.read() {
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
        } else {
            None
        }
    }

    pub fn get_active_streams_count(&self) -> u64 {
        self.active_streams.load(Ordering::Relaxed) as u64
    }

    pub fn get_connected_peers_count(&self) -> u64 {
        self.connected_peers.load(Ordering::Relaxed) as u64
    }
} 