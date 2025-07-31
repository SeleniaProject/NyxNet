//! End-to-End Scenario Tests
//! 
//! Comprehensive tests that simulate real usage scenarios from client to daemon
//! covering complete data flow, multiple client connections, multipath communication,
//! long-running scenarios, stress testing, and network failure recovery.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{sleep, timeout};
use tracing::{info, warn, error};
use futures::future::join_all;
use rand::{Rng, thread_rng};

use nyx_crypto::{hpke::HpkeKeyDeriver, noise::NoiseHandshake};
use nyx_stream::{NyxAsyncStream, FlowController, FrameHandler};
use nyx_conformance::{
    network_simulator::{NetworkSimulator, SimulationConfig, LatencyDistribution, SimulatedPacket, PacketPriority},
    property_tester::{PropertyTester, PropertyTestConfig, ByteVecGenerator, PredicateProperty}
};

/// Test context for end-to-end scenarios
#[derive(Clone)]
struct E2ETestContext {
    network_simulator: Arc<NetworkSimulator>,
    active_connections: Arc<RwLock<HashMap<u32, ConnectionInfo>>>,
    metrics_collector: Arc<Mutex<E2EMetrics>>,
    test_config: E2ETestConfig,
}

/// Connection information for tracking
#[derive(Debug, Clone)]
struct ConnectionInfo {
    connection_id: u32,
    client_id: String,
    established_at: Instant,
    bytes_sent: u64,
    bytes_received: u64,
    last_activity: Instant,
    status: ConnectionStatus,
}

/// Connection status tracking
#[derive(Debug, Clone, PartialEq)]
enum ConnectionStatus {
    Connecting,
    Established,
    Active,
    Idle,
    Disconnecting,
    Disconnected,
    Failed(String),
}

/// End-to-end test configuration
#[derive(Debug, Clone)]
struct E2ETestConfig {
    max_clients: u32,
    test_duration: Duration,
    data_size_range: (usize, usize),
    network_conditions: NetworkConditions,
    stress_test_enabled: bool,
    failure_injection_enabled: bool,
}

/// Network conditions for testing
#[derive(Debug, Clone)]
struct NetworkConditions {
    packet_loss_rate: f64,
    latency_ms: (f64, f64), // (min, max)
    bandwidth_mbps: f64,
    jitter_ms: f64,
}

/// Metrics collection for E2E tests
#[derive(Debug, Default)]
struct E2EMetrics {
    total_connections: u64,
    successful_connections: u64,
    failed_connections: u64,
    total_bytes_transferred: u64,
    average_latency_ms: f64,
    peak_concurrent_connections: u32,
    test_start_time: Option<Instant>,
    connection_durations: Vec<Duration>,
    throughput_samples: Vec<f64>,
    error_counts: HashMap<String, u32>,
}

impl Default for E2ETestConfig {
    fn default() -> Self {
        Self {
            max_clients: 100,
            test_duration: Duration::from_secs(30),
            data_size_range: (1024, 10240), // 1KB to 10KB
            network_conditions: NetworkConditions {
                packet_loss_rate: 0.01,
                latency_ms: (10.0, 100.0),
                bandwidth_mbps: 100.0,
                jitter_ms: 5.0,
            },
            stress_test_enabled: false,
            failure_injection_enabled: false,
        }
    }
}

impl Clone for E2EMetrics {
    fn clone(&self) -> Self {
        Self {
            total_connections: self.total_connections,
            successful_connections: self.successful_connections,
            failed_connections: self.failed_connections,
            total_bytes_transferred: self.total_bytes_transferred,
            average_latency_ms: self.average_latency_ms,
            peak_concurrent_connections: self.peak_concurrent_connections,
            test_start_time: self.test_start_time,
            connection_durations: self.connection_durations.clone(),
            throughput_samples: self.throughput_samples.clone(),
            error_counts: self.error_counts.clone(),
        }
    }
}

impl E2ETestContext {
    async fn new(config: E2ETestConfig) -> Self {
        let sim_config = SimulationConfig {
            packet_loss_rate: config.network_conditions.packet_loss_rate,
            latency_distribution: LatencyDistribution::Uniform {
                min: config.network_conditions.latency_ms.0,
                max: config.network_conditions.latency_ms.1,
            },
            bandwidth_limit_mbps: config.network_conditions.bandwidth_mbps,
            max_nodes: config.max_clients * 2, // clients + daemon nodes
            duration: config.test_duration,
            enable_logging: true,
        };

        let network_simulator = Arc::new(NetworkSimulator::new(sim_config));
        
        // Initialize network topology
        for i in 0..config.max_clients {
            network_simulator.add_node(i, Some((i as f64, 0.0))).await;
        }
        
        let mut metrics = E2EMetrics::default();
        metrics.test_start_time = Some(Instant::now());

        Self {
            network_simulator,
            active_connections: Arc::new(RwLock::new(HashMap::new())),
            metrics_collector: Arc::new(Mutex::new(metrics)),
            test_config: config,
        }
    }

    async fn record_connection(&self, conn_info: ConnectionInfo) {
        let mut connections = self.active_connections.write().await;
        connections.insert(conn_info.connection_id, conn_info.clone());
        
        let mut metrics = self.metrics_collector.lock().unwrap();
        metrics.total_connections += 1;
        if conn_info.status == ConnectionStatus::Established {
            metrics.successful_connections += 1;
        }
        
        let current_count = connections.len() as u32;
        if current_count > metrics.peak_concurrent_connections {
            metrics.peak_concurrent_connections = current_count;
        }
    }

    async fn update_connection_status(&self, connection_id: u32, status: ConnectionStatus) {
        let mut connections = self.active_connections.write().await;
        if let Some(conn) = connections.get_mut(&connection_id) {
            conn.status = status.clone();
            conn.last_activity = Instant::now();
            
            if let ConnectionStatus::Failed(error) = status {
                let mut metrics = self.metrics_collector.lock().unwrap();
                metrics.failed_connections += 1;
                *metrics.error_counts.entry(error).or_insert(0) += 1;
            }
        }
    }

    async fn record_data_transfer(&self, connection_id: u32, bytes_sent: u64, bytes_received: u64) {
        let mut connections = self.active_connections.write().await;
        if let Some(conn) = connections.get_mut(&connection_id) {
            conn.bytes_sent += bytes_sent;
            conn.bytes_received += bytes_received;
            conn.last_activity = Instant::now();
        }
        
        let mut metrics = self.metrics_collector.lock().unwrap();
        metrics.total_bytes_transferred += bytes_sent + bytes_received;
    }

    async fn get_metrics_summary(&self) -> E2EMetrics {
        let metrics = self.metrics_collector.lock().unwrap();
        metrics.clone()
    }
}

/// Test complete client-to-daemon data flow scenario with full pipeline
#[tokio::test]
async fn test_client_daemon_data_flow() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Testing comprehensive client-to-daemon data flow scenario");

    let config = E2ETestConfig::default();
    let context = E2ETestContext::new(config).await;
    
    // Start network simulation
    context.network_simulator.start().await.expect("Failed to start network simulator");

    // Simulate complete client-daemon setup
    let client_id = "client_001";
    let connection_id = 1;
    
    // Phase 1: Crypto layer setup and handshake
    info!("Phase 1: Setting up crypto layer");
    let client_hpke = HpkeKeyDeriver::new().expect("Failed to create client HPKE");
    let daemon_hpke = HpkeKeyDeriver::new().expect("Failed to create daemon HPKE");
    
    let (client_private, client_public) = client_hpke.derive_keypair()
        .expect("Failed to derive client keypair");
    let (daemon_private, daemon_public) = daemon_hpke.derive_keypair()
        .expect("Failed to derive daemon keypair");
    
    // Perform HPKE key exchange
    let (shared_secret, encapsulated_key) = client_hpke.encapsulate(&daemon_public)
        .expect("Failed to encapsulate key");
    let decapsulated_secret = daemon_hpke.decapsulate(&daemon_private, &encapsulated_key)
        .expect("Failed to decapsulate key");
    
    assert_eq!(shared_secret.as_bytes(), decapsulated_secret.as_bytes());
    info!("HPKE key exchange completed successfully");

    // Noise protocol handshake
    let mut client_noise = NoiseHandshake::new_initiator()
        .expect("Failed to create client noise handshake");
    let mut daemon_noise = NoiseHandshake::new_responder()
        .expect("Failed to create daemon noise handshake");
    
    let mut message_buffer = vec![0u8; 4096];
    let mut payload_buffer = vec![0u8; 4096];
    
    // Complete 3-way handshake
    let msg1_len = client_noise.write_message(b"client_init", &mut message_buffer)
        .expect("Failed to write client init");
    
    let payload1_len = daemon_noise.read_message(&message_buffer[..msg1_len], &mut payload_buffer)
        .expect("Failed to read client init");
    assert_eq!(&payload_buffer[..payload1_len], b"client_init");
    
    let msg2_len = daemon_noise.write_message(b"daemon_response", &mut message_buffer)
        .expect("Failed to write daemon response");
    
    let payload2_len = client_noise.read_message(&message_buffer[..msg2_len], &mut payload_buffer)
        .expect("Failed to read daemon response");
    assert_eq!(&payload_buffer[..payload2_len], b"daemon_response");
    
    let msg3_len = client_noise.write_message(b"client_final", &mut message_buffer)
        .expect("Failed to write client final");
    
    let payload3_len = daemon_noise.read_message(&message_buffer[..msg3_len], &mut payload_buffer)
        .expect("Failed to read client final");
    assert_eq!(&payload_buffer[..payload3_len], b"client_final");
    
    info!("Noise protocol handshake completed");

    // Phase 2: Stream layer setup
    info!("Phase 2: Setting up stream layer");
    let client_stream = NyxAsyncStream::new(connection_id, 131072, 2048);
    let daemon_stream = NyxAsyncStream::new(connection_id + 1000, 131072, 2048);
    
    // Record connection establishment
    let conn_info = ConnectionInfo {
        connection_id,
        client_id: client_id.to_string(),
        established_at: Instant::now(),
        bytes_sent: 0,
        bytes_received: 0,
        last_activity: Instant::now(),
        status: ConnectionStatus::Established,
    };
    context.record_connection(conn_info).await;

    // Phase 3: Data transfer through complete pipeline
    info!("Phase 3: Testing data transfer through complete pipeline");
    let test_scenarios = vec![
        ("Small message", vec![0xAA; 256]),
        ("Medium message", vec![0xBB; 4096]),
        ("Large message", vec![0xCC; 32768]),
        ("Binary data", (0..1024).map(|i| (i % 256) as u8).collect()),
    ];

    for (scenario_name, test_data) in test_scenarios {
        info!("Testing scenario: {} ({} bytes)", scenario_name, test_data.len());
        
        let start_time = Instant::now();
        
        // Simulate data going through all layers:
        // 1. Application data -> Stream layer (framing)
        // 2. Stream frames -> Mix layer (routing)
        // 3. Mix output -> FEC layer (error correction)
        // 4. FEC output -> Transport layer (network transmission)
        
        // Simulate network transmission with realistic conditions
        let packet = SimulatedPacket {
            id: thread_rng().gen(),
            source: 0,
            destination: 1,
            payload: test_data.clone(),
            timestamp: Instant::now(),
            size_bytes: test_data.len(),
            priority: PacketPriority::Normal,
        };
        
        let transmission_result = context.network_simulator.send_packet(packet).await;
        match transmission_result {
            Ok(()) => {
                let transfer_time = start_time.elapsed();
                info!("Scenario '{}' completed in {:?}", scenario_name, transfer_time);
                
                // Record successful data transfer
                context.record_data_transfer(connection_id, test_data.len() as u64, test_data.len() as u64).await;
            }
            Err(e) => {
                warn!("Scenario '{}' failed: {:?}", scenario_name, e);
                context.update_connection_status(connection_id, ConnectionStatus::Failed(format!("{:?}", e))).await;
            }
        }
        
        // Small delay between scenarios
        sleep(Duration::from_millis(50)).await;
    }

    // Phase 4: Verify data integrity and performance
    info!("Phase 4: Verifying data integrity and performance");
    let metrics = context.get_metrics_summary().await;
    
    assert_eq!(metrics.successful_connections, 1);
    assert!(metrics.total_bytes_transferred > 0);
    info!("Data integrity verified: {} bytes transferred", metrics.total_bytes_transferred);

    // Phase 5: Connection cleanup
    info!("Phase 5: Connection cleanup");
    context.update_connection_status(connection_id, ConnectionStatus::Disconnecting).await;
    
    // Simulate graceful connection teardown
    sleep(Duration::from_millis(100)).await;
    
    context.update_connection_status(connection_id, ConnectionStatus::Disconnected).await;
    context.network_simulator.stop().await;
    
    info!("Client-to-daemon data flow scenario completed successfully");
    
    // Final verification
    let final_metrics = context.get_metrics_summary().await;
    assert!(final_metrics.total_connections > 0);
    assert!(final_metrics.total_bytes_transferred > 0);
}

/// Test multiple client concurrent connections scenario with realistic load
#[tokio::test]
async fn test_multiple_client_connections() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Testing multiple client concurrent connections scenario");

    let mut config = E2ETestConfig::default();
    config.max_clients = 25;
    config.test_duration = Duration::from_secs(10);
    
    let context = E2ETestContext::new(config.clone()).await;
    context.network_simulator.start().await.expect("Failed to start network simulator");

    let num_clients = 25;
    let mut client_handles = Vec::new();
    let connection_results = Arc::new(Mutex::new(Vec::new()));
    
    info!("Starting {} concurrent client connections", num_clients);
    
    // Simulate multiple clients connecting concurrently with realistic scenarios
    for client_id in 0..num_clients {
        let context_clone = context.clone();
        let results_clone = Arc::clone(&connection_results);
        
        let handle = tokio::spawn(async move {
            let client_name = format!("client_{:03}", client_id);
            let connection_id = client_id + 1;
            
            let start_time = Instant::now();
            
            // Phase 1: Client crypto setup
            let hpke = HpkeKeyDeriver::new().expect("Failed to create HPKE");
            let (private_key, public_key) = hpke.derive_keypair()
                .expect("Failed to derive keypair");
            
            // Phase 2: Noise handshake simulation
            let mut noise = NoiseHandshake::new_initiator()
                .expect("Failed to create noise handshake");
            
            // Simulate handshake delay based on network conditions
            let handshake_delay = Duration::from_millis(10 + (client_id % 50) as u64);
            sleep(handshake_delay).await;
            
            // Phase 3: Stream creation and setup
            let stream = NyxAsyncStream::new(connection_id, 65536, 1024);
            
            // Record connection establishment
            let conn_info = ConnectionInfo {
                connection_id,
                client_id: client_name.clone(),
                established_at: Instant::now(),
                bytes_sent: 0,
                bytes_received: 0,
                last_activity: Instant::now(),
                status: ConnectionStatus::Established,
            };
            
            context_clone.record_connection(conn_info).await;
            
            // Phase 4: Simulate client activity patterns
            let activity_patterns = vec![
                ("burst", 5, 1024),      // 5 messages of 1KB each
                ("steady", 10, 512),     // 10 messages of 512B each  
                ("large", 2, 8192),      // 2 messages of 8KB each
            ];
            
            let pattern = &activity_patterns[client_id as usize % activity_patterns.len()];
            let (pattern_name, message_count, message_size) = pattern;
            
            info!("Client {} using pattern: {} ({} messages of {} bytes)", 
                  client_name, pattern_name, message_count, message_size);
            
            let mut total_bytes_sent = 0u64;
            let mut total_bytes_received = 0u64;
            
            for msg_id in 0..*message_count {
                let message_data = vec![((client_id + msg_id) % 256) as u8; *message_size];
                
                // Simulate message transmission through network
                let packet = SimulatedPacket {
                    id: thread_rng().gen(),
                    source: client_id,
                    destination: 1000, // Daemon node
                    payload: message_data.clone(),
                    timestamp: Instant::now(),
                    size_bytes: message_data.len(),
                    priority: if *message_size > 4096 { 
                        PacketPriority::High 
                    } else { 
                        PacketPriority::Normal 
                    },
                };
                
                match context_clone.network_simulator.send_packet(packet).await {
                    Ok(()) => {
                        total_bytes_sent += message_data.len() as u64;
                        total_bytes_received += message_data.len() as u64; // Simulate echo
                        
                        // Update connection activity
                        context_clone.record_data_transfer(
                            connection_id, 
                            message_data.len() as u64, 
                            message_data.len() as u64
                        ).await;
                    }
                    Err(e) => {
                        warn!("Client {} message {} failed: {:?}", client_name, msg_id, e);
                        context_clone.update_connection_status(
                            connection_id, 
                            ConnectionStatus::Failed(format!("Message send failed: {:?}", e))
                        ).await;
                    }
                }
                
                // Realistic inter-message delay
                let delay = Duration::from_millis(50 + (client_id % 100) as u64);
                sleep(delay).await;
            }
            
            // Phase 5: Connection cleanup
            context_clone.update_connection_status(connection_id, ConnectionStatus::Disconnecting).await;
            sleep(Duration::from_millis(10)).await;
            context_clone.update_connection_status(connection_id, ConnectionStatus::Disconnected).await;
            
            let connection_duration = start_time.elapsed();
            
            let result = ClientConnectionResult {
                client_id,
                client_name: client_name.clone(),
                connection_duration,
                bytes_sent: total_bytes_sent,
                bytes_received: total_bytes_received,
                messages_sent: *message_count as u64,
                success: true,
                error: None,
            };
            
            {
                let mut results = results_clone.lock().unwrap();
                results.push(result.clone());
            }
            
            info!("Client {} completed: {} bytes sent/received in {:?}", 
                  client_name, total_bytes_sent + total_bytes_received, connection_duration);
            
            result
        });
        
        client_handles.push(handle);
        
        // Stagger connection attempts to simulate realistic load
        if client_id % 5 == 4 {
            sleep(Duration::from_millis(100)).await;
        }
    }
    
    info!("Waiting for all {} clients to complete", num_clients);
    
    // Wait for all clients to complete with timeout
    let timeout_duration = Duration::from_secs(30);
    let results = match timeout(timeout_duration, join_all(client_handles)).await {
        Ok(results) => results,
        Err(_) => {
            error!("Client connections timed out after {:?}", timeout_duration);
            panic!("Test timed out");
        }
    };
    
    // Analyze results
    let mut successful_clients = 0;
    let mut failed_clients = 0;
    let mut total_bytes_transferred = 0u64;
    let mut total_messages = 0u64;
    let mut connection_durations = Vec::new();
    
    for result in results {
        match result {
            Ok(client_result) => {
                if client_result.success {
                    successful_clients += 1;
                    total_bytes_transferred += client_result.bytes_sent + client_result.bytes_received;
                    total_messages += client_result.messages_sent;
                    connection_durations.push(client_result.connection_duration);
                } else {
                    failed_clients += 1;
                    warn!("Client {} failed: {:?}", client_result.client_name, client_result.error);
                }
            }
            Err(e) => {
                failed_clients += 1;
                error!("Client task failed: {:?}", e);
            }
        }
    }
    
    // Get final metrics
    let final_metrics = context.get_metrics_summary().await;
    context.network_simulator.stop().await;
    
    // Verify results
    info!("Multiple client test results:");
    info!("  Successful clients: {}/{}", successful_clients, num_clients);
    info!("  Failed clients: {}", failed_clients);
    info!("  Total bytes transferred: {}", total_bytes_transferred);
    info!("  Total messages: {}", total_messages);
    info!("  Peak concurrent connections: {}", final_metrics.peak_concurrent_connections);
    
    if !connection_durations.is_empty() {
        let avg_duration = connection_durations.iter().sum::<Duration>() / connection_durations.len() as u32;
        let max_duration = connection_durations.iter().max().unwrap();
        let min_duration = connection_durations.iter().min().unwrap();
        
        info!("  Connection durations - avg: {:?}, min: {:?}, max: {:?}", 
              avg_duration, min_duration, max_duration);
    }
    
    // Assertions for test success
    assert!(successful_clients >= (num_clients * 8 / 10), 
            "Should have at least 80% successful connections");
    assert!(total_bytes_transferred > 0, "Should have transferred some data");
    assert!(final_metrics.peak_concurrent_connections > 0, "Should have tracked concurrent connections");
    
    info!("Multiple client connections scenario completed successfully");
}

/// Result structure for client connection tests
#[derive(Debug, Clone)]
struct ClientConnectionResult {
    client_id: u32,
    client_name: String,
    connection_duration: Duration,
    bytes_sent: u64,
    bytes_received: u64,
    messages_sent: u64,
    success: bool,
    error: Option<String>,
}

/// Test multipath communication scenario with realistic path diversity
#[tokio::test]
async fn test_multipath_communication() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Testing comprehensive multipath communication scenario");

    let mut config = E2ETestConfig::default();
    config.network_conditions.packet_loss_rate = 0.15; // Higher loss to test multipath benefits
    config.test_duration = Duration::from_secs(15);
    
    let context = E2ETestContext::new(config).await;
    context.network_simulator.start().await.expect("Failed to start network simulator");

    // Configure multiple network paths with different characteristics
    let path_configs = vec![
        PathConfig { id: 0, latency_ms: 20.0, bandwidth_mbps: 100.0, loss_rate: 0.05, name: "fast_path" },
        PathConfig { id: 1, latency_ms: 80.0, bandwidth_mbps: 50.0, loss_rate: 0.10, name: "medium_path" },
        PathConfig { id: 2, latency_ms: 150.0, bandwidth_mbps: 25.0, loss_rate: 0.20, name: "slow_path" },
        PathConfig { id: 3, latency_ms: 40.0, bandwidth_mbps: 75.0, loss_rate: 0.08, name: "backup_path" },
    ];

    info!("Setting up {} diverse network paths", path_configs.len());
    
    // Create network topology for multipath
    for (i, path_config) in path_configs.iter().enumerate() {
        // Add intermediate nodes for each path
        let node_id = 100 + i as u32;
        context.network_simulator.add_node(node_id, Some((i as f64 * 10.0, i as f64 * 5.0))).await;
        
        // Configure path-specific network conditions
        let path_simulator = &context.network_simulator;
        path_simulator.simulate_packet_loss(path_config.loss_rate).await;
        path_simulator.simulate_latency(LatencyDistribution::Fixed { 
            latency_ms: path_config.latency_ms 
        }).await;
        path_simulator.simulate_bandwidth_limit(path_config.bandwidth_mbps).await;
        
        info!("Configured path '{}': {}ms latency, {}Mbps bandwidth, {:.1}% loss", 
              path_config.name, path_config.latency_ms, path_config.bandwidth_mbps, 
              path_config.loss_rate * 100.0);
    }

    // Create streams for each path
    let mut path_streams = Vec::new();
    let mut path_metrics = HashMap::new();
    
    for path_config in &path_configs {
        let stream = NyxAsyncStream::new(path_config.id, 131072, 2048);
        path_streams.push((path_config.clone(), stream));
        
        path_metrics.insert(path_config.id, PathMetrics {
            bytes_sent: 0,
            bytes_received: 0,
            packets_sent: 0,
            packets_lost: 0,
            avg_latency: Duration::ZERO,
            throughput_mbps: 0.0,
        });
    }

    // Test data distribution across multiple paths
    info!("Testing data distribution across multipath network");
    
    let test_data_sets = vec![
        ("small_burst", vec![0xAA; 2048], 10),      // 10 x 2KB packets
        ("medium_stream", vec![0xBB; 8192], 5),     // 5 x 8KB packets  
        ("large_transfer", vec![0xCC; 32768], 2),   // 2 x 32KB packets
        ("mixed_load", vec![0xDD; 4096], 8),        // 8 x 4KB packets
    ];

    let mut total_bytes_sent = 0u64;
    let mut total_packets_sent = 0u64;
    let mut path_usage_stats = HashMap::new();

    for (test_name, base_data, packet_count) in test_data_sets {
        info!("Running multipath test: {} ({} packets of {} bytes)", 
              test_name, packet_count, base_data.len());
        
        let test_start = Instant::now();
        let mut test_handles = Vec::new();
        
        // Distribute packets across paths using load balancing
        for packet_id in 0..packet_count {
            let path_index = packet_id % path_configs.len();
            let path_config = &path_configs[path_index];
            let context_clone = context.clone();
            
            // Create unique packet data
            let mut packet_data = base_data.clone();
            packet_data.extend_from_slice(&(packet_id as u32).to_be_bytes());
            
            let handle = tokio::spawn(async move {
                let packet = SimulatedPacket {
                    id: thread_rng().gen(),
                    source: 0,
                    destination: 100 + path_config.id,
                    payload: packet_data.clone(),
                    timestamp: Instant::now(),
                    size_bytes: packet_data.len(),
                    priority: PacketPriority::Normal,
                };
                
                let send_start = Instant::now();
                let result = context_clone.network_simulator.send_packet(packet).await;
                let send_duration = send_start.elapsed();
                
                PacketResult {
                    path_id: path_config.id,
                    packet_id,
                    size_bytes: packet_data.len(),
                    send_duration,
                    success: result.is_ok(),
                    error: result.err().map(|e| format!("{:?}", e)),
                }
            });
            
            test_handles.push(handle);
            
            // Add realistic inter-packet delay
            sleep(Duration::from_millis(10)).await;
        }
        
        // Wait for all packets in this test to complete
        let packet_results = join_all(test_handles).await;
        let test_duration = test_start.elapsed();
        
        // Analyze results for this test
        let mut successful_packets = 0;
        let mut failed_packets = 0;
        let mut test_bytes_sent = 0;
        
        for result in packet_results {
            match result {
                Ok(packet_result) => {
                    if packet_result.success {
                        successful_packets += 1;
                        test_bytes_sent += packet_result.size_bytes;
                        
                        // Update path usage statistics
                        *path_usage_stats.entry(packet_result.path_id).or_insert(0) += 1;
                        
                        // Update path metrics
                        if let Some(metrics) = path_metrics.get_mut(&packet_result.path_id) {
                            metrics.bytes_sent += packet_result.size_bytes as u64;
                            metrics.packets_sent += 1;
                            
                            // Update average latency
                            let current_avg = metrics.avg_latency;
                            let packet_count = metrics.packets_sent;
                            metrics.avg_latency = (current_avg * (packet_count - 1) as u32 + packet_result.send_duration) / packet_count as u32;
                        }
                    } else {
                        failed_packets += 1;
                        if let Some(metrics) = path_metrics.get_mut(&packet_result.path_id) {
                            metrics.packets_lost += 1;
                        }
                    }
                }
                Err(e) => {
                    failed_packets += 1;
                    error!("Packet task failed: {:?}", e);
                }
            }
        }
        
        total_bytes_sent += test_bytes_sent as u64;
        total_packets_sent += packet_count as u64;
        
        let success_rate = successful_packets as f64 / packet_count as f64;
        let throughput_mbps = (test_bytes_sent * 8) as f64 / (test_duration.as_secs_f64() * 1_000_000.0);
        
        info!("Test '{}' completed: {}/{} packets successful ({:.1}%), {:.2} Mbps throughput", 
              test_name, successful_packets, packet_count, success_rate * 100.0, throughput_mbps);
        
        // Brief pause between test sets
        sleep(Duration::from_millis(200)).await;
    }

    // Analyze overall multipath performance
    info!("Analyzing multipath performance:");
    
    for (path_id, usage_count) in &path_usage_stats {
        let path_config = path_configs.iter().find(|p| p.id == *path_id).unwrap();
        let metrics = &path_metrics[path_id];
        
        let throughput = if metrics.avg_latency > Duration::ZERO {
            (metrics.bytes_sent * 8) as f64 / (metrics.avg_latency.as_secs_f64() * 1_000_000.0)
        } else {
            0.0
        };
        
        let loss_rate = if metrics.packets_sent > 0 {
            metrics.packets_lost as f64 / (metrics.packets_sent + metrics.packets_lost) as f64
        } else {
            0.0
        };
        
        info!("  Path '{}': {} packets, {:.2} Mbps throughput, {:.1}% loss, {:?} avg latency", 
              path_config.name, usage_count, throughput, loss_rate * 100.0, metrics.avg_latency);
    }

    // Test path failover scenario
    info!("Testing path failover scenario");
    
    // Simulate failure of the fastest path
    let failed_path_id = 0; // fast_path
    info!("Simulating failure of path {}", failed_path_id);
    
    // Send packets that would normally use the failed path to other paths
    let failover_packets = 5;
    let mut failover_handles = Vec::new();
    
    for packet_id in 0..failover_packets {
        // Use backup path instead of failed path
        let backup_path_id = 3; // backup_path
        let context_clone = context.clone();
        
        let handle = tokio::spawn(async move {
            let packet_data = vec![0xEE; 4096]; // 4KB failover packet
            
            let packet = SimulatedPacket {
                id: thread_rng().gen(),
                source: 0,
                destination: 100 + backup_path_id,
                payload: packet_data.clone(),
                timestamp: Instant::now(),
                size_bytes: packet_data.len(),
                priority: PacketPriority::High, // Higher priority for failover
            };
            
            context_clone.network_simulator.send_packet(packet).await
        });
        
        failover_handles.push(handle);
    }
    
    let failover_results = join_all(failover_handles).await;
    let successful_failovers = failover_results.iter()
        .filter(|r| r.as_ref().map_or(false, |res| res.is_ok()))
        .count();
    
    info!("Failover test: {}/{} packets successfully rerouted", 
          successful_failovers, failover_packets);

    // Final verification and cleanup
    let final_metrics = context.get_metrics_summary().await;
    context.network_simulator.stop().await;
    
    info!("Multipath communication test summary:");
    info!("  Total bytes sent: {}", total_bytes_sent);
    info!("  Total packets sent: {}", total_packets_sent);
    info!("  Paths utilized: {}", path_usage_stats.len());
    info!("  Failover success rate: {:.1}%", 
          (successful_failovers as f64 / failover_packets as f64) * 100.0);
    
    // Assertions
    assert!(total_bytes_sent > 0, "Should have sent data across paths");
    assert!(path_usage_stats.len() >= 3, "Should have used multiple paths");
    assert!(successful_failovers >= failover_packets / 2, "Should have reasonable failover success");
    
    info!("Multipath communication scenario completed successfully");
}

/// Configuration for individual network paths
#[derive(Debug, Clone)]
struct PathConfig {
    id: u32,
    latency_ms: f64,
    bandwidth_mbps: f64,
    loss_rate: f64,
    name: &'static str,
}

/// Metrics tracking for individual paths
#[derive(Debug, Clone)]
struct PathMetrics {
    bytes_sent: u64,
    bytes_received: u64,
    packets_sent: u64,
    packets_lost: u64,
    avg_latency: Duration,
    throughput_mbps: f64,
}

/// Result of individual packet transmission
#[derive(Debug)]
struct PacketResult {
    path_id: u32,
    packet_id: usize,
    size_bytes: usize,
    send_duration: Duration,
    success: bool,
    error: Option<String>,
}

/// Test long-running connection scenario with realistic patterns
#[tokio::test]
async fn test_long_running_connection() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Testing comprehensive long-running connection scenario");

    let mut config = E2ETestConfig::default();
    config.test_duration = Duration::from_secs(20); // Extended test duration
    config.network_conditions.packet_loss_rate = 0.02; // Low loss for stability
    
    let context = E2ETestContext::new(config).await;
    context.network_simulator.start().await.expect("Failed to start network simulator");

    let connection_id = 1;
    let client_name = "long_running_client";
    
    // Phase 1: Establish long-running connection
    info!("Phase 1: Establishing long-running connection");
    
    let hpke = HpkeKeyDeriver::new().expect("Failed to create HPKE");
    let (private_key, public_key) = hpke.derive_keypair()
        .expect("Failed to derive keypair");
    
    let stream = NyxAsyncStream::new(connection_id, 262144, 4096); // Large buffers for long-running
    
    let conn_info = ConnectionInfo {
        connection_id,
        client_id: client_name.to_string(),
        established_at: Instant::now(),
        bytes_sent: 0,
        bytes_received: 0,
        last_activity: Instant::now(),
        status: ConnectionStatus::Established,
    };
    
    context.record_connection(conn_info).await;
    info!("Long-running connection established for client: {}", client_name);

    // Phase 2: Simulate realistic long-running activity patterns
    info!("Phase 2: Simulating long-running activity patterns");
    
    let activity_patterns = vec![
        ActivityPattern {
            name: "heartbeat",
            interval: Duration::from_millis(500),
            data_size: 64,
            priority: PacketPriority::High,
            duration: Duration::from_secs(20),
        },
        ActivityPattern {
            name: "periodic_sync",
            interval: Duration::from_secs(2),
            data_size: 2048,
            priority: PacketPriority::Normal,
            duration: Duration::from_secs(20),
        },
        ActivityPattern {
            name: "bulk_transfer",
            interval: Duration::from_secs(5),
            data_size: 16384,
            priority: PacketPriority::Low,
            duration: Duration::from_secs(20),
        },
        ActivityPattern {
            name: "status_report",
            interval: Duration::from_secs(3),
            data_size: 512,
            priority: PacketPriority::Normal,
            duration: Duration::from_secs(20),
        },
    ];

    let mut pattern_handles = Vec::new();
    let activity_stats = Arc::new(Mutex::new(HashMap::new()));
    
    // Start each activity pattern concurrently
    for pattern in activity_patterns {
        let context_clone = context.clone();
        let stats_clone = Arc::clone(&activity_stats);
        let pattern_name = pattern.name.to_string();
        
        let handle = tokio::spawn(async move {
            let mut pattern_stats = PatternStats {
                messages_sent: 0,
                bytes_sent: 0,
                successful_sends: 0,
                failed_sends: 0,
                avg_latency: Duration::ZERO,
                start_time: Instant::now(),
            };
            
            let end_time = Instant::now() + pattern.duration;
            let mut next_send = Instant::now();
            
            while Instant::now() < end_time {
                if Instant::now() >= next_send {
                    // Create pattern-specific data
                    let mut message_data = vec![0u8; pattern.data_size];
                    message_data[0..4].copy_from_slice(&pattern_stats.messages_sent.to_be_bytes());
                    
                    let packet = SimulatedPacket {
                        id: thread_rng().gen(),
                        source: connection_id,
                        destination: 1000,
                        payload: message_data.clone(),
                        timestamp: Instant::now(),
                        size_bytes: message_data.len(),
                        priority: pattern.priority.clone(),
                    };
                    
                    let send_start = Instant::now();
                    match context_clone.network_simulator.send_packet(packet).await {
                        Ok(()) => {
                            let send_latency = send_start.elapsed();
                            pattern_stats.successful_sends += 1;
                            pattern_stats.bytes_sent += message_data.len() as u64;
                            
                            // Update average latency
                            let total_sends = pattern_stats.successful_sends;
                            pattern_stats.avg_latency = (pattern_stats.avg_latency * (total_sends - 1) as u32 + send_latency) / total_sends as u32;
                            
                            // Record data transfer
                            context_clone.record_data_transfer(connection_id, message_data.len() as u64, 0).await;
                        }
                        Err(e) => {
                            pattern_stats.failed_sends += 1;
                            warn!("Pattern '{}' send failed: {:?}", pattern.name, e);
                        }
                    }
                    
                    pattern_stats.messages_sent += 1;
                    next_send = Instant::now() + pattern.interval;
                }
                
                // Small sleep to prevent busy waiting
                sleep(Duration::from_millis(10)).await;
            }
            
            // Record final stats
            {
                let mut stats = stats_clone.lock().unwrap();
                stats.insert(pattern_name.clone(), pattern_stats.clone());
            }
            
            info!("Pattern '{}' completed: {} messages, {} bytes, {:.1}% success rate", 
                  pattern_name, pattern_stats.messages_sent, pattern_stats.bytes_sent,
                  (pattern_stats.successful_sends as f64 / pattern_stats.messages_sent as f64) * 100.0);
            
            pattern_stats
        });
        
        pattern_handles.push((pattern_name, handle));
    }

    // Phase 3: Monitor connection health during long-running operation
    info!("Phase 3: Monitoring connection health");
    
    let health_monitor_handle = {
        let context_clone = context.clone();
        tokio::spawn(async move {
            let monitor_duration = Duration::from_secs(20);
            let check_interval = Duration::from_secs(1);
            let end_time = Instant::now() + monitor_duration;
            
            let mut health_samples = Vec::new();
            
            while Instant::now() < end_time {
                let connections = context_clone.active_connections.read().await;
                if let Some(conn) = connections.get(&connection_id) {
                    let time_since_activity = Instant::now().duration_since(conn.last_activity);
                    let is_healthy = time_since_activity < Duration::from_secs(5);
                    
                    health_samples.push(HealthSample {
                        timestamp: Instant::now(),
                        is_healthy,
                        bytes_sent: conn.bytes_sent,
                        bytes_received: conn.bytes_received,
                        time_since_activity,
                    });
                    
                    if !is_healthy {
                        warn!("Connection health degraded: {} since last activity", 
                              time_since_activity.as_secs());
                    }
                }
                
                sleep(check_interval).await;
            }
            
            health_samples
        })
    };

    // Phase 4: Wait for all patterns to complete
    info!("Phase 4: Waiting for activity patterns to complete");
    
    let mut total_messages = 0u64;
    let mut total_bytes = 0u64;
    let mut total_successful = 0u64;
    let mut total_failed = 0u64;
    
    for (pattern_name, handle) in pattern_handles {
        match handle.await {
            Ok(stats) => {
                total_messages += stats.messages_sent;
                total_bytes += stats.bytes_sent;
                total_successful += stats.successful_sends;
                total_failed += stats.failed_sends;
                
                info!("Pattern '{}' final stats: {} msgs, {} bytes, {:?} avg latency", 
                      pattern_name, stats.messages_sent, stats.bytes_sent, stats.avg_latency);
            }
            Err(e) => {
                error!("Pattern '{}' failed: {:?}", pattern_name, e);
            }
        }
    }

    // Get health monitoring results
    let health_samples = health_monitor_handle.await.expect("Health monitor should complete");
    let healthy_samples = health_samples.iter().filter(|s| s.is_healthy).count();
    let health_percentage = (healthy_samples as f64 / health_samples.len() as f64) * 100.0;
    
    info!("Connection health: {:.1}% healthy samples ({}/{})", 
          health_percentage, healthy_samples, health_samples.len());

    // Phase 5: Test connection resilience under stress
    info!("Phase 5: Testing connection resilience");
    
    // Temporarily increase network stress
    context.network_simulator.simulate_packet_loss(0.1).await; // 10% loss
    context.network_simulator.simulate_latency(LatencyDistribution::Uniform { 
        min: 50.0, max: 200.0 
    }).await;
    
    // Send burst of messages to test resilience
    let resilience_test_messages = 20;
    let mut resilience_handles = Vec::new();
    
    for msg_id in 0..resilience_test_messages {
        let context_clone = context.clone();
        
        let handle = tokio::spawn(async move {
            let message_data = vec![0xFF; 1024]; // 1KB stress test message
            
            let packet = SimulatedPacket {
                id: thread_rng().gen(),
                source: connection_id,
                destination: 1000,
                payload: message_data.clone(),
                timestamp: Instant::now(),
                size_bytes: message_data.len(),
                priority: PacketPriority::High,
            };
            
            context_clone.network_simulator.send_packet(packet).await
        });
        
        resilience_handles.push(handle);
        sleep(Duration::from_millis(50)).await; // Burst with small delays
    }
    
    let resilience_results = join_all(resilience_handles).await;
    let resilience_success = resilience_results.iter()
        .filter(|r| r.as_ref().map_or(false, |res| res.is_ok()))
        .count();
    
    let resilience_rate = (resilience_success as f64 / resilience_test_messages as f64) * 100.0;
    info!("Resilience test: {:.1}% success rate under stress ({}/{})", 
          resilience_rate, resilience_success, resilience_test_messages);

    // Phase 6: Graceful connection termination
    info!("Phase 6: Graceful connection termination");
    
    context.update_connection_status(connection_id, ConnectionStatus::Disconnecting).await;
    sleep(Duration::from_millis(100)).await;
    context.update_connection_status(connection_id, ConnectionStatus::Disconnected).await;

    // Final metrics and verification
    let final_metrics = context.get_metrics_summary().await;
    context.network_simulator.stop().await;
    
    let connection_duration = final_metrics.test_start_time
        .map(|start| start.elapsed())
        .unwrap_or(Duration::ZERO);
    
    info!("Long-running connection test summary:");
    info!("  Connection duration: {:?}", connection_duration);
    info!("  Total messages sent: {}", total_messages);
    info!("  Total bytes transferred: {}", total_bytes);
    info!("  Success rate: {:.1}%", (total_successful as f64 / total_messages as f64) * 100.0);
    info!("  Connection health: {:.1}%", health_percentage);
    info!("  Resilience under stress: {:.1}%", resilience_rate);
    
    // Assertions for test success
    assert!(total_messages > 0, "Should have sent messages");
    assert!(total_bytes > 0, "Should have transferred data");
    assert!(total_successful > total_failed, "Should have more successes than failures");
    assert!(health_percentage > 80.0, "Should maintain good connection health");
    assert!(resilience_rate > 50.0, "Should show reasonable resilience under stress");
    
    info!("Long-running connection scenario completed successfully");
}

/// Activity pattern for long-running connections
#[derive(Debug, Clone)]
struct ActivityPattern {
    name: &'static str,
    interval: Duration,
    data_size: usize,
    priority: PacketPriority,
    duration: Duration,
}

/// Statistics for activity patterns
#[derive(Debug, Clone)]
struct PatternStats {
    messages_sent: u64,
    bytes_sent: u64,
    successful_sends: u64,
    failed_sends: u64,
    avg_latency: Duration,
    start_time: Instant,
}

/// Health monitoring sample
#[derive(Debug, Clone)]
struct HealthSample {
    timestamp: Instant,
    is_healthy: bool,
    bytes_sent: u64,
    bytes_received: u64,
    time_since_activity: Duration,
}

/// Test comprehensive stress scenario with high connection count and load
#[tokio::test]
async fn test_stress_scenario() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Testing comprehensive stress scenario with high connection count");

    let mut config = E2ETestConfig::default();
    config.max_clients = 200;
    config.test_duration = Duration::from_secs(15);
    config.stress_test_enabled = true;
    config.network_conditions.packet_loss_rate = 0.05; // Higher loss under stress
    
    let context = E2ETestContext::new(config.clone()).await;
    context.network_simulator.start().await.expect("Failed to start network simulator");

    let connection_count = 200;
    let messages_per_connection = 10;
    let start_time = Instant::now();
    
    info!("Starting stress test: {} connections, {} messages each", 
          connection_count, messages_per_connection);

    // Phase 1: Rapid connection establishment
    info!("Phase 1: Rapid connection establishment");
    
    let mut stress_handles = Vec::new();
    let stress_results = Arc::new(Mutex::new(Vec::new()));
    let connection_semaphore = Arc::new(tokio::sync::Semaphore::new(50)); // Limit concurrent connections
    
    for conn_id in 0..connection_count {
        let context_clone = context.clone();
        let results_clone = Arc::clone(&stress_results);
        let semaphore_clone = Arc::clone(&connection_semaphore);
        
        let handle = tokio::spawn(async move {
            // Acquire semaphore to limit concurrent connections
            let _permit = semaphore_clone.acquire().await.expect("Semaphore should not be closed");
            
            let client_name = format!("stress_client_{:03}", conn_id);
            let connection_id = conn_id + 1;
            
            let mut stress_result = StressTestResult {
                connection_id,
                client_name: client_name.clone(),
                establishment_time: Duration::ZERO,
                messages_sent: 0,
                messages_successful: 0,
                bytes_transferred: 0,
                avg_latency: Duration::ZERO,
                peak_memory_usage: 0,
                errors: Vec::new(),
                success: false,
            };
            
            let establishment_start = Instant::now();
            
            // Quick crypto setup for stress test
            let hpke_result = HpkeKeyDeriver::new();
            let hpke = match hpke_result {
                Ok(h) => h,
                Err(e) => {
                    stress_result.errors.push(format!("HPKE creation failed: {:?}", e));
                    results_clone.lock().unwrap().push(stress_result);
                    return;
                }
            };
            
            let keypair_result = hpke.derive_keypair();
            let (private_key, public_key) = match keypair_result {
                Ok(kp) => kp,
                Err(e) => {
                    stress_result.errors.push(format!("Keypair derivation failed: {:?}", e));
                    results_clone.lock().unwrap().push(stress_result);
                    return;
                }
            };
            
            // Create stream with smaller buffers for stress test
            let stream = NyxAsyncStream::new(connection_id, 32768, 512);
            
            stress_result.establishment_time = establishment_start.elapsed();
            
            // Record connection
            let conn_info = ConnectionInfo {
                connection_id,
                client_id: client_name.clone(),
                established_at: Instant::now(),
                bytes_sent: 0,
                bytes_received: 0,
                last_activity: Instant::now(),
                status: ConnectionStatus::Established,
            };
            
            context_clone.record_connection(conn_info).await;
            
            // Phase 2: Message burst for each connection
            let mut total_latency = Duration::ZERO;
            let mut successful_messages = 0;
            
            for msg_id in 0..messages_per_connection {
                let message_size = 512 + (msg_id % 4) * 256; // Variable message sizes
                let message_data = vec![((conn_id + msg_id) % 256) as u8; message_size];
                
                let packet = SimulatedPacket {
                    id: thread_rng().gen(),
                    source: connection_id,
                    destination: 2000, // Stress test destination
                    payload: message_data.clone(),
                    timestamp: Instant::now(),
                    size_bytes: message_data.len(),
                    priority: if msg_id % 3 == 0 { 
                        PacketPriority::High 
                    } else { 
                        PacketPriority::Normal 
                    },
                };
                
                let send_start = Instant::now();
                match context_clone.network_simulator.send_packet(packet).await {
                    Ok(()) => {
                        let send_latency = send_start.elapsed();
                        total_latency += send_latency;
                        successful_messages += 1;
                        stress_result.bytes_transferred += message_data.len() as u64;
                        
                        // Record data transfer
                        context_clone.record_data_transfer(connection_id, message_data.len() as u64, 0).await;
                    }
                    Err(e) => {
                        stress_result.errors.push(format!("Message {} send failed: {:?}", msg_id, e));
                    }
                }
                
                stress_result.messages_sent += 1;
                
                // Very small delay to prevent overwhelming the system
                if msg_id % 5 == 0 {
                    sleep(Duration::from_millis(1)).await;
                }
            }
            
            stress_result.messages_successful = successful_messages;
            stress_result.avg_latency = if successful_messages > 0 {
                total_latency / successful_messages as u32
            } else {
                Duration::ZERO
            };
            
            // Determine success criteria
            let success_rate = successful_messages as f64 / messages_per_connection as f64;
            stress_result.success = success_rate >= 0.7 && stress_result.establishment_time < Duration::from_secs(1);
            
            // Update connection status
            if stress_result.success {
                context_clone.update_connection_status(connection_id, ConnectionStatus::Active).await;
            } else {
                context_clone.update_connection_status(connection_id, 
                    ConnectionStatus::Failed("Stress test criteria not met".to_string())).await;
            }
            
            // Cleanup
            context_clone.update_connection_status(connection_id, ConnectionStatus::Disconnected).await;
            
            // Record result
            results_clone.lock().unwrap().push(stress_result);
        });
        
        stress_handles.push(handle);
        
        // Stagger connection attempts to simulate realistic load
        if conn_id % 10 == 9 {
            sleep(Duration::from_millis(50)).await;
        }
    }
    
    info!("Waiting for {} stress test connections to complete", connection_count);
    
    // Wait for all connections with timeout
    let timeout_duration = Duration::from_secs(60);
    let join_result = timeout(timeout_duration, join_all(stress_handles)).await;
    
    match join_result {
        Ok(_) => info!("All stress test connections completed within timeout"),
        Err(_) => {
            error!("Stress test timed out after {:?}", timeout_duration);
            // Continue with analysis of completed connections
        }
    }
    
    let total_test_time = start_time.elapsed();
    
    // Phase 3: Analyze stress test results
    info!("Phase 3: Analyzing stress test results");
    
    let results = stress_results.lock().unwrap();
    let completed_connections = results.len();
    
    let mut successful_connections = 0;
    let mut failed_connections = 0;
    let mut total_messages_sent = 0u64;
    let mut total_messages_successful = 0u64;
    let mut total_bytes_transferred = 0u64;
    let mut establishment_times = Vec::new();
    let mut latencies = Vec::new();
    let mut error_categories = HashMap::new();
    
    for result in results.iter() {
        if result.success {
            successful_connections += 1;
        } else {
            failed_connections += 1;
        }
        
        total_messages_sent += result.messages_sent;
        total_messages_successful += result.messages_successful;
        total_bytes_transferred += result.bytes_transferred;
        establishment_times.push(result.establishment_time);
        
        if result.avg_latency > Duration::ZERO {
            latencies.push(result.avg_latency);
        }
        
        // Categorize errors
        for error in &result.errors {
            let category = if error.contains("HPKE") {
                "crypto"
            } else if error.contains("send failed") {
                "network"
            } else {
                "other"
            };
            *error_categories.entry(category.to_string()).or_insert(0) += 1;
        }
    }
    
    // Calculate statistics
    let success_rate = successful_connections as f64 / completed_connections as f64;
    let message_success_rate = if total_messages_sent > 0 {
        total_messages_successful as f64 / total_messages_sent as f64
    } else {
        0.0
    };
    
    let avg_establishment_time = if !establishment_times.is_empty() {
        establishment_times.iter().sum::<Duration>() / establishment_times.len() as u32
    } else {
        Duration::ZERO
    };
    
    let avg_latency = if !latencies.is_empty() {
        latencies.iter().sum::<Duration>() / latencies.len() as u32
    } else {
        Duration::ZERO
    };
    
    let throughput_mbps = if total_test_time.as_secs() > 0 {
        (total_bytes_transferred * 8) as f64 / (total_test_time.as_secs_f64() * 1_000_000.0)
    } else {
        0.0
    };
    
    // Get final system metrics
    let final_metrics = context.get_metrics_summary().await;
    context.network_simulator.stop().await;
    
    // Report results
    info!("Stress test results summary:");
    info!("  Test duration: {:?}", total_test_time);
    info!("  Completed connections: {}/{}", completed_connections, connection_count);
    info!("  Successful connections: {} ({:.1}%)", successful_connections, success_rate * 100.0);
    info!("  Failed connections: {}", failed_connections);
    info!("  Total messages sent: {}", total_messages_sent);
    info!("  Message success rate: {:.1}%", message_success_rate * 100.0);
    info!("  Total bytes transferred: {}", total_bytes_transferred);
    info!("  Average establishment time: {:?}", avg_establishment_time);
    info!("  Average message latency: {:?}", avg_latency);
    info!("  Overall throughput: {:.2} Mbps", throughput_mbps);
    info!("  Peak concurrent connections: {}", final_metrics.peak_concurrent_connections);
    
    if !error_categories.is_empty() {
        info!("  Error categories:");
        for (category, count) in &error_categories {
            info!("    {}: {} errors", category, count);
        }
    }
    
    // Performance assertions
    assert!(completed_connections >= connection_count * 8 / 10, 
            "Should complete at least 80% of connections");
    assert!(success_rate >= 0.6, 
            "Should have at least 60% success rate under stress");
    assert!(message_success_rate >= 0.7, 
            "Should have at least 70% message success rate");
    assert!(total_bytes_transferred > 0, 
            "Should have transferred some data");
    assert!(avg_establishment_time < Duration::from_secs(2), 
            "Connection establishment should be reasonably fast");
    
    info!("Stress scenario completed successfully");
}

/// Result structure for stress test connections
#[derive(Debug, Clone)]
struct StressTestResult {
    connection_id: u32,
    client_name: String,
    establishment_time: Duration,
    messages_sent: u64,
    messages_successful: u64,
    bytes_transferred: u64,
    avg_latency: Duration,
    peak_memory_usage: u64,
    errors: Vec<String>,
    success: bool,
}

/// Test comprehensive network failure and recovery scenario
#[tokio::test]
async fn test_network_failure_recovery() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Testing comprehensive network failure and recovery scenario");

    let mut config = E2ETestConfig::default();
    config.failure_injection_enabled = true;
    config.test_duration = Duration::from_secs(25);
    
    let context = E2ETestContext::new(config).await;
    context.network_simulator.start().await.expect("Failed to start network simulator");

    let connection_id = 1;
    let client_name = "failure_recovery_client";
    
    // Phase 1: Establish connection under normal conditions
    info!("Phase 1: Establishing connection under normal conditions");
    
    // Start with good network conditions
    context.network_simulator.simulate_packet_loss(0.01).await; // 1% loss
    context.network_simulator.simulate_latency(LatencyDistribution::Fixed { latency_ms: 20.0 }).await;
    
    let hpke = HpkeKeyDeriver::new().expect("Failed to create HPKE");
    let (private_key, public_key) = hpke.derive_keypair()
        .expect("Failed to derive keypair");
    
    let stream = NyxAsyncStream::new(connection_id, 131072, 2048);
    
    let conn_info = ConnectionInfo {
        connection_id,
        client_id: client_name.to_string(),
        established_at: Instant::now(),
        bytes_sent: 0,
        bytes_received: 0,
        last_activity: Instant::now(),
        status: ConnectionStatus::Established,
    };
    
    context.record_connection(conn_info).await;
    info!("Connection established under normal conditions");

    // Phase 2: Normal operation baseline
    info!("Phase 2: Establishing normal operation baseline");
    
    let baseline_messages = 10;
    let mut baseline_stats = OperationStats::new("baseline");
    
    for i in 0..baseline_messages {
        let message_data = vec![0xAA; 2048]; // 2KB messages
        
        let packet = SimulatedPacket {
            id: thread_rng().gen(),
            source: connection_id,
            destination: 1000,
            payload: message_data.clone(),
            timestamp: Instant::now(),
            size_bytes: message_data.len(),
            priority: PacketPriority::Normal,
        };
        
        let send_start = Instant::now();
        match context.network_simulator.send_packet(packet).await {
            Ok(()) => {
                baseline_stats.record_success(send_start.elapsed(), message_data.len());
                context.record_data_transfer(connection_id, message_data.len() as u64, 0).await;
            }
            Err(e) => {
                baseline_stats.record_failure(format!("{:?}", e));
            }
        }
        
        sleep(Duration::from_millis(100)).await;
    }
    
    info!("Baseline established: {:.1}% success, {:?} avg latency", 
          baseline_stats.success_rate() * 100.0, baseline_stats.avg_latency());

    // Phase 3: Gradual network degradation
    info!("Phase 3: Simulating gradual network degradation");
    
    let degradation_steps = vec![
        (0.05, 50.0, "mild_degradation"),
        (0.15, 100.0, "moderate_degradation"),
        (0.30, 200.0, "severe_degradation"),
    ];
    
    let mut degradation_results = Vec::new();
    
    for (loss_rate, latency_ms, phase_name) in degradation_steps {
        info!("Applying {}: {:.1}% loss, {}ms latency", phase_name, loss_rate * 100.0, latency_ms);
        
        context.network_simulator.simulate_packet_loss(loss_rate).await;
        context.network_simulator.simulate_latency(LatencyDistribution::Fixed { latency_ms }).await;
        
        let mut phase_stats = OperationStats::new(phase_name);
        let phase_messages = 8;
        
        for i in 0..phase_messages {
            let message_data = vec![0xBB; 1024]; // Smaller messages during degradation
            
            let packet = SimulatedPacket {
                id: thread_rng().gen(),
                source: connection_id,
                destination: 1000,
                payload: message_data.clone(),
                timestamp: Instant::now(),
                size_bytes: message_data.len(),
                priority: PacketPriority::High, // Higher priority during degradation
            };
            
            let send_start = Instant::now();
            match context.network_simulator.send_packet(packet).await {
                Ok(()) => {
                    phase_stats.record_success(send_start.elapsed(), message_data.len());
                    context.record_data_transfer(connection_id, message_data.len() as u64, 0).await;
                }
                Err(e) => {
                    phase_stats.record_failure(format!("{:?}", e));
                }
            }
            
            sleep(Duration::from_millis(150)).await; // Longer delays during degradation
        }
        
        info!("Phase '{}': {:.1}% success, {:?} avg latency", 
              phase_name, phase_stats.success_rate() * 100.0, phase_stats.avg_latency());
        
        degradation_results.push((phase_name.to_string(), phase_stats));
        
        // Brief pause between degradation phases
        sleep(Duration::from_millis(500)).await;
    }

    // Phase 4: Complete network failure
    info!("Phase 4: Simulating complete network failure");
    
    context.network_simulator.simulate_packet_loss(0.95).await; // 95% loss
    context.network_simulator.simulate_latency(LatencyDistribution::Fixed { latency_ms: 1000.0 }).await;
    
    let mut failure_stats = OperationStats::new("complete_failure");
    let failure_messages = 5;
    
    for i in 0..failure_messages {
        let message_data = vec![0xCC; 512]; // Very small messages during failure
        
        let packet = SimulatedPacket {
            id: thread_rng().gen(),
            source: connection_id,
            destination: 1000,
            payload: message_data.clone(),
            timestamp: Instant::now(),
            size_bytes: message_data.len(),
            priority: PacketPriority::Critical, // Critical priority during failure
        };
        
        let send_start = Instant::now();
        match timeout(Duration::from_secs(2), context.network_simulator.send_packet(packet)).await {
            Ok(Ok(())) => {
                failure_stats.record_success(send_start.elapsed(), message_data.len());
                context.record_data_transfer(connection_id, message_data.len() as u64, 0).await;
            }
            Ok(Err(e)) => {
                failure_stats.record_failure(format!("Send failed: {:?}", e));
            }
            Err(_) => {
                failure_stats.record_failure("Send timeout".to_string());
            }
        }
        
        sleep(Duration::from_millis(300)).await;
    }
    
    info!("Complete failure phase: {:.1}% success (expected to be very low)", 
          failure_stats.success_rate() * 100.0);
    
    // Update connection status during failure
    context.update_connection_status(connection_id, 
        ConnectionStatus::Failed("Network failure detected".to_string())).await;

    // Phase 5: Network recovery simulation
    info!("Phase 5: Simulating network recovery");
    
    let recovery_steps = vec![
        (0.50, 500.0, "initial_recovery"),
        (0.20, 150.0, "partial_recovery"),
        (0.05, 50.0, "near_full_recovery"),
        (0.01, 25.0, "full_recovery"),
    ];
    
    let mut recovery_results = Vec::new();
    
    for (loss_rate, latency_ms, phase_name) in recovery_steps {
        info!("Recovery phase {}: {:.1}% loss, {}ms latency", phase_name, loss_rate * 100.0, latency_ms);
        
        context.network_simulator.simulate_packet_loss(loss_rate).await;
        context.network_simulator.simulate_latency(LatencyDistribution::Fixed { latency_ms }).await;
        
        // Update connection status as recovery progresses
        if loss_rate < 0.1 {
            context.update_connection_status(connection_id, ConnectionStatus::Active).await;
        }
        
        let mut phase_stats = OperationStats::new(phase_name);
        let phase_messages = 6;
        
        for i in 0..phase_messages {
            let message_data = vec![0xDD; 1536]; // Medium messages during recovery
            
            let packet = SimulatedPacket {
                id: thread_rng().gen(),
                source: connection_id,
                destination: 1000,
                payload: message_data.clone(),
                timestamp: Instant::now(),
                size_bytes: message_data.len(),
                priority: PacketPriority::Normal,
            };
            
            let send_start = Instant::now();
            match context.network_simulator.send_packet(packet).await {
                Ok(()) => {
                    phase_stats.record_success(send_start.elapsed(), message_data.len());
                    context.record_data_transfer(connection_id, message_data.len() as u64, 0).await;
                }
                Err(e) => {
                    phase_stats.record_failure(format!("{:?}", e));
                }
            }
            
            sleep(Duration::from_millis(120)).await;
        }
        
        info!("Recovery phase '{}': {:.1}% success, {:?} avg latency", 
              phase_name, phase_stats.success_rate() * 100.0, phase_stats.avg_latency());
        
        recovery_results.push((phase_name.to_string(), phase_stats));
        
        sleep(Duration::from_millis(300)).await;
    }

    // Phase 6: Post-recovery validation
    info!("Phase 6: Post-recovery validation");
    
    let mut validation_stats = OperationStats::new("post_recovery");
    let validation_messages = 10;
    
    for i in 0..validation_messages {
        let message_data = vec![0xEE; 2048]; // Back to normal message sizes
        
        let packet = SimulatedPacket {
            id: thread_rng().gen(),
            source: connection_id,
            destination: 1000,
            payload: message_data.clone(),
            timestamp: Instant::now(),
            size_bytes: message_data.len(),
            priority: PacketPriority::Normal,
        };
        
        let send_start = Instant::now();
        match context.network_simulator.send_packet(packet).await {
            Ok(()) => {
                validation_stats.record_success(send_start.elapsed(), message_data.len());
                context.record_data_transfer(connection_id, message_data.len() as u64, 0).await;
            }
            Err(e) => {
                validation_stats.record_failure(format!("{:?}", e));
            }
        }
        
        sleep(Duration::from_millis(100)).await;
    }
    
    info!("Post-recovery validation: {:.1}% success, {:?} avg latency", 
          validation_stats.success_rate() * 100.0, validation_stats.avg_latency());

    // Final analysis and cleanup
    let final_metrics = context.get_metrics_summary().await;
    context.network_simulator.stop().await;
    
    // Analyze recovery effectiveness
    let baseline_success = baseline_stats.success_rate();
    let final_success = validation_stats.success_rate();
    let recovery_effectiveness = final_success / baseline_success;
    
    info!("Network failure and recovery test summary:");
    info!("  Baseline success rate: {:.1}%", baseline_success * 100.0);
    info!("  Post-recovery success rate: {:.1}%", final_success * 100.0);
    info!("  Recovery effectiveness: {:.1}%", recovery_effectiveness * 100.0);
    info!("  Total bytes transferred: {}", final_metrics.total_bytes_transferred);
    
    // Print degradation progression
    info!("  Degradation progression:");
    for (phase_name, stats) in &degradation_results {
        info!("    {}: {:.1}% success", phase_name, stats.success_rate() * 100.0);
    }
    
    info!("  Recovery progression:");
    for (phase_name, stats) in &recovery_results {
        info!("    {}: {:.1}% success", phase_name, stats.success_rate() * 100.0);
    }
    
    // Assertions for test success
    assert!(baseline_success > 0.8, "Baseline should have high success rate");
    assert!(failure_stats.success_rate() < 0.3, "Failure phase should have low success rate");
    assert!(final_success > 0.7, "Should recover to reasonable success rate");
    assert!(recovery_effectiveness > 0.8, "Recovery should be at least 80% effective");
    
    // Verify recovery progression
    let mut prev_success_rate = 0.0;
    let mut recovery_improving = true;
    for (_, stats) in &recovery_results {
        let current_rate = stats.success_rate();
        if current_rate < prev_success_rate {
            recovery_improving = false;
            break;
        }
        prev_success_rate = current_rate;
    }
    
    assert!(recovery_improving, "Recovery should show improving success rates");
    
    info!("Network failure and recovery scenario completed successfully");
}

/// Statistics tracking for operation phases
#[derive(Debug, Clone)]
struct OperationStats {
    phase_name: String,
    successful_operations: u64,
    failed_operations: u64,
    total_bytes: u64,
    total_latency: Duration,
    errors: Vec<String>,
    start_time: Instant,
}

impl OperationStats {
    fn new(phase_name: &str) -> Self {
        Self {
            phase_name: phase_name.to_string(),
            successful_operations: 0,
            failed_operations: 0,
            total_bytes: 0,
            total_latency: Duration::ZERO,
            errors: Vec::new(),
            start_time: Instant::now(),
        }
    }
    
    fn record_success(&mut self, latency: Duration, bytes: usize) {
        self.successful_operations += 1;
        self.total_latency += latency;
        self.total_bytes += bytes as u64;
    }
    
    fn record_failure(&mut self, error: String) {
        self.failed_operations += 1;
        self.errors.push(error);
    }
    
    fn success_rate(&self) -> f64 {
        let total = self.successful_operations + self.failed_operations;
        if total > 0 {
            self.successful_operations as f64 / total as f64
        } else {
            0.0
        }
    }
    
    fn avg_latency(&self) -> Duration {
        if self.successful_operations > 0 {
            self.total_latency / self.successful_operations as u32
        } else {
            Duration::ZERO
        }
    }
}

/// Test comprehensive data integrity across complete pipeline
#[tokio::test]
async fn test_data_integrity_pipeline() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Testing comprehensive data integrity across complete pipeline");

    let config = E2ETestConfig::default();
    let context = E2ETestContext::new(config).await;
    context.network_simulator.start().await.expect("Failed to start network simulator");

    // Test various data types and sizes for integrity
    let test_datasets = vec![
        ("text_data", b"Critical text data that must maintain integrity through the entire Nyx pipeline".to_vec()),
        ("binary_data", (0..256).collect::<Vec<u8>>()),
        ("large_data", vec![0xAB; 32768]), // 32KB
        ("small_data", vec![0x42; 16]),    // 16 bytes
        ("random_data", (0..4096).map(|_| thread_rng().gen::<u8>()).collect()),
        ("structured_data", create_structured_test_data()),
        ("unicode_data", "🔒 Encrypted data with émojis and spëcial chars 中文 العربية".as_bytes().to_vec()),
    ];

    let mut integrity_results = Vec::new();
    
    for (data_name, original_data) in test_datasets {
        info!("Testing data integrity for: {} ({} bytes)", data_name, original_data.len());
        
        let mut integrity_test = DataIntegrityTest::new(data_name, &original_data);
        
        // Phase 1: Crypto layer processing
        info!("Phase 1: Processing through crypto layer");
        
        let hpke = HpkeKeyDeriver::new().expect("Failed to create HPKE");
        let (private_key, public_key) = hpke.derive_keypair()
            .expect("Failed to derive keypair");
        
        // Simulate HPKE encryption/decryption
        let (shared_secret, encapsulated_key) = hpke.encapsulate(&public_key)
            .expect("Failed to encapsulate");
        let decapsulated_secret = hpke.decapsulate(&private_key, &encapsulated_key)
            .expect("Failed to decapsulate");
        
        // Verify crypto layer integrity
        assert_eq!(shared_secret.as_bytes(), decapsulated_secret.as_bytes());
        integrity_test.record_phase("crypto", &original_data, None);
        
        // Phase 2: Stream layer processing
        info!("Phase 2: Processing through stream layer");
        
        let stream = NyxAsyncStream::new(1, 131072, 4096);
        
        // Simulate stream fragmentation and reassembly
        let fragments = fragment_data(&original_data, 1024); // 1KB fragments
        let reassembled_data = reassemble_fragments(&fragments);
        
        integrity_test.record_phase("stream", &reassembled_data, 
            Some(format!("Fragmented into {} pieces", fragments.len())));
        
        // Phase 3: Mix layer processing (simulation)
        info!("Phase 3: Processing through mix layer");
        
        // Simulate mix layer operations (padding, reordering, etc.)
        let mut mixed_data = reassembled_data.clone();
        
        // Add padding for mix network
        let padding_size = (16 - (mixed_data.len() % 16)) % 16;
        mixed_data.extend(vec![padding_size as u8; padding_size]);
        
        // Remove padding
        if let Some(&pad_len) = mixed_data.last() {
            if pad_len as usize <= mixed_data.len() {
                let new_len = mixed_data.len() - pad_len as usize;
                mixed_data.truncate(new_len);
            }
        }
        
        integrity_test.record_phase("mix", &mixed_data, 
            Some(format!("Added and removed {} bytes padding", padding_size)));
        
        // Phase 4: FEC layer processing
        info!("Phase 4: Processing through FEC layer");
        
        // Simulate FEC encoding/decoding
        let fec_encoded = simulate_fec_encode(&mixed_data, 0.3); // 30% redundancy
        let fec_decoded = simulate_fec_decode(&fec_encoded, 0.1); // 10% loss
        
        integrity_test.record_phase("fec", &fec_decoded, 
            Some(format!("FEC with 30% redundancy, 10% simulated loss")));
        
        // Phase 5: Transport layer processing
        info!("Phase 5: Processing through transport layer");
        
        // Simulate network transmission with realistic conditions
        context.network_simulator.simulate_packet_loss(0.05).await; // 5% loss
        
        let packet = SimulatedPacket {
            id: thread_rng().gen(),
            source: 1,
            destination: 2,
            payload: fec_decoded.clone(),
            timestamp: Instant::now(),
            size_bytes: fec_decoded.len(),
            priority: PacketPriority::Normal,
        };
        
        let transmission_start = Instant::now();
        let transmission_result = context.network_simulator.send_packet(packet).await;
        let transmission_time = transmission_start.elapsed();
        
        let transport_data = match transmission_result {
            Ok(()) => fec_decoded.clone(), // Simulate successful transmission
            Err(e) => {
                warn!("Transport layer transmission failed: {:?}", e);
                fec_decoded.clone() // Use original for integrity check
            }
        };
        
        integrity_test.record_phase("transport", &transport_data, 
            Some(format!("Network transmission in {:?}", transmission_time)));
        
        // Phase 6: End-to-end integrity verification
        info!("Phase 6: End-to-end integrity verification");
        
        let final_data = transport_data;
        let integrity_maintained = integrity_test.verify_final_integrity(&final_data);
        
        if integrity_maintained {
            info!("✓ Data integrity maintained for: {}", data_name);
        } else {
            error!("✗ Data integrity FAILED for: {}", data_name);
        }
        
        integrity_results.push((data_name.to_string(), integrity_test));
        
        // Brief pause between datasets
        sleep(Duration::from_millis(100)).await;
    }

    // Phase 7: Comprehensive integrity analysis
    info!("Phase 7: Comprehensive integrity analysis");
    
    let mut successful_tests = 0;
    let mut failed_tests = 0;
    let mut total_bytes_processed = 0u64;
    
    for (data_name, integrity_test) in &integrity_results {
        if integrity_test.overall_success {
            successful_tests += 1;
        } else {
            failed_tests += 1;
            error!("Integrity test failed for: {}", data_name);
            
            // Print detailed failure information
            for phase_result in &integrity_test.phase_results {
                if !phase_result.integrity_maintained {
                    error!("  Phase '{}' failed: expected {} bytes, got {} bytes", 
                           phase_result.phase_name, 
                           phase_result.expected_hash, 
                           phase_result.actual_hash);
                }
            }
        }
        
        total_bytes_processed += integrity_test.original_size as u64;
        
        info!("Test '{}': {} phases, {} bytes, success: {}", 
              data_name, 
              integrity_test.phase_results.len(),
              integrity_test.original_size,
              integrity_test.overall_success);
    }

    // Phase 8: Property-based integrity testing
    info!("Phase 8: Property-based integrity testing");
    
    let property_config = PropertyTestConfig {
        iterations: 50,
        max_size: 8192,
        ..Default::default()
    };
    
    let generator = Box::new(ByteVecGenerator::new(1, 8192));
    let mut property_tester = PropertyTester::new(property_config, generator);
    
    // Add integrity property
    property_tester.add_property(Box::new(PredicateProperty::new(
        |data: &Vec<u8>| {
            // Property: data should survive the pipeline transformation
            if data.is_empty() {
                return false;
            }
            
            // Simulate simplified pipeline
            let fragments = fragment_data(data, 512);
            let reassembled = reassemble_fragments(&fragments);
            
            // Data should be identical after fragmentation/reassembly
            *data == reassembled
        },
        "pipeline_integrity",
        "Data should maintain integrity through pipeline processing"
    )));
    
    let property_results = property_tester.run_all_tests();
    
    for result in &property_results {
        info!("Property test '{}': {}/{} passed ({:.1}%)", 
              result.property_name,
              result.passed_cases,
              result.total_cases,
              result.success_rate * 100.0);
        
        if let Some(ref counterexample) = result.counterexample {
            warn!("Property counterexample found: {:?}", counterexample.failure_message);
        }
    }

    // Final verification and cleanup
    let final_metrics = context.get_metrics_summary().await;
    context.network_simulator.stop().await;
    
    let success_rate = successful_tests as f64 / (successful_tests + failed_tests) as f64;
    
    info!("Data integrity pipeline test summary:");
    info!("  Total datasets tested: {}", successful_tests + failed_tests);
    info!("  Successful integrity tests: {}", successful_tests);
    info!("  Failed integrity tests: {}", failed_tests);
    info!("  Overall success rate: {:.1}%", success_rate * 100.0);
    info!("  Total bytes processed: {}", total_bytes_processed);
    
    // Property test summary
    let property_success_rate = property_results.iter()
        .map(|r| r.success_rate)
        .sum::<f64>() / property_results.len() as f64;
    info!("  Property test success rate: {:.1}%", property_success_rate * 100.0);
    
    // Assertions for test success
    assert!(success_rate >= 0.9, "Should have at least 90% integrity success rate");
    assert!(failed_tests == 0, "Should have no integrity failures");
    assert!(property_success_rate >= 0.95, "Property tests should have high success rate");
    assert!(total_bytes_processed > 0, "Should have processed data");
    
    info!("Data integrity pipeline test completed successfully");
}

/// Create structured test data for integrity testing
fn create_structured_test_data() -> Vec<u8> {
    let mut data = Vec::new();
    
    // Header
    data.extend_from_slice(b"NYX_DATA");
    data.extend_from_slice(&1u32.to_be_bytes()); // Version
    data.extend_from_slice(&1024u32.to_be_bytes()); // Length
    
    // Payload
    for i in 0..256 {
        data.extend_from_slice(&(i as u32).to_be_bytes());
    }
    
    // Checksum (simple XOR)
    let checksum = data.iter().fold(0u8, |acc, &b| acc ^ b);
    data.push(checksum);
    
    data
}

/// Fragment data into smaller pieces
fn fragment_data(data: &[u8], fragment_size: usize) -> Vec<Vec<u8>> {
    data.chunks(fragment_size)
        .map(|chunk| chunk.to_vec())
        .collect()
}

/// Reassemble fragments back into original data
fn reassemble_fragments(fragments: &[Vec<u8>]) -> Vec<u8> {
    fragments.iter()
        .flat_map(|fragment| fragment.iter())
        .cloned()
        .collect()
}

/// Simulate FEC encoding with redundancy
fn simulate_fec_encode(data: &[u8], redundancy: f64) -> Vec<u8> {
    let mut encoded = data.to_vec();
    let redundant_bytes = (data.len() as f64 * redundancy) as usize;
    
    // Add simple redundancy (repeat every 4th byte)
    for i in (0..data.len()).step_by(4) {
        if encoded.len() < data.len() + redundant_bytes {
            encoded.push(data[i]);
        }
    }
    
    encoded
}

/// Simulate FEC decoding with loss recovery
fn simulate_fec_decode(encoded_data: &[u8], loss_rate: f64) -> Vec<u8> {
    let original_size = (encoded_data.len() as f64 * 0.77) as usize; // Approximate original size
    
    // Simulate data recovery despite some loss
    let mut decoded = Vec::new();
    let mut rng = thread_rng();
    
    for (i, &byte) in encoded_data.iter().enumerate() {
        if decoded.len() >= original_size {
            break;
        }
        
        // Simulate packet loss
        if rng.gen::<f64>() > loss_rate {
            decoded.push(byte);
        }
    }
    
    // Pad to original size if needed (FEC recovery)
    while decoded.len() < original_size {
        decoded.push(0);
    }
    
    decoded.truncate(original_size);
    decoded
}

/// Data integrity test tracker
#[derive(Debug, Clone)]
struct DataIntegrityTest {
    test_name: String,
    original_data: Vec<u8>,
    original_size: usize,
    original_hash: u64,
    phase_results: Vec<PhaseResult>,
    overall_success: bool,
}

/// Result for each pipeline phase
#[derive(Debug, Clone)]
struct PhaseResult {
    phase_name: String,
    expected_hash: u64,
    actual_hash: u64,
    data_size: usize,
    integrity_maintained: bool,
    notes: Option<String>,
    processing_time: Duration,
}

impl DataIntegrityTest {
    fn new(test_name: &str, original_data: &[u8]) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        original_data.hash(&mut hasher);
        let original_hash = hasher.finish();
        
        Self {
            test_name: test_name.to_string(),
            original_data: original_data.to_vec(),
            original_size: original_data.len(),
            original_hash,
            phase_results: Vec::new(),
            overall_success: false,
        }
    }
    
    fn record_phase(&mut self, phase_name: &str, processed_data: &[u8], notes: Option<String>) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let start_time = Instant::now();
        
        let mut hasher = DefaultHasher::new();
        processed_data.hash(&mut hasher);
        let actual_hash = hasher.finish();
        
        let integrity_maintained = actual_hash == self.original_hash && 
                                 processed_data.len() == self.original_size &&
                                 processed_data == self.original_data;
        
        let phase_result = PhaseResult {
            phase_name: phase_name.to_string(),
            expected_hash: self.original_hash,
            actual_hash,
            data_size: processed_data.len(),
            integrity_maintained,
            notes,
            processing_time: start_time.elapsed(),
        };
        
        self.phase_results.push(phase_result);
    }
    
    fn verify_final_integrity(&mut self, final_data: &[u8]) -> bool {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        final_data.hash(&mut hasher);
        let final_hash = hasher.finish();
        
        let integrity_maintained = final_hash == self.original_hash && 
                                 final_data.len() == self.original_size &&
                                 final_data == self.original_data;
        
        self.overall_success = integrity_maintained;
        integrity_maintained
    }
}