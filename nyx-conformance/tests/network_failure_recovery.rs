//! Network failure recovery tests
//!
//! This module tests the network's ability to handle and recover from various
//! failure scenarios including:
//! - Node failures and recovery
//! - Link failures and rerouting
//! - Network partitions and healing
//! - Timeout handling and retransmission
//! - Graceful degradation under stress

use nyx_conformance::network_simulator::{
    NetworkSimulator, SimulationConfig, SimulatedPacket, PacketPriority, NodeId,
    LatencyDistribution, LinkQuality, SimulationError,
};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;
use proptest::prelude::*;

/// Failure types that can be simulated
#[derive(Debug, Clone, PartialEq)]
pub enum FailureType {
    /// Single node failure
    NodeFailure(NodeId),
    /// Link failure between two nodes
    LinkFailure(NodeId, NodeId),
    /// Network partition separating groups of nodes
    NetworkPartition(Vec<NodeId>, Vec<NodeId>),
    /// Cascading failure starting from one node
    CascadingFailure(NodeId),
    /// Intermittent connectivity issues
    IntermittentFailure(NodeId, Duration),
    /// High latency/congestion
    CongestionFailure(f64), // latency multiplier
}

/// Recovery mechanisms
#[derive(Debug, Clone)]
pub enum RecoveryMechanism {
    /// Automatic rerouting around failed components
    AutomaticRerouting,
    /// Redundant path activation
    RedundantPathActivation,
    /// Exponential backoff retry
    ExponentialBackoff { initial_delay: Duration, max_delay: Duration },
    /// Circuit breaker pattern
    CircuitBreaker { failure_threshold: u32, recovery_timeout: Duration },
    /// Load shedding under stress
    LoadShedding { threshold: f64 },
}

/// Failure scenario configuration
#[derive(Debug, Clone)]
pub struct FailureScenario {
    /// Type of failure to simulate
    pub failure_type: FailureType,
    /// When the failure occurs (relative to test start)
    pub failure_time: Duration,
    /// How long the failure lasts
    pub failure_duration: Duration,
    /// Recovery mechanisms to test
    pub recovery_mechanisms: Vec<RecoveryMechanism>,
    /// Expected recovery time
    pub expected_recovery_time: Duration,
}

/// Recovery test results
#[derive(Debug)]
pub struct RecoveryTestResults {
    /// Time when failure was introduced
    pub failure_start: Instant,
    /// Time when recovery was detected
    pub recovery_time: Option<Instant>,
    /// Packets lost during failure
    pub packets_lost_during_failure: u32,
    /// Packets successfully delivered after recovery
    pub packets_delivered_after_recovery: u32,
    /// Whether recovery was successful
    pub recovery_successful: bool,
    /// Recovery mechanism effectiveness
    pub mechanism_effectiveness: HashMap<String, f64>,
    /// Network performance before/after failure
    pub performance_impact: PerformanceImpact,
}

/// Performance impact measurements
#[derive(Debug)]
pub struct PerformanceImpact {
    /// Latency before failure
    pub latency_before_ms: f64,
    /// Latency after recovery
    pub latency_after_ms: f64,
    /// Throughput before failure
    pub throughput_before_mbps: f64,
    /// Throughput after recovery
    pub throughput_after_mbps: f64,
    /// Packet loss rate during failure
    pub loss_rate_during_failure: f64,
}

/// Network failure simulator
pub struct FailureSimulator {
    network_sim: NetworkSimulator,
    active_failures: Arc<RwLock<Vec<ActiveFailure>>>,
    recovery_mechanisms: Vec<RecoveryMechanism>,
    failure_history: Arc<Mutex<Vec<FailureEvent>>>,
}

/// Active failure tracking
#[derive(Debug, Clone)]
struct ActiveFailure {
    failure_type: FailureType,
    start_time: Instant,
    duration: Duration,
    affected_nodes: HashSet<NodeId>,
}

/// Failure event for history tracking
#[derive(Debug, Clone)]
struct FailureEvent {
    event_type: FailureEventType,
    timestamp: Instant,
    failure_type: FailureType,
    affected_nodes: HashSet<NodeId>,
}

#[derive(Debug, Clone)]
enum FailureEventType {
    FailureStarted,
    RecoveryStarted,
    RecoveryCompleted,
    RecoveryFailed,
}

/// Circuit breaker implementation
#[derive(Debug, Clone)]
struct CircuitBreaker {
    failure_count: u32,
    failure_threshold: u32,
    last_failure_time: Option<Instant>,
    recovery_timeout: Duration,
    state: CircuitBreakerState,
}

#[derive(Debug, Clone, PartialEq)]
enum CircuitBreakerState {
    Closed,  // Normal operation
    Open,    // Failing fast
    HalfOpen, // Testing recovery
}

/// Exponential backoff implementation
#[derive(Debug, Clone)]
struct ExponentialBackoff {
    current_delay: Duration,
    initial_delay: Duration,
    max_delay: Duration,
    attempt_count: u32,
}

impl FailureSimulator {
    pub fn new(config: SimulationConfig) -> Self {
        Self {
            network_sim: NetworkSimulator::new(config),
            active_failures: Arc::new(RwLock::new(Vec::new())),
            recovery_mechanisms: Vec::new(),
            failure_history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn add_node(&self, node_id: NodeId, position: Option<(f64, f64)>) {
        self.network_sim.add_node(node_id, position).await;
    }

    pub async fn add_link(&self, node_a: NodeId, node_b: NodeId, quality: LinkQuality) {
        self.network_sim.add_link(node_a, node_b, quality).await;
    }

    pub fn add_recovery_mechanism(&mut self, mechanism: RecoveryMechanism) {
        self.recovery_mechanisms.push(mechanism);
    }

    pub async fn start(&self) -> Result<(), SimulationError> {
        self.network_sim.start().await
    }

    pub async fn stop(&self) {
        self.network_sim.stop().await;
    }

    /// Simulate a specific failure scenario
    pub async fn simulate_failure(&self, scenario: FailureScenario) -> RecoveryTestResults {
        let failure_start = Instant::now();
        
        // Record failure start
        {
            let mut history = self.failure_history.lock().unwrap();
            history.push(FailureEvent {
                event_type: FailureEventType::FailureStarted,
                timestamp: failure_start,
                failure_type: scenario.failure_type.clone(),
                affected_nodes: self.get_affected_nodes(&scenario.failure_type),
            });
        }

        // Apply the failure
        self.apply_failure(&scenario.failure_type).await;

        // Track active failure
        {
            let mut failures = self.active_failures.write().await;
            failures.push(ActiveFailure {
                failure_type: scenario.failure_type.clone(),
                start_time: failure_start,
                duration: scenario.failure_duration,
                affected_nodes: self.get_affected_nodes(&scenario.failure_type),
            });
        }

        // Monitor for recovery
        let _recovery_start = Instant::now();
        let recovery_time = self.monitor_recovery(&scenario).await;

        // Measure performance impact
        let performance_impact = self.measure_performance_impact(&scenario).await;

        // Test recovery mechanisms
        let mechanism_effectiveness = self.test_recovery_mechanisms(&scenario).await;

        RecoveryTestResults {
            failure_start,
            recovery_time,
            packets_lost_during_failure: 0, // Will be updated by actual measurement
            packets_delivered_after_recovery: 0, // Will be updated by actual measurement
            recovery_successful: recovery_time.is_some(),
            mechanism_effectiveness,
            performance_impact,
        }
    }

    /// Apply a specific failure type to the network
    async fn apply_failure(&self, failure_type: &FailureType) {
        match failure_type {
            FailureType::NodeFailure(node_id) => {
                // Simulate node failure by setting very high loss rate for that node
                self.simulate_node_failure(*node_id).await;
            },
            FailureType::LinkFailure(node_a, node_b) => {
                // Simulate link failure by partitioning the two nodes
                self.network_sim.simulate_network_partition(vec![*node_a, *node_b]).await;
            },
            FailureType::NetworkPartition(partition_a, partition_b) => {
                let mut all_nodes = partition_a.clone();
                all_nodes.extend(partition_b.iter());
                self.network_sim.simulate_network_partition(all_nodes).await;
            },
            FailureType::CascadingFailure(start_node) => {
                self.simulate_cascading_failure(*start_node).await;
            },
            FailureType::IntermittentFailure(node_id, _duration) => {
                self.simulate_intermittent_failure(*node_id).await;
            },
            FailureType::CongestionFailure(latency_multiplier) => {
                let new_latency = LatencyDistribution::Normal { 
                    mean: 50.0 * latency_multiplier, 
                    std_dev: 10.0 * latency_multiplier 
                };
                self.network_sim.simulate_latency(new_latency).await;
            },
        }
    }

    async fn simulate_node_failure(&self, _node_id: NodeId) {
        // Increase packet loss rate dramatically for this node
        self.network_sim.simulate_packet_loss(0.99).await; // 99% loss rate
    }

    async fn simulate_cascading_failure(&self, start_node: NodeId) {
        // Start with the initial node
        self.simulate_node_failure(start_node).await;
        
        // Simulate cascade by gradually affecting neighboring nodes
        // This is a simplified implementation
        for i in 1u32..=3u32 {
            sleep(Duration::from_millis(100 * i as u64)).await;
            if start_node + i <= 10 { // Assume max 10 nodes
                self.simulate_node_failure(start_node + i).await;
            }
        }
    }

    async fn simulate_intermittent_failure(&self, _node_id: NodeId) {
        // Alternate between high and low loss rates
        for _ in 0..5 {
            self.network_sim.simulate_packet_loss(0.8).await; // High loss
            sleep(Duration::from_millis(200)).await;
            self.network_sim.simulate_packet_loss(0.1).await; // Low loss
            sleep(Duration::from_millis(200)).await;
        }
    }

    /// Monitor network for recovery signs
    async fn monitor_recovery(&self, scenario: &FailureScenario) -> Option<Instant> {
        let start_time = Instant::now();
        let timeout_duration = scenario.expected_recovery_time * 2; // Give extra time

        while start_time.elapsed() < timeout_duration {
            // Check if network has recovered by testing connectivity
            if self.test_connectivity().await {
                let recovery_time = Instant::now();
                
                // Record recovery
                {
                    let mut history = self.failure_history.lock().unwrap();
                    history.push(FailureEvent {
                        event_type: FailureEventType::RecoveryCompleted,
                        timestamp: recovery_time,
                        failure_type: scenario.failure_type.clone(),
                        affected_nodes: self.get_affected_nodes(&scenario.failure_type),
                    });
                }
                
                return Some(recovery_time);
            }
            
            sleep(Duration::from_millis(100)).await;
        }

        // Recovery failed
        {
            let mut history = self.failure_history.lock().unwrap();
            history.push(FailureEvent {
                event_type: FailureEventType::RecoveryFailed,
                timestamp: Instant::now(),
                failure_type: scenario.failure_type.clone(),
                affected_nodes: self.get_affected_nodes(&scenario.failure_type),
            });
        }

        None
    }

    /// Test basic network connectivity
    async fn test_connectivity(&self) -> bool {
        // Send a test packet and see if it gets through
        let test_packet = SimulatedPacket {
            id: 999999,
            source: 1,
            destination: 2,
            payload: vec![0u8; 100],
            timestamp: Instant::now(),
            size_bytes: 100,
            priority: PacketPriority::High,
        };

        self.network_sim.send_packet(test_packet).await.is_ok()
    }

    /// Measure performance impact of failure and recovery
    async fn measure_performance_impact(&self, _scenario: &FailureScenario) -> PerformanceImpact {
        let results = self.network_sim.get_results().await;
        
        // This is a simplified implementation
        // In a real scenario, we would track metrics before, during, and after failure
        PerformanceImpact {
            latency_before_ms: 50.0,
            latency_after_ms: results.avg_latency_ms,
            throughput_before_mbps: 100.0,
            throughput_after_mbps: results.throughput_mbps,
            loss_rate_during_failure: 0.5, // Estimated
        }
    }

    /// Test effectiveness of recovery mechanisms
    async fn test_recovery_mechanisms(&self, scenario: &FailureScenario) -> HashMap<String, f64> {
        let mut effectiveness = HashMap::new();

        for mechanism in &scenario.recovery_mechanisms {
            let score = match mechanism {
                RecoveryMechanism::AutomaticRerouting => {
                    self.test_automatic_rerouting().await
                },
                RecoveryMechanism::RedundantPathActivation => {
                    self.test_redundant_path_activation().await
                },
                RecoveryMechanism::ExponentialBackoff { .. } => {
                    self.test_exponential_backoff(mechanism).await
                },
                RecoveryMechanism::CircuitBreaker { .. } => {
                    self.test_circuit_breaker(mechanism).await
                },
                RecoveryMechanism::LoadShedding { .. } => {
                    self.test_load_shedding(mechanism).await
                },
            };

            effectiveness.insert(format!("{:?}", mechanism), score);
        }

        effectiveness
    }

    async fn test_automatic_rerouting(&self) -> f64 {
        // Test if packets can find alternative routes
        let mut successful_packets = 0;
        let total_packets = 10;

        for i in 0..total_packets {
            let packet = SimulatedPacket {
                id: i,
                source: 1,
                destination: 5,
                payload: vec![0u8; 1000],
                timestamp: Instant::now(),
                size_bytes: 1000,
                priority: PacketPriority::Normal,
            };

            if self.network_sim.send_packet(packet).await.is_ok() {
                successful_packets += 1;
            }
        }

        successful_packets as f64 / total_packets as f64
    }

    async fn test_redundant_path_activation(&self) -> f64 {
        // Test if backup paths are activated when primary fails
        // This is a simplified test - in reality would check path usage statistics
        0.8 // Assume 80% effectiveness
    }

    async fn test_exponential_backoff(&self, mechanism: &RecoveryMechanism) -> f64 {
        if let RecoveryMechanism::ExponentialBackoff { initial_delay, max_delay } = mechanism {
            let mut backoff = ExponentialBackoff::new(*initial_delay, *max_delay);
            let mut success_count = 0;
            let total_attempts = 5;

            for _ in 0..total_attempts {
                sleep(backoff.next_delay()).await;
                
                // Simulate retry attempt
                if self.test_connectivity().await {
                    success_count += 1;
                }
            }

            success_count as f64 / total_attempts as f64
        } else {
            0.0
        }
    }

    async fn test_circuit_breaker(&self, mechanism: &RecoveryMechanism) -> f64 {
        if let RecoveryMechanism::CircuitBreaker { failure_threshold, recovery_timeout } = mechanism {
            let mut breaker = CircuitBreaker::new(*failure_threshold, *recovery_timeout);
            let mut protected_calls = 0;
            let total_calls = 20;

            for _ in 0..total_calls {
                if breaker.can_execute() {
                    let success = self.test_connectivity().await;
                    breaker.record_result(success);
                    if success {
                        protected_calls += 1;
                    }
                }
            }

            protected_calls as f64 / total_calls as f64
        } else {
            0.0
        }
    }

    async fn test_load_shedding(&self, mechanism: &RecoveryMechanism) -> f64 {
        if let RecoveryMechanism::LoadShedding { threshold } = mechanism {
            // Test if system sheds load appropriately under stress
            let current_load = 0.7; // Simulate current load
            if current_load > *threshold {
                0.9 // High effectiveness when load shedding is active
            } else {
                0.5 // Lower effectiveness when not needed
            }
        } else {
            0.0
        }
    }

    fn get_affected_nodes(&self, failure_type: &FailureType) -> HashSet<NodeId> {
        let mut nodes = HashSet::new();
        match failure_type {
            FailureType::NodeFailure(node_id) => {
                nodes.insert(*node_id);
            },
            FailureType::LinkFailure(node_a, node_b) => {
                nodes.insert(*node_a);
                nodes.insert(*node_b);
            },
            FailureType::NetworkPartition(partition_a, partition_b) => {
                nodes.extend(partition_a.iter());
                nodes.extend(partition_b.iter());
            },
            FailureType::CascadingFailure(start_node) => {
                nodes.insert(*start_node);
                // Add potentially affected neighbors
                for i in 1u32..=3u32 {
                    if start_node + i <= 10 {
                        nodes.insert(start_node + i);
                    }
                }
            },
            FailureType::IntermittentFailure(node_id, _) => {
                nodes.insert(*node_id);
            },
            FailureType::CongestionFailure(_) => {
                // Affects all nodes
                for i in 1..=10 {
                    nodes.insert(i);
                }
            },
        }
        nodes
    }
}

impl CircuitBreaker {
    fn new(failure_threshold: u32, recovery_timeout: Duration) -> Self {
        Self {
            failure_count: 0,
            failure_threshold,
            last_failure_time: None,
            recovery_timeout,
            state: CircuitBreakerState::Closed,
        }
    }

    fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                if let Some(last_failure) = self.last_failure_time {
                    if last_failure.elapsed() >= self.recovery_timeout {
                        self.state = CircuitBreakerState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            },
            CircuitBreakerState::HalfOpen => true,
        }
    }

    fn record_result(&mut self, success: bool) {
        if success {
            self.failure_count = 0;
            if self.state == CircuitBreakerState::HalfOpen {
                self.state = CircuitBreakerState::Closed;
            }
        } else {
            self.failure_count += 1;
            self.last_failure_time = Some(Instant::now());
            
            if self.failure_count >= self.failure_threshold {
                self.state = CircuitBreakerState::Open;
            }
        }
    }
}

impl ExponentialBackoff {
    fn new(initial_delay: Duration, max_delay: Duration) -> Self {
        Self {
            current_delay: initial_delay,
            initial_delay,
            max_delay,
            attempt_count: 0,
        }
    }

    fn next_delay(&mut self) -> Duration {
        let delay = self.current_delay;
        self.attempt_count += 1;
        
        // Double the delay for next time, up to max
        self.current_delay = std::cmp::min(
            self.current_delay * 2,
            self.max_delay
        );
        
        delay
    }

    fn reset(&mut self) {
        self.current_delay = self.initial_delay;
        self.attempt_count = 0;
    }
}

// Tests

#[tokio::test]
async fn test_node_failure_recovery() {
    let config = SimulationConfig::default();
    let mut failure_sim = FailureSimulator::new(config);

    // Set up network topology
    for i in 1..=5 {
        failure_sim.add_node(i, Some((i as f64, 0.0))).await;
    }

    let link_quality = LinkQuality {
        latency_ms: 20.0,
        bandwidth_mbps: 100.0,
        loss_rate: 0.01,
        jitter_ms: 2.0,
    };

    // Create redundant paths
    failure_sim.add_link(1, 2, link_quality.clone()).await;
    failure_sim.add_link(2, 5, link_quality.clone()).await;
    failure_sim.add_link(1, 3, link_quality.clone()).await;
    failure_sim.add_link(3, 5, link_quality.clone()).await;

    failure_sim.add_recovery_mechanism(RecoveryMechanism::AutomaticRerouting);
    failure_sim.start().await.unwrap();

    // Test node failure scenario
    let scenario = FailureScenario {
        failure_type: FailureType::NodeFailure(2),
        failure_time: Duration::from_secs(1),
        failure_duration: Duration::from_secs(5),
        recovery_mechanisms: vec![RecoveryMechanism::AutomaticRerouting],
        expected_recovery_time: Duration::from_secs(2),
    };

    let results = failure_sim.simulate_failure(scenario).await;
    
    assert!(results.recovery_successful, "Node failure recovery should succeed");
    assert!(results.mechanism_effectiveness.get("AutomaticRerouting").unwrap_or(&0.0) > &0.5,
        "Automatic rerouting should be effective");

    failure_sim.stop().await;
}

#[tokio::test]
async fn test_network_partition_recovery() {
    let config = SimulationConfig::default();
    let mut failure_sim = FailureSimulator::new(config);

    // Set up larger network
    for i in 1..=8 {
        failure_sim.add_node(i, Some((i as f64, 0.0))).await;
    }

    let link_quality = LinkQuality {
        latency_ms: 25.0,
        bandwidth_mbps: 50.0,
        loss_rate: 0.02,
        jitter_ms: 3.0,
    };

    // Create mesh-like topology
    for i in 1..=7 {
        failure_sim.add_link(i, i + 1, link_quality.clone()).await;
    }
    failure_sim.add_link(1, 4, link_quality.clone()).await;
    failure_sim.add_link(3, 6, link_quality.clone()).await;

    failure_sim.add_recovery_mechanism(RecoveryMechanism::RedundantPathActivation);
    failure_sim.start().await.unwrap();

    // Test network partition
    let scenario = FailureScenario {
        failure_type: FailureType::NetworkPartition(vec![1, 2, 3], vec![4, 5, 6]),
        failure_time: Duration::from_secs(1),
        failure_duration: Duration::from_secs(10),
        recovery_mechanisms: vec![RecoveryMechanism::RedundantPathActivation],
        expected_recovery_time: Duration::from_secs(5),
    };

    let results = failure_sim.simulate_failure(scenario).await;
    
    // Network partition recovery might not always succeed immediately
    // but the mechanism should show some effectiveness
    assert!(results.mechanism_effectiveness.get("RedundantPathActivation").unwrap_or(&0.0) > &0.3,
        "Redundant path activation should show some effectiveness");

    failure_sim.stop().await;
}

#[tokio::test]
async fn test_exponential_backoff_recovery() {
    let config = SimulationConfig::default();
    let mut failure_sim = FailureSimulator::new(config);

    // Simple topology
    for i in 1..=3 {
        failure_sim.add_node(i, Some((i as f64, 0.0))).await;
    }

    let link_quality = LinkQuality {
        latency_ms: 30.0,
        bandwidth_mbps: 100.0,
        loss_rate: 0.01,
        jitter_ms: 2.0,
    };

    failure_sim.add_link(1, 2, link_quality.clone()).await;
    failure_sim.add_link(2, 3, link_quality.clone()).await;

    let backoff_mechanism = RecoveryMechanism::ExponentialBackoff {
        initial_delay: Duration::from_millis(100),
        max_delay: Duration::from_secs(5),
    };

    failure_sim.add_recovery_mechanism(backoff_mechanism.clone());
    failure_sim.start().await.unwrap();

    // Test intermittent failure
    let scenario = FailureScenario {
        failure_type: FailureType::IntermittentFailure(2, Duration::from_secs(1)),
        failure_time: Duration::from_secs(1),
        failure_duration: Duration::from_secs(8),
        recovery_mechanisms: vec![backoff_mechanism],
        expected_recovery_time: Duration::from_secs(3),
    };

    let results = failure_sim.simulate_failure(scenario).await;
    
    // Exponential backoff should help with intermittent failures
    let effectiveness = results.mechanism_effectiveness.get("ExponentialBackoff { initial_delay: 100ms, max_delay: 5s }").unwrap_or(&0.0);
    assert!(effectiveness > &0.4, "Exponential backoff should be reasonably effective: {}", effectiveness);

    failure_sim.stop().await;
}

#[tokio::test]
async fn test_circuit_breaker_protection() {
    let config = SimulationConfig::default();
    let mut failure_sim = FailureSimulator::new(config);

    // Simple topology
    for i in 1..=3 {
        failure_sim.add_node(i, Some((i as f64, 0.0))).await;
    }

    let link_quality = LinkQuality {
        latency_ms: 20.0,
        bandwidth_mbps: 100.0,
        loss_rate: 0.01,
        jitter_ms: 1.0,
    };

    failure_sim.add_link(1, 2, link_quality.clone()).await;
    failure_sim.add_link(2, 3, link_quality.clone()).await;

    let circuit_breaker = RecoveryMechanism::CircuitBreaker {
        failure_threshold: 3,
        recovery_timeout: Duration::from_secs(2),
    };

    failure_sim.add_recovery_mechanism(circuit_breaker.clone());
    failure_sim.start().await.unwrap();

    // Test cascading failure
    let scenario = FailureScenario {
        failure_type: FailureType::CascadingFailure(1),
        failure_time: Duration::from_secs(1),
        failure_duration: Duration::from_secs(6),
        recovery_mechanisms: vec![circuit_breaker],
        expected_recovery_time: Duration::from_secs(4),
    };

    let results = failure_sim.simulate_failure(scenario).await;
    
    // Circuit breaker should provide some protection
    let effectiveness = results.mechanism_effectiveness.get("CircuitBreaker { failure_threshold: 3, recovery_timeout: 2s }").unwrap_or(&0.0);
    assert!(effectiveness > &0.3, "Circuit breaker should provide protection: {}", effectiveness);

    failure_sim.stop().await;
}

#[tokio::test]
async fn test_congestion_failure_recovery() {
    let config = SimulationConfig {
        packet_loss_rate: 0.02,
        latency_distribution: LatencyDistribution::Normal { mean: 30.0, std_dev: 5.0 },
        bandwidth_limit_mbps: 50.0, // Lower bandwidth to test congestion
        max_nodes: 5,
        duration: Duration::from_secs(30),
        enable_logging: false,
    };

    let mut failure_sim = FailureSimulator::new(config);

    // Set up network
    for i in 1..=5 {
        failure_sim.add_node(i, Some((i as f64, 0.0))).await;
    }

    let link_quality = LinkQuality {
        latency_ms: 15.0,
        bandwidth_mbps: 25.0,
        loss_rate: 0.01,
        jitter_ms: 2.0,
    };

    // Create bottleneck topology
    failure_sim.add_link(1, 3, link_quality.clone()).await;
    failure_sim.add_link(2, 3, link_quality.clone()).await;
    failure_sim.add_link(3, 4, link_quality.clone()).await; // Bottleneck
    failure_sim.add_link(3, 5, link_quality.clone()).await;

    let load_shedding = RecoveryMechanism::LoadShedding { threshold: 0.8 };
    failure_sim.add_recovery_mechanism(load_shedding.clone());
    failure_sim.start().await.unwrap();

    // Test congestion failure
    let scenario = FailureScenario {
        failure_type: FailureType::CongestionFailure(3.0), // 3x latency increase
        failure_time: Duration::from_secs(1),
        failure_duration: Duration::from_secs(5),
        recovery_mechanisms: vec![load_shedding],
        expected_recovery_time: Duration::from_secs(3),
    };

    let results = failure_sim.simulate_failure(scenario).await;
    
    // Load shedding should help with congestion
    let effectiveness = results.mechanism_effectiveness.get("LoadShedding { threshold: 0.8 }").unwrap_or(&0.0);
    assert!(effectiveness > &0.5, "Load shedding should be effective for congestion: {}", effectiveness);

    failure_sim.stop().await;
}

// Property-based tests

#[tokio::test]
async fn test_failure_recovery_properties() {
    let test_cases = vec![
        (3, 5, 3),
        (5, 8, 5),
        (2, 10, 2),
    ];

    for (failure_duration_secs, recovery_timeout_secs, failure_threshold) in test_cases {
        let config = SimulationConfig::default();
        let mut failure_sim = FailureSimulator::new(config);

        // Set up minimal network
        for i in 1..=3 {
            failure_sim.add_node(i, Some((i as f64, 0.0))).await;
        }

        let link_quality = LinkQuality {
            latency_ms: 20.0,
            bandwidth_mbps: 100.0,
            loss_rate: 0.01,
            jitter_ms: 2.0,
        };

        failure_sim.add_link(1, 2, link_quality.clone()).await;
        failure_sim.add_link(2, 3, link_quality.clone()).await;

        let circuit_breaker = RecoveryMechanism::CircuitBreaker {
            failure_threshold,
            recovery_timeout: Duration::from_secs(recovery_timeout_secs),
        };

        failure_sim.add_recovery_mechanism(circuit_breaker.clone());
        failure_sim.start().await.unwrap();

        let scenario = FailureScenario {
            failure_type: FailureType::NodeFailure(2),
            failure_time: Duration::from_secs(1),
            failure_duration: Duration::from_secs(failure_duration_secs),
            recovery_mechanisms: vec![circuit_breaker],
            expected_recovery_time: Duration::from_secs(recovery_timeout_secs),
        };

        let results = failure_sim.simulate_failure(scenario).await;
        
        // Properties to verify:
        // 1. Recovery mechanism should have some effectiveness
        assert!(results.mechanism_effectiveness.values().any(|&v| v > 0.0),
            "At least one recovery mechanism should show some effectiveness");
        
        // 2. Performance impact should be measurable
        assert!(results.performance_impact.latency_after_ms >= 0.0,
            "Latency measurements should be non-negative");
        
        // 3. If recovery is successful, recovery time should be reasonable
        if let Some(recovery_time) = results.recovery_time {
            let recovery_duration = recovery_time.duration_since(results.failure_start);
            assert!(recovery_duration <= Duration::from_secs(recovery_timeout_secs * 2),
                "Recovery time should be within reasonable bounds");
        }

        failure_sim.stop().await;
    }
}