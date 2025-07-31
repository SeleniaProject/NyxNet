//! Network Simulator for testing Nyx protocol under various network conditions
//!
//! This module provides comprehensive network simulation capabilities including:
//! - Packet loss simulation
//! - Latency modeling with various distributions
//! - Bandwidth constraints
//! - Network topology management
//! - Network partitioning scenarios

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;
use rand::{Rng, thread_rng};
use serde::{Deserialize, Serialize};

/// Unique identifier for network nodes
pub type NodeId = u32;

/// Unique identifier for network packets
pub type PacketId = u64;

/// Network simulation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    /// Base packet loss rate (0.0 to 1.0)
    pub packet_loss_rate: f64,
    /// Base latency distribution
    pub latency_distribution: LatencyDistribution,
    /// Bandwidth limit in Mbps
    pub bandwidth_limit_mbps: f64,
    /// Maximum number of nodes in simulation
    pub max_nodes: u32,
    /// Simulation duration
    pub duration: Duration,
    /// Enable detailed logging
    pub enable_logging: bool,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            packet_loss_rate: 0.01, // 1% loss
            latency_distribution: LatencyDistribution::Normal { mean: 50.0, std_dev: 10.0 },
            bandwidth_limit_mbps: 100.0,
            max_nodes: 1000,
            duration: Duration::from_secs(300), // 5 minutes
            enable_logging: false,
        }
    }
}

/// Latency distribution models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LatencyDistribution {
    /// Fixed latency
    Fixed { latency_ms: f64 },
    /// Normal distribution
    Normal { mean: f64, std_dev: f64 },
    /// Exponential distribution
    Exponential { lambda: f64 },
    /// Uniform distribution
    Uniform { min: f64, max: f64 },
}

/// Network topology representation
#[derive(Debug, Clone)]
pub struct NetworkTopology {
    /// Adjacency list representation
    pub adjacency: HashMap<NodeId, Vec<NodeId>>,
    /// Node positions for geographic simulation
    pub node_positions: HashMap<NodeId, (f64, f64)>,
    /// Link qualities between nodes
    pub link_qualities: HashMap<(NodeId, NodeId), LinkQuality>,
}

/// Link quality metrics
#[derive(Debug, Clone)]
pub struct LinkQuality {
    pub latency_ms: f64,
    pub bandwidth_mbps: f64,
    pub loss_rate: f64,
    pub jitter_ms: f64,
}

/// Network conditions that can be applied
#[derive(Debug, Clone)]
pub struct NetworkConditions {
    pub packet_loss_rate: f64,
    pub latency_distribution: LatencyDistribution,
    pub bandwidth_limit_mbps: f64,
    pub jitter_ms: f64,
    pub corruption_rate: f64,
}

/// Simulated network packet
#[derive(Debug, Clone)]
pub struct SimulatedPacket {
    pub id: PacketId,
    pub source: NodeId,
    pub destination: NodeId,
    pub payload: Vec<u8>,
    pub timestamp: Instant,
    pub size_bytes: usize,
    pub priority: PacketPriority,
}

/// Packet priority levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PacketPriority {
    Low,
    Normal,
    High,
    Critical,
}

/// Packet scheduling strategy
pub struct PacketScheduler {
    /// Queue for each priority level
    queues: HashMap<PacketPriority, VecDeque<SimulatedPacket>>,
    /// Bandwidth tracking
    bandwidth_tracker: BandwidthTracker,
    /// Last packet send time
    last_send_time: Instant,
}

/// Bandwidth usage tracking
struct BandwidthTracker {
    limit_mbps: f64,
    current_usage: f64,
    window_start: Instant,
    window_duration: Duration,
}

/// Network partition configuration
#[derive(Debug, Clone)]
pub struct NetworkPartition {
    /// Nodes in partition A
    pub partition_a: Vec<NodeId>,
    /// Nodes in partition B  
    pub partition_b: Vec<NodeId>,
    /// Duration of partition
    pub duration: Duration,
    /// Whether partition is complete or allows some communication
    pub complete_partition: bool,
}

/// Simulation results and statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResults {
    /// Total packets sent
    pub packets_sent: u64,
    /// Total packets received
    pub packets_received: u64,
    /// Total packets lost
    pub packets_lost: u64,
    /// Average latency
    pub avg_latency_ms: f64,
    /// Maximum latency observed
    pub max_latency_ms: f64,
    /// Minimum latency observed
    pub min_latency_ms: f64,
    /// Throughput in Mbps
    pub throughput_mbps: f64,
    /// Jitter measurements
    pub jitter_ms: f64,
    /// Per-node statistics
    pub node_stats: HashMap<NodeId, NodeStatistics>,
    /// Simulation duration
    pub duration: Duration,
}

/// Per-node statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatistics {
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub avg_latency_ms: f64,
    pub packet_loss_rate: f64,
}

/// Main network simulator
pub struct NetworkSimulator {
    config: SimulationConfig,
    topology: Arc<RwLock<NetworkTopology>>,
    conditions: Arc<RwLock<NetworkConditions>>,
    packet_scheduler: Arc<Mutex<PacketScheduler>>,
    active_partitions: Arc<RwLock<Vec<NetworkPartition>>>,
    results: Arc<Mutex<SimulationResults>>,
    packet_counter: Arc<Mutex<u64>>,
    running: Arc<RwLock<bool>>,
}

impl NetworkSimulator {
    /// Create a new network simulator with the given configuration
    pub fn new(config: SimulationConfig) -> Self {
        let topology = NetworkTopology {
            adjacency: HashMap::new(),
            node_positions: HashMap::new(),
            link_qualities: HashMap::new(),
        };

        let conditions = NetworkConditions {
            packet_loss_rate: config.packet_loss_rate,
            latency_distribution: config.latency_distribution.clone(),
            bandwidth_limit_mbps: config.bandwidth_limit_mbps,
            jitter_ms: 5.0,
            corruption_rate: 0.001,
        };

        let scheduler = PacketScheduler::new(config.bandwidth_limit_mbps);

        let results = SimulationResults {
            packets_sent: 0,
            packets_received: 0,
            packets_lost: 0,
            avg_latency_ms: 0.0,
            max_latency_ms: 0.0,
            min_latency_ms: f64::MAX,
            throughput_mbps: 0.0,
            jitter_ms: 0.0,
            node_stats: HashMap::new(),
            duration: Duration::from_secs(0),
        };

        Self {
            config,
            topology: Arc::new(RwLock::new(topology)),
            conditions: Arc::new(RwLock::new(conditions)),
            packet_scheduler: Arc::new(Mutex::new(scheduler)),
            active_partitions: Arc::new(RwLock::new(Vec::new())),
            results: Arc::new(Mutex::new(results)),
            packet_counter: Arc::new(Mutex::new(0)),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Add a node to the network topology
    pub async fn add_node(&self, node_id: NodeId, position: Option<(f64, f64)>) {
        let mut topology = self.topology.write().await;
        topology.adjacency.entry(node_id).or_insert_with(Vec::new);
        
        if let Some(pos) = position {
            topology.node_positions.insert(node_id, pos);
        }
    }

    /// Add a link between two nodes
    pub async fn add_link(&self, node_a: NodeId, node_b: NodeId, quality: LinkQuality) {
        let mut topology = self.topology.write().await;
        
        topology.adjacency.entry(node_a).or_default().push(node_b);
        topology.adjacency.entry(node_b).or_default().push(node_a);
        
        topology.link_qualities.insert((node_a, node_b), quality.clone());
        topology.link_qualities.insert((node_b, node_a), quality);
    }

    /// Simulate packet loss with the configured rate
    pub async fn simulate_packet_loss(&self, rate: f64) {
        let mut conditions = self.conditions.write().await;
        conditions.packet_loss_rate = rate.clamp(0.0, 1.0);
    }

    /// Simulate network latency with the given distribution
    pub async fn simulate_latency(&self, distribution: LatencyDistribution) {
        let mut conditions = self.conditions.write().await;
        conditions.latency_distribution = distribution;
    }

    /// Simulate bandwidth limitations
    pub async fn simulate_bandwidth_limit(&self, limit_mbps: f64) {
        let mut conditions = self.conditions.write().await;
        conditions.bandwidth_limit_mbps = limit_mbps.max(0.1); // Minimum 0.1 Mbps
        
        let mut scheduler = self.packet_scheduler.lock().unwrap();
        scheduler.bandwidth_tracker.limit_mbps = limit_mbps;
    }

    /// Simulate network partition
    pub async fn simulate_network_partition(&self, nodes: Vec<NodeId>) {
        if nodes.len() < 2 {
            return;
        }
        
        let mid = nodes.len() / 2;
        let partition = NetworkPartition {
            partition_a: nodes[..mid].to_vec(),
            partition_b: nodes[mid..].to_vec(),
            duration: Duration::from_secs(30), // Default 30 seconds
            complete_partition: true,
        };
        
        let mut partitions = self.active_partitions.write().await;
        partitions.push(partition);
    }

    /// Send a packet through the simulated network
    pub async fn send_packet(&self, packet: SimulatedPacket) -> Result<(), SimulationError> {
        if !*self.running.read().await {
            return Err(SimulationError::NotRunning);
        }

        // Check if packet should be dropped due to loss
        if self.should_drop_packet().await {
            self.record_packet_loss(&packet).await;
            return Ok(());
        }

        // Check network partitions
        if self.is_partitioned(packet.source, packet.destination).await {
            self.record_packet_loss(&packet).await;
            return Ok(());
        }

        // Calculate latency
        let latency = self.calculate_latency(packet.source, packet.destination).await;
        
        // Schedule packet delivery
        let mut scheduler = self.packet_scheduler.lock().unwrap();
        scheduler.schedule_packet(packet, latency)?;
        
        Ok(())
    }

    /// Start the network simulation
    pub async fn start(&self) -> Result<(), SimulationError> {
        let mut running = self.running.write().await;
        if *running {
            return Err(SimulationError::AlreadyRunning);
        }
        *running = true;
        
        // Start packet processing loop
        let scheduler = Arc::clone(&self.packet_scheduler);
        let results = Arc::clone(&self.results);
        let running_flag = Arc::clone(&self.running);
        
        tokio::spawn(async move {
            while *running_flag.read().await {
                let packet_opt = {
                    let mut sched = scheduler.lock().unwrap();
                    sched.get_next_packet()
                };
                
                if let Some(_packet) = packet_opt {
                    // Process delivered packet
                    let mut res = results.lock().unwrap();
                    res.packets_received += 1;
                    res.throughput_mbps = Self::calculate_throughput(&res);
                }
                
                sleep(Duration::from_millis(1)).await;
            }
        });
        
        Ok(())
    }

    /// Stop the network simulation
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
    }

    /// Get current simulation results
    pub async fn get_results(&self) -> SimulationResults {
        let results = self.results.lock().unwrap();
        results.clone()
    }

    /// Reset simulation state
    pub async fn reset(&self) {
        self.stop().await;
        
        let mut results = self.results.lock().unwrap();
        *results = SimulationResults {
            packets_sent: 0,
            packets_received: 0,
            packets_lost: 0,
            avg_latency_ms: 0.0,
            max_latency_ms: 0.0,
            min_latency_ms: f64::MAX,
            throughput_mbps: 0.0,
            jitter_ms: 0.0,
            node_stats: HashMap::new(),
            duration: Duration::from_secs(0),
        };
        
        let mut counter = self.packet_counter.lock().unwrap();
        *counter = 0;
        
        let mut partitions = self.active_partitions.write().await;
        partitions.clear();
    }

    // Private helper methods
    
    async fn should_drop_packet(&self) -> bool {
        let conditions = self.conditions.read().await;
        let mut rng = thread_rng();
        rng.gen::<f64>() < conditions.packet_loss_rate
    }

    async fn is_partitioned(&self, source: NodeId, destination: NodeId) -> bool {
        let partitions = self.active_partitions.read().await;
        
        for partition in partitions.iter() {
            let source_in_a = partition.partition_a.contains(&source);
            let source_in_b = partition.partition_b.contains(&source);
            let dest_in_a = partition.partition_a.contains(&destination);
            let dest_in_b = partition.partition_b.contains(&destination);
            
            if partition.complete_partition {
                if (source_in_a && dest_in_b) || (source_in_b && dest_in_a) {
                    return true;
                }
            }
        }
        
        false
    }

    async fn calculate_latency(&self, _source: NodeId, _destination: NodeId) -> Duration {
        let conditions = self.conditions.read().await;
        let mut rng = thread_rng();
        
        let latency_ms = match &conditions.latency_distribution {
            LatencyDistribution::Fixed { latency_ms } => *latency_ms,
            LatencyDistribution::Normal { mean, std_dev } => {
                use rand_distr::{Normal, Distribution};
                let normal = Normal::new(*mean, *std_dev).unwrap_or_else(|_| Normal::new(50.0, 10.0).unwrap());
                normal.sample(&mut rng).max(0.0)
            },
            LatencyDistribution::Exponential { lambda } => {
                use rand_distr::{Exp, Distribution};
                let exp = Exp::new(*lambda).unwrap_or_else(|_| Exp::new(0.02).unwrap());
                exp.sample(&mut rng)
            },
            LatencyDistribution::Uniform { min, max } => {
                rng.gen_range(*min..*max)
            },
        };
        
        Duration::from_millis(latency_ms as u64)
    }

    async fn record_packet_loss(&self, packet: &SimulatedPacket) {
        let mut results = self.results.lock().unwrap();
        results.packets_lost += 1;
        
        let packets_lost = results.packets_lost;
        let packets_sent = results.packets_sent;
        
        // Update node statistics
        let node_stats = results.node_stats.entry(packet.source).or_insert_with(|| NodeStatistics {
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            avg_latency_ms: 0.0,
            packet_loss_rate: 0.0,
        });
        
        node_stats.packets_sent += 1;
        node_stats.bytes_sent += packet.size_bytes as u64;
        node_stats.packet_loss_rate = packets_lost as f64 / (packets_sent + packets_lost) as f64;
    }

    fn calculate_throughput(results: &SimulationResults) -> f64 {
        if results.duration.as_secs() == 0 {
            return 0.0;
        }
        
        let total_bytes = results.node_stats.values()
            .map(|stats| stats.bytes_received)
            .sum::<u64>();
            
        let bits = total_bytes * 8;
        let seconds = results.duration.as_secs_f64();
        
        (bits as f64) / (seconds * 1_000_000.0) // Convert to Mbps
    }
}

impl PacketScheduler {
    fn new(bandwidth_limit_mbps: f64) -> Self {
        let mut queues = HashMap::new();
        queues.insert(PacketPriority::Critical, VecDeque::new());
        queues.insert(PacketPriority::High, VecDeque::new());
        queues.insert(PacketPriority::Normal, VecDeque::new());
        queues.insert(PacketPriority::Low, VecDeque::new());
        
        Self {
            queues,
            bandwidth_tracker: BandwidthTracker {
                limit_mbps: bandwidth_limit_mbps,
                current_usage: 0.0,
                window_start: Instant::now(),
                window_duration: Duration::from_secs(1),
            },
            last_send_time: Instant::now(),
        }
    }

    fn schedule_packet(&mut self, packet: SimulatedPacket, _latency: Duration) -> Result<(), SimulationError> {
        // Check bandwidth constraints
        if !self.bandwidth_tracker.can_send(packet.size_bytes) {
            return Err(SimulationError::BandwidthExceeded);
        }
        
        // Add to appropriate priority queue
        if let Some(queue) = self.queues.get_mut(&packet.priority) {
            queue.push_back(packet);
        }
        
        Ok(())
    }

    fn get_next_packet(&mut self) -> Option<SimulatedPacket> {
        // Process queues in priority order
        let priorities = [
            PacketPriority::Critical,
            PacketPriority::High,
            PacketPriority::Normal,
            PacketPriority::Low,
        ];
        
        for priority in &priorities {
            if let Some(queue) = self.queues.get_mut(priority) {
                if let Some(packet) = queue.pop_front() {
                    self.bandwidth_tracker.record_send(packet.size_bytes);
                    return Some(packet);
                }
            }
        }
        
        None
    }
}

impl BandwidthTracker {
    fn can_send(&mut self, bytes: usize) -> bool {
        self.update_window();
        
        let bits = bytes * 8;
        let mbits = bits as f64 / 1_000_000.0;
        
        self.current_usage + mbits <= self.limit_mbps
    }

    fn record_send(&mut self, bytes: usize) {
        let bits = bytes * 8;
        let mbits = bits as f64 / 1_000_000.0;
        self.current_usage += mbits;
    }

    fn update_window(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.window_start) >= self.window_duration {
            self.current_usage = 0.0;
            self.window_start = now;
        }
    }
}

/// Simulation errors
#[derive(Debug, thiserror::Error)]
pub enum SimulationError {
    #[error("Simulation is not running")]
    NotRunning,
    
    #[error("Simulation is already running")]
    AlreadyRunning,
    
    #[error("Bandwidth limit exceeded")]
    BandwidthExceeded,
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[test]
    async fn test_network_simulator_creation() {
        let config = SimulationConfig::default();
        let simulator = NetworkSimulator::new(config);
        
        // Test basic functionality
        simulator.add_node(1, Some((0.0, 0.0))).await;
        simulator.add_node(2, Some((1.0, 1.0))).await;
        
        let quality = LinkQuality {
            latency_ms: 10.0,
            bandwidth_mbps: 100.0,
            loss_rate: 0.01,
            jitter_ms: 2.0,
        };
        
        simulator.add_link(1, 2, quality).await;
        
        assert!(simulator.start().await.is_ok());
        simulator.stop().await;
    }

    #[test]
    async fn test_packet_loss_simulation() {
        let mut config = SimulationConfig::default();
        config.packet_loss_rate = 0.5; // 50% loss rate
        
        let simulator = NetworkSimulator::new(config);
        simulator.simulate_packet_loss(0.8).await; // Change to 80%
        
        // Verify the loss rate was updated
        let conditions = simulator.conditions.read().await;
        assert_eq!(conditions.packet_loss_rate, 0.8);
    }

    #[test]
    async fn test_latency_distributions() {
        let simulator = NetworkSimulator::new(SimulationConfig::default());
        
        // Test different latency distributions
        simulator.simulate_latency(LatencyDistribution::Fixed { latency_ms: 100.0 }).await;
        simulator.simulate_latency(LatencyDistribution::Normal { mean: 50.0, std_dev: 10.0 }).await;
        simulator.simulate_latency(LatencyDistribution::Uniform { min: 10.0, max: 100.0 }).await;
        
        // All should succeed without panicking
    }
}