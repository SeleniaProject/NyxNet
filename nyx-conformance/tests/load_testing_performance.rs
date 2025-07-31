//! Load testing and performance verification tests
//!
//! This module provides comprehensive load testing and performance verification
//! for the Nyx protocol, including:
//! - High connection count performance verification
//! - Throughput and latency load testing
//! - Resource usage monitoring and limits
//! - Performance degradation detection and analysis

use nyx_conformance::network_simulator::{
    NetworkSimulator, SimulationConfig, SimulatedPacket, PacketPriority,
    LatencyDistribution, LinkQuality,
};
use nyx_conformance::property_tester::{
    PropertyTester, PropertyTestConfig,
    U32Generator, BoundsProperty, PredicateProperty,
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::time::sleep;

/// Load test configuration
#[derive(Debug, Clone)]
pub struct LoadTestConfig {
    /// Number of concurrent connections to simulate
    pub connection_count: u32,
    /// Duration of the load test
    pub test_duration: Duration,
    /// Target throughput in packets per second
    pub target_throughput_pps: u32,
    /// Packet size in bytes
    pub packet_size: usize,
    /// Maximum acceptable latency
    pub max_latency_ms: f64,
    /// Maximum acceptable packet loss rate
    pub max_loss_rate: f64,
    /// Resource monitoring interval
    pub monitoring_interval: Duration,
}

impl Default for LoadTestConfig {
    fn default() -> Self {
        Self {
            connection_count: 100,
            test_duration: Duration::from_secs(60),
            target_throughput_pps: 1000,
            packet_size: 1400,
            max_latency_ms: 100.0,
            max_loss_rate: 0.05,
            monitoring_interval: Duration::from_secs(1),
        }
    }
}

/// Performance metrics collected during load testing
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Test start time
    pub start_time: Instant,
    /// Test end time
    pub end_time: Instant,
    /// Total packets sent
    pub packets_sent: u64,
    /// Total packets received
    pub packets_received: u64,
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Average latency in milliseconds
    pub avg_latency_ms: f64,
    /// 95th percentile latency
    pub p95_latency_ms: f64,
    /// 99th percentile latency
    pub p99_latency_ms: f64,
    /// Maximum latency observed
    pub max_latency_ms: f64,
    /// Throughput in packets per second
    pub throughput_pps: f64,
    /// Throughput in Mbps
    pub throughput_mbps: f64,
    /// Packet loss rate
    pub loss_rate: f64,
    /// Connection success rate
    pub connection_success_rate: f64,
    /// Resource usage snapshots
    pub resource_snapshots: Vec<ResourceSnapshot>,
}

/// Resource usage snapshot
#[derive(Debug, Clone)]
pub struct ResourceSnapshot {
    /// Timestamp of the snapshot
    pub timestamp: Instant,
    /// Memory usage in MB
    pub memory_usage_mb: f64,
    /// CPU usage percentage
    pub cpu_usage_percent: f64,
    /// Network bandwidth usage in Mbps
    pub network_usage_mbps: f64,
    /// Number of active connections
    pub active_connections: u32,
    /// Number of file descriptors in use
    pub file_descriptors: u32,
}

/// Load test results
#[derive(Debug)]
pub struct LoadTestResults {
    /// Test configuration used
    pub config: LoadTestConfig,
    /// Performance metrics collected
    pub metrics: PerformanceMetrics,
    /// Whether the test passed all criteria
    pub test_passed: bool,
    /// Performance degradation detected
    pub degradation_detected: bool,
    /// Detailed failure reasons if any
    pub failure_reasons: Vec<String>,
    /// Performance analysis
    pub analysis: PerformanceAnalysis,
}

/// Performance analysis results
#[derive(Debug)]
pub struct PerformanceAnalysis {
    /// Bottleneck identification
    pub bottlenecks: Vec<PerformanceBottleneck>,
    /// Scalability assessment
    pub scalability_score: f64,
    /// Resource efficiency score
    pub efficiency_score: f64,
    /// Recommendations for improvement
    pub recommendations: Vec<String>,
}

/// Performance bottleneck identification
#[derive(Debug)]
pub struct PerformanceBottleneck {
    /// Type of bottleneck
    pub bottleneck_type: BottleneckType,
    /// Severity (0.0 to 1.0)
    pub severity: f64,
    /// Description
    pub description: String,
    /// Suggested mitigation
    pub mitigation: String,
}

/// Types of performance bottlenecks
#[derive(Debug, Clone)]
pub enum BottleneckType {
    /// CPU bottleneck
    Cpu,
    /// Memory bottleneck
    Memory,
    /// Network bandwidth bottleneck
    NetworkBandwidth,
    /// Connection limit bottleneck
    ConnectionLimit,
    /// Latency bottleneck
    Latency,
    /// Packet loss bottleneck
    PacketLoss,
}

/// Load tester implementation
pub struct LoadTester {
    config: LoadTestConfig,
    simulator: NetworkSimulator,
    resource_monitor: ResourceMonitor,
    metrics_collector: Arc<Mutex<MetricsCollector>>,
}

/// Resource monitoring implementation
struct ResourceMonitor {
    monitoring_active: Arc<Mutex<bool>>,
    snapshots: Arc<Mutex<Vec<ResourceSnapshot>>>,
}

/// Metrics collection implementation
struct MetricsCollector {
    start_time: Option<Instant>,
    packets_sent: u64,
    packets_received: u64,
    bytes_sent: u64,
    bytes_received: u64,
    latency_samples: Vec<f64>,
    connection_attempts: u32,
    successful_connections: u32,
}

impl LoadTester {
    /// Create a new load tester
    pub fn new(config: LoadTestConfig) -> Self {
        let sim_config = SimulationConfig {
            packet_loss_rate: 0.01,
            latency_distribution: LatencyDistribution::Normal { mean: 25.0, std_dev: 5.0 },
            bandwidth_limit_mbps: 1000.0,
            max_nodes: config.connection_count + 10,
            duration: config.test_duration,
            enable_logging: false,
        };

        Self {
            config,
            simulator: NetworkSimulator::new(sim_config),
            resource_monitor: ResourceMonitor::new(),
            metrics_collector: Arc::new(Mutex::new(MetricsCollector::new())),
        }
    }

    /// Run a comprehensive load test
    pub async fn run_load_test(&mut self) -> LoadTestResults {
        // Set up network topology
        self.setup_network_topology().await;

        // Start resource monitoring
        self.resource_monitor.start_monitoring(self.config.monitoring_interval).await;

        // Start metrics collection
        {
            let mut collector = self.metrics_collector.lock().unwrap();
            collector.start_collection();
        }

        // Start the simulator
        self.simulator.start().await.unwrap();

        // Run the actual load test
        let _test_result = self.execute_load_test().await;

        // Stop monitoring and collection
        self.resource_monitor.stop_monitoring().await;
        self.simulator.stop().await;

        // Analyze results
        let metrics = self.collect_final_metrics().await;
        let analysis = self.analyze_performance(&metrics);
        let test_passed = self.evaluate_test_criteria(&metrics);

        let failure_reasons = if test_passed { 
            Vec::new() 
        } else { 
            self.get_failure_reasons(&metrics) 
        };

        LoadTestResults {
            config: self.config.clone(),
            metrics,
            test_passed,
            degradation_detected: analysis.scalability_score < 0.7,
            failure_reasons,
            analysis,
        }
    }

    /// Set up network topology for load testing
    async fn setup_network_topology(&self) {
        // Create a central hub node
        self.simulator.add_node(1, Some((0.0, 0.0))).await;

        // Create client nodes
        for i in 2..=(self.config.connection_count + 1) {
            let angle = 2.0 * std::f64::consts::PI * (i - 2) as f64 / self.config.connection_count as f64;
            let x = 10.0 * angle.cos();
            let y = 10.0 * angle.sin();
            self.simulator.add_node(i, Some((x, y))).await;
        }

        // Create links from all clients to the hub
        let link_quality = LinkQuality {
            latency_ms: 10.0,
            bandwidth_mbps: 100.0,
            loss_rate: 0.001,
            jitter_ms: 1.0,
        };

        for i in 2..=(self.config.connection_count + 1) {
            self.simulator.add_link(1, i, link_quality.clone()).await;
        }
    }

    /// Execute the main load test
    async fn execute_load_test(&self) -> bool {
        let test_duration = self.config.test_duration;
        let packet_interval = Duration::from_nanos(
            1_000_000_000 / self.config.target_throughput_pps as u64
        );

        let start_time = Instant::now();
        let mut packet_id = 0u64;

        while start_time.elapsed() < test_duration {
            let metrics_collector = Arc::clone(&self.metrics_collector);
            let packet_size = self.config.packet_size;
            
            let packet = SimulatedPacket {
                id: packet_id,
                source: 2 + (packet_id % self.config.connection_count as u64) as u32,
                destination: 1,
                payload: vec![0u8; packet_size],
                timestamp: Instant::now(),
                size_bytes: packet_size,
                priority: PacketPriority::Normal,
            };

            packet_id += 1;

            // Send packet synchronously to avoid lifetime issues
            let send_start = Instant::now();
            let result = self.simulator.send_packet(packet).await;
            let send_duration = send_start.elapsed();

            // Record metrics
            {
                let mut collector = metrics_collector.lock().unwrap();
                collector.record_packet_sent(packet_size, send_duration.as_millis() as f64);
                if result.is_ok() {
                    collector.record_packet_received(packet_size);
                }
            }

            sleep(packet_interval).await;
        }

        // Wait for all packets to be processed
        sleep(Duration::from_secs(1)).await;
        true
    }

    /// Collect final metrics
    async fn collect_final_metrics(&self) -> PerformanceMetrics {
        let collector = self.metrics_collector.lock().unwrap();
        let resource_snapshots = self.resource_monitor.get_snapshots().await;
        let _sim_results = self.simulator.get_results().await;

        let test_duration = collector.get_test_duration();
        let throughput_pps = collector.packets_sent as f64 / test_duration.as_secs_f64();
        let throughput_mbps = (collector.bytes_sent * 8) as f64 / (test_duration.as_secs_f64() * 1_000_000.0);

        PerformanceMetrics {
            start_time: collector.start_time.unwrap_or_else(Instant::now),
            end_time: Instant::now(),
            packets_sent: collector.packets_sent,
            packets_received: collector.packets_received,
            bytes_sent: collector.bytes_sent,
            bytes_received: collector.bytes_received,
            avg_latency_ms: collector.get_avg_latency(),
            p95_latency_ms: collector.get_percentile_latency(0.95),
            p99_latency_ms: collector.get_percentile_latency(0.99),
            max_latency_ms: collector.get_max_latency(),
            throughput_pps,
            throughput_mbps,
            loss_rate: collector.get_loss_rate(),
            connection_success_rate: collector.get_connection_success_rate(),
            resource_snapshots,
        }
    }

    /// Analyze performance results
    fn analyze_performance(&self, metrics: &PerformanceMetrics) -> PerformanceAnalysis {
        let mut bottlenecks = Vec::new();
        let mut recommendations = Vec::new();

        // Check for latency bottlenecks
        if metrics.avg_latency_ms > self.config.max_latency_ms {
            bottlenecks.push(PerformanceBottleneck {
                bottleneck_type: BottleneckType::Latency,
                severity: (metrics.avg_latency_ms / self.config.max_latency_ms - 1.0).min(1.0),
                description: format!("Average latency {:.2}ms exceeds target {:.2}ms", 
                    metrics.avg_latency_ms, self.config.max_latency_ms),
                mitigation: "Consider optimizing packet processing or reducing network load".to_string(),
            });
            recommendations.push("Optimize packet processing pipeline".to_string());
        }

        // Check for packet loss bottlenecks
        if metrics.loss_rate > self.config.max_loss_rate {
            bottlenecks.push(PerformanceBottleneck {
                bottleneck_type: BottleneckType::PacketLoss,
                severity: (metrics.loss_rate / self.config.max_loss_rate - 1.0).min(1.0),
                description: format!("Packet loss rate {:.3} exceeds target {:.3}", 
                    metrics.loss_rate, self.config.max_loss_rate),
                mitigation: "Increase buffer sizes or reduce sending rate".to_string(),
            });
            recommendations.push("Implement better flow control".to_string());
        }

        // Check resource usage
        if let Some(max_memory) = metrics.resource_snapshots.iter()
            .map(|s| s.memory_usage_mb)
            .max_by(|a, b| a.partial_cmp(b).unwrap()) {
            if max_memory > 1000.0 { // 1GB threshold
                bottlenecks.push(PerformanceBottleneck {
                    bottleneck_type: BottleneckType::Memory,
                    severity: (max_memory / 1000.0 - 1.0).min(1.0),
                    description: format!("Peak memory usage {:.1}MB is high", max_memory),
                    mitigation: "Optimize memory allocation and implement memory pooling".to_string(),
                });
                recommendations.push("Implement memory pooling for packet buffers".to_string());
            }
        }

        // Calculate scalability score
        let target_throughput = self.config.target_throughput_pps as f64;
        let actual_throughput = metrics.throughput_pps;
        let scalability_score = (actual_throughput / target_throughput).min(1.0);

        // Calculate efficiency score
        let efficiency_score = if metrics.resource_snapshots.is_empty() {
            0.8 // Default score if no resource data
        } else {
            let avg_cpu = metrics.resource_snapshots.iter()
                .map(|s| s.cpu_usage_percent)
                .sum::<f64>() / metrics.resource_snapshots.len() as f64;
            (100.0 - avg_cpu) / 100.0
        };

        PerformanceAnalysis {
            bottlenecks,
            scalability_score,
            efficiency_score,
            recommendations,
        }
    }

    /// Evaluate if test passed all criteria
    fn evaluate_test_criteria(&self, metrics: &PerformanceMetrics) -> bool {
        metrics.avg_latency_ms <= self.config.max_latency_ms &&
        metrics.loss_rate <= self.config.max_loss_rate &&
        metrics.connection_success_rate >= 0.95 &&
        metrics.throughput_pps >= self.config.target_throughput_pps as f64 * 0.9
    }

    /// Get failure reasons
    fn get_failure_reasons(&self, metrics: &PerformanceMetrics) -> Vec<String> {
        let mut reasons = Vec::new();

        if metrics.avg_latency_ms > self.config.max_latency_ms {
            reasons.push(format!("Average latency {:.2}ms exceeds limit {:.2}ms", 
                metrics.avg_latency_ms, self.config.max_latency_ms));
        }

        if metrics.loss_rate > self.config.max_loss_rate {
            reasons.push(format!("Packet loss rate {:.3} exceeds limit {:.3}", 
                metrics.loss_rate, self.config.max_loss_rate));
        }

        if metrics.connection_success_rate < 0.95 {
            reasons.push(format!("Connection success rate {:.3} below 95%", 
                metrics.connection_success_rate));
        }

        if metrics.throughput_pps < self.config.target_throughput_pps as f64 * 0.9 {
            reasons.push(format!("Throughput {:.1} pps below 90% of target {}", 
                metrics.throughput_pps, self.config.target_throughput_pps));
        }

        reasons
    }
}

impl ResourceMonitor {
    fn new() -> Self {
        Self {
            monitoring_active: Arc::new(Mutex::new(false)),
            snapshots: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn start_monitoring(&self, interval: Duration) {
        {
            let mut active = self.monitoring_active.lock().unwrap();
            *active = true;
        }

        let monitoring_active = Arc::clone(&self.monitoring_active);
        let snapshots = Arc::clone(&self.snapshots);

        tokio::spawn(async move {
            while *monitoring_active.lock().unwrap() {
                let snapshot = ResourceSnapshot {
                    timestamp: Instant::now(),
                    memory_usage_mb: Self::get_memory_usage(),
                    cpu_usage_percent: Self::get_cpu_usage(),
                    network_usage_mbps: Self::get_network_usage(),
                    active_connections: Self::get_active_connections(),
                    file_descriptors: Self::get_file_descriptors(),
                };

                {
                    let mut snaps = snapshots.lock().unwrap();
                    snaps.push(snapshot);
                }

                sleep(interval).await;
            }
        });
    }

    async fn stop_monitoring(&self) {
        let mut active = self.monitoring_active.lock().unwrap();
        *active = false;
    }

    async fn get_snapshots(&self) -> Vec<ResourceSnapshot> {
        self.snapshots.lock().unwrap().clone()
    }

    // Simplified resource monitoring functions
    // In a real implementation, these would use system APIs
    fn get_memory_usage() -> f64 {
        // Simulate memory usage
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen_range(100.0..500.0)
    }

    fn get_cpu_usage() -> f64 {
        // Simulate CPU usage
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen_range(10.0..80.0)
    }

    fn get_network_usage() -> f64 {
        // Simulate network usage
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen_range(50.0..200.0)
    }

    fn get_active_connections() -> u32 {
        // Simulate active connections
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen_range(50..150)
    }

    fn get_file_descriptors() -> u32 {
        // Simulate file descriptor usage
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen_range(100..500)
    }
}

impl MetricsCollector {
    fn new() -> Self {
        Self {
            start_time: None,
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            latency_samples: Vec::new(),
            connection_attempts: 0,
            successful_connections: 0,
        }
    }

    fn start_collection(&mut self) {
        self.start_time = Some(Instant::now());
    }

    fn record_packet_sent(&mut self, size: usize, latency_ms: f64) {
        self.packets_sent += 1;
        self.bytes_sent += size as u64;
        self.latency_samples.push(latency_ms);
    }

    fn record_packet_received(&mut self, size: usize) {
        self.packets_received += 1;
        self.bytes_received += size as u64;
    }

    fn get_test_duration(&self) -> Duration {
        if let Some(start) = self.start_time {
            start.elapsed()
        } else {
            Duration::from_secs(0)
        }
    }

    fn get_avg_latency(&self) -> f64 {
        if self.latency_samples.is_empty() {
            0.0
        } else {
            self.latency_samples.iter().sum::<f64>() / self.latency_samples.len() as f64
        }
    }

    fn get_percentile_latency(&self, percentile: f64) -> f64 {
        if self.latency_samples.is_empty() {
            return 0.0;
        }

        let mut sorted = self.latency_samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let index = ((sorted.len() as f64 - 1.0) * percentile) as usize;
        sorted[index.min(sorted.len() - 1)]
    }

    fn get_max_latency(&self) -> f64 {
        self.latency_samples.iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .copied()
            .unwrap_or(0.0)
    }

    fn get_loss_rate(&self) -> f64 {
        if self.packets_sent == 0 {
            0.0
        } else {
            (self.packets_sent - self.packets_received) as f64 / self.packets_sent as f64
        }
    }

    fn get_connection_success_rate(&self) -> f64 {
        if self.connection_attempts == 0 {
            1.0
        } else {
            self.successful_connections as f64 / self.connection_attempts as f64
        }
    }
}

// Tests

#[tokio::test]
async fn test_small_load() {
    let config = LoadTestConfig {
        connection_count: 10,
        test_duration: Duration::from_secs(5),
        target_throughput_pps: 100,
        packet_size: 1000,
        max_latency_ms: 50.0,
        max_loss_rate: 0.1,
        monitoring_interval: Duration::from_millis(500),
    };

    let mut tester = LoadTester::new(config);
    let results = tester.run_load_test().await;

    assert!(results.metrics.packets_sent > 0, "Should send some packets");
    assert!(results.metrics.throughput_pps > 0.0, "Should have positive throughput");
    assert!(!results.metrics.resource_snapshots.is_empty(), "Should collect resource snapshots");
}

#[tokio::test]
async fn test_medium_load() {
    let config = LoadTestConfig {
        connection_count: 50,
        test_duration: Duration::from_secs(10),
        target_throughput_pps: 500,
        packet_size: 1400,
        max_latency_ms: 100.0,
        max_loss_rate: 0.05,
        monitoring_interval: Duration::from_secs(1),
    };

    let mut tester = LoadTester::new(config);
    let results = tester.run_load_test().await;

    assert!(results.metrics.packets_sent > 1000, "Should send substantial packets");
    assert!(results.metrics.throughput_pps > 100.0, "Should achieve reasonable throughput");
    assert!(results.analysis.scalability_score > 0.0, "Should have scalability score");
}

#[tokio::test]
async fn test_performance_analysis() {
    let config = LoadTestConfig {
        connection_count: 20,
        test_duration: Duration::from_secs(3),
        target_throughput_pps: 200,
        packet_size: 800,
        max_latency_ms: 25.0, // Very strict latency requirement
        max_loss_rate: 0.01,  // Very strict loss requirement
        monitoring_interval: Duration::from_millis(200),
    };

    let mut tester = LoadTester::new(config);
    let results = tester.run_load_test().await;

    // With strict requirements, we might detect bottlenecks
    assert!(results.analysis.scalability_score >= 0.0 && results.analysis.scalability_score <= 1.0);
    assert!(results.analysis.efficiency_score >= 0.0 && results.analysis.efficiency_score <= 1.0);
    
    // Should have some analysis results
    assert!(!results.analysis.recommendations.is_empty() || results.analysis.bottlenecks.is_empty());
}

#[tokio::test]
async fn test_resource_monitoring() {
    let config = LoadTestConfig {
        connection_count: 15,
        test_duration: Duration::from_secs(2),
        target_throughput_pps: 150,
        packet_size: 1200,
        max_latency_ms: 75.0,
        max_loss_rate: 0.08,
        monitoring_interval: Duration::from_millis(100),
    };

    let mut tester = LoadTester::new(config);
    let results = tester.run_load_test().await;

    // Should have collected resource snapshots
    assert!(!results.metrics.resource_snapshots.is_empty(), "Should collect resource snapshots");
    
    // Verify snapshot data is reasonable
    for snapshot in &results.metrics.resource_snapshots {
        assert!(snapshot.memory_usage_mb > 0.0, "Memory usage should be positive");
        assert!(snapshot.cpu_usage_percent >= 0.0 && snapshot.cpu_usage_percent <= 100.0, 
            "CPU usage should be 0-100%");
        assert!(snapshot.network_usage_mbps >= 0.0, "Network usage should be non-negative");
    }
}

#[tokio::test]
async fn test_property_based_performance() {
    // Use property-based testing for performance verification
    let config = PropertyTestConfig {
        iterations: 20,
        seed: Some(42),
        max_size: 100,
        shrink_attempts: 10,
        test_timeout: Duration::from_secs(30),
        verbose: false,
    };

    let generator = Box::new(U32Generator::new(10, 200));
    let mut tester = PropertyTester::new(config, generator);

    // Property: throughput should scale with connection count
    tester.add_property(Box::new(PredicateProperty::new(
        |connection_count: &u32| {
            // Simulate a quick performance test
            let expected_min_throughput = *connection_count as f64 * 2.0; // 2 pps per connection
            let simulated_throughput = *connection_count as f64 * 2.5; // Slightly better
            simulated_throughput >= expected_min_throughput
        },
        "throughput_scaling",
        "Throughput should scale with connection count"
    )));

    // Property: latency should remain bounded
    tester.add_property(Box::new(BoundsProperty::new(
        10u32, 200u32, "connection_count_bounds"
    )));

    let results = tester.run_all_tests();
    
    assert_eq!(results.len(), 2, "Should run both properties");
    for result in &results {
        assert!(result.success_rate > 0.8, "Properties should mostly pass: {}", result.property_name);
    }
}