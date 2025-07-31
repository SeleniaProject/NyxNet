//! Comprehensive multipath routing property tests
//!
//! This module tests the correctness of multipath routing algorithms including:
//! - Packet delivery verification across multiple paths
//! - Path selection algorithm correctness
//! - Load balancing verification
//! - Path failure handling
//! - Route convergence properties

use nyx_conformance::network_simulator::{
    NetworkSimulator, SimulationConfig, SimulatedPacket, PacketPriority,
    LatencyDistribution, LinkQuality,
};
use nyx_stream::{MultipathReceiver, Sequencer};

/// Path identifier type
pub type PathId = u8;
use proptest::prelude::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::test;

/// Test data structure for multipath routing verification
#[derive(Debug, Clone)]
struct MultipathTestScenario {
    /// Number of nodes in the network
    node_count: u32,
    /// Number of paths to establish
    path_count: u32,
    /// Number of packets to send
    packet_count: u32,
    /// Packet loss rate per path
    loss_rate: f64,
    /// Network latency characteristics
    latency_distribution: LatencyDistribution,
}

/// Path selection algorithm implementation for testing
#[derive(Debug, Clone)]
struct PathSelector {
    /// Available paths with their quality metrics
    paths: HashMap<PathId, PathQuality>,
    /// Current load balancing state
    load_state: HashMap<PathId, LoadState>,
    /// Selection strategy
    strategy: SelectionStrategy,
}

/// Path quality metrics
#[derive(Debug, Clone)]
struct PathQuality {
    latency_ms: f64,
    bandwidth_mbps: f64,
    loss_rate: f64,
    reliability_score: f64,
    congestion_level: f64,
}

/// Load balancing state per path
#[derive(Debug, Clone)]
struct LoadState {
    packets_sent: u64,
    bytes_sent: u64,
    last_used: Instant,
    current_load: f64,
}

/// Path selection strategies
#[derive(Debug, Clone)]
enum SelectionStrategy {
    /// Round-robin selection
    RoundRobin,
    /// Weighted round-robin based on quality
    WeightedRoundRobin,
    /// Least loaded path first
    LeastLoaded,
    /// Best quality path first
    BestQuality,
    /// Adaptive selection based on current conditions
    Adaptive,
}

/// Multipath routing test results
#[derive(Debug)]
struct RoutingTestResults {
    /// Total packets sent
    packets_sent: u32,
    /// Packets successfully delivered
    packets_delivered: u32,
    /// Packets lost during transmission
    packets_lost: u32,
    /// Average delivery latency
    avg_latency_ms: f64,
    /// Path utilization distribution
    path_utilization: HashMap<PathId, f64>,
    /// Load balancing effectiveness score
    load_balance_score: f64,
    /// Path failure recovery time
    recovery_time_ms: Option<f64>,
}

impl PathSelector {
    fn new(strategy: SelectionStrategy) -> Self {
        Self {
            paths: HashMap::new(),
            load_state: HashMap::new(),
            strategy,
        }
    }

    fn add_path(&mut self, path_id: PathId, quality: PathQuality) {
        self.paths.insert(path_id, quality);
        self.load_state.insert(path_id, LoadState {
            packets_sent: 0,
            bytes_sent: 0,
            last_used: Instant::now(),
            current_load: 0.0,
        });
    }

    fn select_path(&mut self, packet_size: usize) -> Option<PathId> {
        if self.paths.is_empty() {
            return None;
        }

        let selected = match self.strategy {
            SelectionStrategy::RoundRobin => self.select_round_robin(),
            SelectionStrategy::WeightedRoundRobin => self.select_weighted_round_robin(),
            SelectionStrategy::LeastLoaded => self.select_least_loaded(),
            SelectionStrategy::BestQuality => self.select_best_quality(),
            SelectionStrategy::Adaptive => self.select_adaptive(packet_size),
        };

        if let Some(path_id) = selected {
            self.update_load_state(path_id, packet_size);
        }

        selected
    }

    fn select_round_robin(&self) -> Option<PathId> {
        // Simple round-robin implementation
        let mut paths: Vec<_> = self.paths.keys().collect();
        paths.sort();
        
        if let Some(least_used) = paths.iter()
            .min_by_key(|&&path_id| self.load_state[&path_id].packets_sent) {
            Some(**least_used)
        } else {
            None
        }
    }

    fn select_weighted_round_robin(&self) -> Option<PathId> {
        // Weight paths by inverse of latency and loss rate
        let mut best_path = None;
        let mut best_score = f64::NEG_INFINITY;

        for (&path_id, quality) in &self.paths {
            let load = &self.load_state[&path_id];
            let weight = 1.0 / (quality.latency_ms + 1.0) * (1.0 - quality.loss_rate);
            let load_factor = 1.0 / (load.current_load + 1.0);
            let score = weight * load_factor;

            if score > best_score {
                best_score = score;
                best_path = Some(path_id);
            }
        }

        best_path
    }

    fn select_least_loaded(&self) -> Option<PathId> {
        self.load_state.iter()
            .min_by(|(_, a), (_, b)| a.current_load.partial_cmp(&b.current_load).unwrap())
            .map(|(&path_id, _)| path_id)
    }

    fn select_best_quality(&self) -> Option<PathId> {
        self.paths.iter()
            .max_by(|(_, a), (_, b)| a.reliability_score.partial_cmp(&b.reliability_score).unwrap())
            .map(|(&path_id, _)| path_id)
    }

    fn select_adaptive(&self, _packet_size: usize) -> Option<PathId> {
        // Adaptive selection considering current network conditions
        let mut best_path = None;
        let mut best_score = f64::NEG_INFINITY;

        for (&path_id, quality) in &self.paths {
            let load = &self.load_state[&path_id];
            
            // Calculate adaptive score based on multiple factors
            let latency_score = 1.0 / (quality.latency_ms + 1.0);
            let bandwidth_score = quality.bandwidth_mbps / 1000.0; // Normalize
            let reliability_score = quality.reliability_score;
            let load_score = 1.0 / (load.current_load + 1.0);
            let congestion_score = 1.0 - quality.congestion_level;
            
            let composite_score = latency_score * 0.3 + 
                                bandwidth_score * 0.2 + 
                                reliability_score * 0.2 + 
                                load_score * 0.2 + 
                                congestion_score * 0.1;

            if composite_score > best_score {
                best_score = composite_score;
                best_path = Some(path_id);
            }
        }

        best_path
    }

    fn update_load_state(&mut self, path_id: PathId, packet_size: usize) {
        if let Some(load) = self.load_state.get_mut(&path_id) {
            load.packets_sent += 1;
            load.bytes_sent += packet_size as u64;
            load.last_used = Instant::now();
            
            // Update current load (exponential moving average)
            let new_load = packet_size as f64 / 1500.0; // Normalize by MTU
            load.current_load = load.current_load * 0.9 + new_load * 0.1;
        }
    }

    fn remove_path(&mut self, path_id: PathId) {
        self.paths.remove(&path_id);
        self.load_state.remove(&path_id);
    }

    fn update_path_quality(&mut self, path_id: PathId, quality: PathQuality) {
        self.paths.insert(path_id, quality);
    }
}

/// Test packet delivery across multiple paths
#[test]
async fn test_multipath_packet_delivery() {
    let config = SimulationConfig {
        packet_loss_rate: 0.05, // 5% loss
        latency_distribution: LatencyDistribution::Normal { mean: 50.0, std_dev: 10.0 },
        bandwidth_limit_mbps: 100.0,
        max_nodes: 10,
        duration: Duration::from_secs(60),
        enable_logging: false,
    };

    let simulator = NetworkSimulator::new(config);
    
    // Set up network topology with multiple paths
    for i in 1..=5 {
        simulator.add_node(i, Some((i as f64, 0.0))).await;
    }

    // Create multiple paths between nodes 1 and 5
    let link_quality = LinkQuality {
        latency_ms: 20.0,
        bandwidth_mbps: 50.0,
        loss_rate: 0.02,
        jitter_ms: 5.0,
    };

    // Path 1: 1 -> 2 -> 5
    simulator.add_link(1, 2, link_quality.clone()).await;
    simulator.add_link(2, 5, link_quality.clone()).await;

    // Path 2: 1 -> 3 -> 5
    simulator.add_link(1, 3, link_quality.clone()).await;
    simulator.add_link(3, 5, link_quality.clone()).await;

    // Path 3: 1 -> 4 -> 5
    simulator.add_link(1, 4, link_quality.clone()).await;
    simulator.add_link(4, 5, link_quality.clone()).await;

    simulator.start().await.unwrap();

    // Test packet delivery across all paths
    let mut packets_sent = 0;
    for i in 0..100 {
        let packet = SimulatedPacket {
            id: i,
            source: 1,
            destination: 5,
            payload: vec![0u8; 1000],
            timestamp: Instant::now(),
            size_bytes: 1000,
            priority: PacketPriority::Normal,
        };

        if simulator.send_packet(packet).await.is_ok() {
            packets_sent += 1;
        }
    }

    // Allow time for packet processing
    tokio::time::sleep(Duration::from_secs(2)).await;

    let results = simulator.get_results().await;
    
    // Verify packet delivery
    assert!(results.packets_received > 0, "No packets were delivered");
    assert!(results.packets_received <= packets_sent, "More packets received than sent");
    
    // Verify reasonable delivery rate (accounting for loss)
    let delivery_rate = results.packets_received as f64 / packets_sent as f64;
    assert!(delivery_rate > 0.8, "Delivery rate too low: {}", delivery_rate);

    simulator.stop().await;
}

/// Test path selection algorithm correctness
#[tokio::test]
async fn test_path_selection_algorithms() {
    let mut selector = PathSelector::new(SelectionStrategy::WeightedRoundRobin);

    // Add paths with different qualities
    selector.add_path(1, PathQuality {
        latency_ms: 10.0,
        bandwidth_mbps: 100.0,
        loss_rate: 0.01,
        reliability_score: 0.95,
        congestion_level: 0.1,
    });

    selector.add_path(2, PathQuality {
        latency_ms: 50.0,
        bandwidth_mbps: 50.0,
        loss_rate: 0.05,
        reliability_score: 0.85,
        congestion_level: 0.3,
    });

    selector.add_path(3, PathQuality {
        latency_ms: 100.0,
        bandwidth_mbps: 25.0,
        loss_rate: 0.1,
        reliability_score: 0.75,
        congestion_level: 0.5,
    });

    // Test path selection
    let mut path_usage = HashMap::new();
    for _ in 0..1000 {
        if let Some(path) = selector.select_path(1000) {
            *path_usage.entry(path).or_insert(0) += 1;
        }
    }

    // Verify that better quality paths are used more frequently
    assert!(path_usage[&1] > path_usage[&2], "High quality path should be used more");
    assert!(path_usage[&2] > path_usage[&3], "Medium quality path should be used more than low quality");
}

/// Test load balancing across multiple paths
#[test]
async fn test_multipath_load_balancing() {
    let config = SimulationConfig::default();
    let simulator = NetworkSimulator::new(config);
    
    // Set up network with multiple equal-quality paths
    for i in 1..=6 {
        simulator.add_node(i, Some((i as f64, 0.0))).await;
    }

    let link_quality = LinkQuality {
        latency_ms: 25.0,
        bandwidth_mbps: 100.0,
        loss_rate: 0.01,
        jitter_ms: 2.0,
    };

    // Create 3 equal paths: 1->2->6, 1->3->6, 1->4->6
    for intermediate in 2..=4 {
        simulator.add_link(1, intermediate, link_quality.clone()).await;
        simulator.add_link(intermediate, 6, link_quality.clone()).await;
    }

    simulator.start().await.unwrap();

    // Send packets and track path usage
    let mut path_selector = PathSelector::new(SelectionStrategy::RoundRobin);
    
    for path_id in 2..=4 {
        path_selector.add_path(path_id as u8, PathQuality {
            latency_ms: 25.0,
            bandwidth_mbps: 100.0,
            loss_rate: 0.01,
            reliability_score: 0.9,
            congestion_level: 0.1,
        });
    }

    let mut path_usage = HashMap::new();
    for i in 0..300 {
        if let Some(path_id) = path_selector.select_path(1000) {
            *path_usage.entry(path_id).or_insert(0) += 1;
            
            let packet = SimulatedPacket {
                id: i,
                source: 1,
                destination: 6,
                payload: vec![0u8; 1000],
                timestamp: Instant::now(),
                size_bytes: 1000,
                priority: PacketPriority::Normal,
            };
            
            let _ = simulator.send_packet(packet).await;
        }
    }

    // Verify load balancing - each path should be used roughly equally
    let total_packets: u32 = path_usage.values().sum();
    let expected_per_path = total_packets / 3;
    let tolerance = expected_per_path / 4; // 25% tolerance

    for (&path_id, &usage) in &path_usage {
        let diff = (usage as i32 - expected_per_path as i32).abs() as u32;
        assert!(diff <= tolerance, 
            "Path {} usage {} differs too much from expected {} (tolerance: {})", 
            path_id, usage, expected_per_path, tolerance);
    }

    simulator.stop().await;
}

/// Test path failure and recovery handling
#[test]
async fn test_path_failure_recovery() {
    let config = SimulationConfig::default();
    let simulator = NetworkSimulator::new(config);
    
    // Set up network topology
    for i in 1..=4 {
        simulator.add_node(i, Some((i as f64, 0.0))).await;
    }

    let link_quality = LinkQuality {
        latency_ms: 30.0,
        bandwidth_mbps: 100.0,
        loss_rate: 0.01,
        jitter_ms: 3.0,
    };

    // Create two paths: 1->2->4 and 1->3->4
    simulator.add_link(1, 2, link_quality.clone()).await;
    simulator.add_link(2, 4, link_quality.clone()).await;
    simulator.add_link(1, 3, link_quality.clone()).await;
    simulator.add_link(3, 4, link_quality.clone()).await;

    simulator.start().await.unwrap();

    let mut path_selector = PathSelector::new(SelectionStrategy::Adaptive);
    
    // Add both paths
    path_selector.add_path(2, PathQuality {
        latency_ms: 30.0,
        bandwidth_mbps: 100.0,
        loss_rate: 0.01,
        reliability_score: 0.9,
        congestion_level: 0.1,
    });

    path_selector.add_path(3, PathQuality {
        latency_ms: 30.0,
        bandwidth_mbps: 100.0,
        loss_rate: 0.01,
        reliability_score: 0.9,
        congestion_level: 0.1,
    });

    // Send packets normally
    let mut successful_sends = 0;
    for i in 0..50 {
        if let Some(_path_id) = path_selector.select_path(1000) {
            let packet = SimulatedPacket {
                id: i,
                source: 1,
                destination: 4,
                payload: vec![0u8; 1000],
                timestamp: Instant::now(),
                size_bytes: 1000,
                priority: PacketPriority::Normal,
            };
            
            if simulator.send_packet(packet).await.is_ok() {
                successful_sends += 1;
            }
        }
    }

    // Simulate path failure by creating network partition
    simulator.simulate_network_partition(vec![1, 2, 3, 4]).await;

    // Remove failed path
    path_selector.remove_path(2);

    // Continue sending packets on remaining path
    for i in 50..100 {
        if let Some(_path_id) = path_selector.select_path(1000) {
            let packet = SimulatedPacket {
                id: i,
                source: 1,
                destination: 4,
                payload: vec![0u8; 1000],
                timestamp: Instant::now(),
                size_bytes: 1000,
                priority: PacketPriority::Normal,
            };
            
            if simulator.send_packet(packet).await.is_ok() {
                successful_sends += 1;
            }
        }
    }

    // Verify that packets continued to be sent after path failure
    assert!(successful_sends > 50, "Should continue sending after path failure");

    simulator.stop().await;
}

/// Property-based test for multipath routing correctness
#[tokio::test]
async fn test_multipath_routing_correctness() {
    let test_cases = vec![
        (5, 3, 50, 0.05),
        (7, 4, 100, 0.1),
        (4, 2, 30, 0.02),
    ];

    for (node_count, path_count, packet_count, loss_rate) in test_cases {
        let config = SimulationConfig {
            packet_loss_rate: loss_rate,
            latency_distribution: LatencyDistribution::Normal { mean: 50.0, std_dev: 10.0 },
            bandwidth_limit_mbps: 100.0,
            max_nodes: node_count,
            duration: Duration::from_secs(30),
            enable_logging: false,
        };

        let simulator = NetworkSimulator::new(config);
        
        // Set up nodes
        for i in 1..=node_count {
            simulator.add_node(i, Some((i as f64, 0.0))).await;
        }

        // Create paths
        let link_quality = LinkQuality {
            latency_ms: 25.0,
            bandwidth_mbps: 50.0,
            loss_rate: loss_rate / 2.0, // Link loss rate lower than overall
            jitter_ms: 5.0,
        };

        // Connect nodes in a way that creates multiple paths
        for i in 2..=std::cmp::min(path_count + 1, node_count - 1) {
            simulator.add_link(1, i, link_quality.clone()).await;
            simulator.add_link(i, node_count, link_quality.clone()).await;
        }

        simulator.start().await.unwrap();

        // Send packets
        let mut packets_sent = 0;
        for i in 0..packet_count {
            let packet = SimulatedPacket {
                id: i as u64,
                source: 1,
                destination: node_count,
                payload: vec![0u8; 1000],
                timestamp: Instant::now(),
                size_bytes: 1000,
                priority: PacketPriority::Normal,
            };

            if simulator.send_packet(packet).await.is_ok() {
                packets_sent += 1;
            }
        }

        // Allow processing time
        tokio::time::sleep(Duration::from_millis(100)).await;

        let results = simulator.get_results().await;
        
        // Properties to verify:
        // 1. Some packets should be delivered (unless loss rate is very high)
        if loss_rate < 0.15 {
            assert!(results.packets_received > 0, "No packets delivered with reasonable loss rate");
        }
        
        // 2. Cannot receive more packets than sent
        assert!(results.packets_received <= packets_sent as u64, 
            "Received more packets than sent: {} > {}", results.packets_received, packets_sent);
        
        // 3. Loss rate should be reasonable
        if packets_sent > 0 {
            let actual_loss_rate = results.packets_lost as f64 / packets_sent as f64;
            assert!(actual_loss_rate <= loss_rate * 2.0, 
                "Actual loss rate {} exceeds expected {}", actual_loss_rate, loss_rate * 2.0);
        }

        simulator.stop().await;
    }
}

/// Test multipath receiver ordering properties
#[tokio::test]
async fn test_multipath_receiver_ordering() {
    let mut receiver = MultipathReceiver::new();
    
    // Test in-order delivery per path
    let result1 = receiver.push(1, 0, vec![0]);
    assert_eq!(result1, vec![vec![0]]);
    
    // Out of order packet should be buffered
    let result2 = receiver.push(1, 2, vec![2]);
    assert!(result2.is_empty());
    
    // Fill the gap
    let result3 = receiver.push(1, 1, vec![1]);
    assert_eq!(result3, vec![vec![1], vec![2]]);
    
    // Test independent paths
    let result4 = receiver.push(2, 0, vec![10]);
    assert_eq!(result4, vec![vec![10]]);
    
    // Verify path independence
    let result5 = receiver.push(1, 3, vec![3]);
    assert_eq!(result5, vec![vec![3]]);
}

/// Test sequencer independence across paths
#[tokio::test]
async fn test_sequencer_path_independence() {
    let mut sequencer = Sequencer::new();
    
    // Each path should have independent sequence numbers
    assert_eq!(sequencer.next(10), 0);
    assert_eq!(sequencer.next(20), 0);
    assert_eq!(sequencer.next(10), 1);
    assert_eq!(sequencer.next(30), 0);
    assert_eq!(sequencer.next(20), 1);
    assert_eq!(sequencer.next(10), 2);
}

#[cfg(test)]
mod async_tests {
    use super::*;

    async fn run_multipath_stress_test(
        node_count: u32,
        path_count: u32,
        packet_count: u32,
        duration: Duration,
    ) -> RoutingTestResults {
        let config = SimulationConfig {
            packet_loss_rate: 0.02,
            latency_distribution: LatencyDistribution::Normal { mean: 30.0, std_dev: 5.0 },
            bandwidth_limit_mbps: 200.0,
            max_nodes: node_count,
            duration,
            enable_logging: false,
        };

        let simulator = NetworkSimulator::new(config);
        
        // Set up topology
        for i in 1..=node_count {
            simulator.add_node(i, Some((i as f64, 0.0))).await;
        }

        let link_quality = LinkQuality {
            latency_ms: 15.0,
            bandwidth_mbps: 100.0,
            loss_rate: 0.01,
            jitter_ms: 2.0,
        };

        // Create multiple paths
        for i in 2..=std::cmp::min(path_count + 1, node_count - 1) {
            simulator.add_link(1, i, link_quality.clone()).await;
            simulator.add_link(i, node_count, link_quality.clone()).await;
        }

        simulator.start().await.unwrap();

        let start_time = Instant::now();
        let mut packets_sent = 0;
        let mut path_usage = HashMap::new();

        // Send packets with load balancing
        let mut selector = PathSelector::new(SelectionStrategy::Adaptive);
        for path_id in 2..=std::cmp::min(path_count + 1, node_count - 1) {
            selector.add_path(path_id as u8, PathQuality {
                latency_ms: 15.0,
                bandwidth_mbps: 100.0,
                loss_rate: 0.01,
                reliability_score: 0.9,
                congestion_level: 0.1,
            });
        }

        for i in 0..packet_count {
            if let Some(path_id) = selector.select_path(1000) {
                *path_usage.entry(path_id).or_insert(0) += 1;
                
                let packet = SimulatedPacket {
                    id: i as u64,
                    source: 1,
                    destination: node_count,
                    payload: vec![0u8; 1000],
                    timestamp: Instant::now(),
                    size_bytes: 1000,
                    priority: PacketPriority::Normal,
                };

                if simulator.send_packet(packet).await.is_ok() {
                    packets_sent += 1;
                }
            }
        }

        // Allow processing time
        tokio::time::sleep(Duration::from_millis(500)).await;

        let results = simulator.get_results().await;
        let _test_duration = start_time.elapsed();

        // Calculate load balancing score
        let total_usage: u32 = path_usage.values().sum();
        let expected_per_path = total_usage as f64 / path_usage.len() as f64;
        let variance: f64 = path_usage.values()
            .map(|&usage| (usage as f64 - expected_per_path).powi(2))
            .sum::<f64>() / path_usage.len() as f64;
        let load_balance_score = 1.0 / (1.0 + variance / expected_per_path);

        simulator.stop().await;

        RoutingTestResults {
            packets_sent,
            packets_delivered: results.packets_received as u32,
            packets_lost: results.packets_lost as u32,
            avg_latency_ms: results.avg_latency_ms,
            path_utilization: path_usage.into_iter()
                .map(|(k, v)| (k, v as f64 / total_usage as f64))
                .collect(),
            load_balance_score,
            recovery_time_ms: None,
        }
    }

    #[tokio::test]
    async fn test_multipath_stress_small() {
        let results = run_multipath_stress_test(5, 3, 100, Duration::from_secs(10)).await;
        
        assert!(results.packets_delivered > 0, "No packets delivered");
        assert!(results.load_balance_score > 0.7, "Poor load balancing: {}", results.load_balance_score);
        
        let delivery_rate = results.packets_delivered as f64 / results.packets_sent as f64;
        assert!(delivery_rate > 0.9, "Low delivery rate: {}", delivery_rate);
    }

    #[tokio::test]
    async fn test_multipath_stress_medium() {
        let results = run_multipath_stress_test(8, 4, 500, Duration::from_secs(30)).await;
        
        assert!(results.packets_delivered > 0, "No packets delivered");
        assert!(results.load_balance_score > 0.6, "Poor load balancing: {}", results.load_balance_score);
        
        let delivery_rate = results.packets_delivered as f64 / results.packets_sent as f64;
        assert!(delivery_rate > 0.85, "Low delivery rate: {}", delivery_rate);
    }
}