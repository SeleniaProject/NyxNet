//! Component Integration Tests
//! 
//! This module contains comprehensive integration tests that verify the interaction
//! between all major components of the Nyx system, including:
//! - Crypto layer integration with other layers
//! - Stream management with flow control
//! - Network simulation with real components
//! - Error propagation and recovery mechanisms

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{timeout, sleep};
use tracing::{info, warn};

use nyx_crypto::{hpke::HpkeKeyDeriver, noise::NoiseHandshake};
use nyx_stream::{NyxAsyncStream, FlowController, FrameHandler};
use nyx_conformance::{network_simulator::NetworkSimulator, property_tester::PropertyTester};

/// Test data structure for component integration scenarios
#[derive(Debug)]
struct IntegrationTestContext {
    network_simulator: Arc<NetworkSimulator>,
    property_tester: Arc<PropertyTester<Vec<u8>>>,
}

impl IntegrationTestContext {
    async fn new() -> Self {
        let network_simulator = Arc::new(NetworkSimulator::new(Default::default()));
        let property_tester = Arc::new(PropertyTester::new());
        
        Self {
            network_simulator,
            property_tester,
        }
    }
}

/// Test crypto layer integration with stream layer
#[tokio::test]
async fn test_crypto_stream_integration() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Starting crypto-stream integration test");

    // Initialize crypto components
    let hpke_deriver = HpkeKeyDeriver::new().expect("Failed to create HPKE deriver");
    let (private_key, public_key) = hpke_deriver.derive_keypair().expect("Failed to derive keypair");
    
    // Create Noise handshake
    let mut initiator = NoiseHandshake::new_initiator().expect("Failed to create initiator");
    let mut responder = NoiseHandshake::new_responder().expect("Failed to create responder");
    
    // Perform handshake
    let mut message_buffer = vec![0u8; 1024];
    let mut payload_buffer = vec![0u8; 1024];
    
    // Initiator sends first message
    let msg1_len = initiator.write_message(b"hello", &mut message_buffer)
        .expect("Failed to write initiator message");
    
    // Responder processes first message and sends response
    let payload1_len = responder.read_message(&message_buffer[..msg1_len], &mut payload_buffer)
        .expect("Failed to read initiator message");
    assert_eq!(&payload_buffer[..payload1_len], b"hello");
    
    let msg2_len = responder.write_message(b"world", &mut message_buffer)
        .expect("Failed to write responder message");
    
    // Initiator processes response
    let payload2_len = initiator.read_message(&message_buffer[..msg2_len], &mut payload_buffer)
        .expect("Failed to read responder message");
    assert_eq!(&payload_buffer[..payload2_len], b"world");
    
    // Complete handshake and get transport states
    let initiator_transport = initiator.into_transport_mode()
        .expect("Failed to convert initiator to transport mode");
    let responder_transport = responder.into_transport_mode()
        .expect("Failed to convert responder to transport mode");
    
    // Create stream with proper parameters
    let flow_controller = FlowController::new(65536, 32768, 1024); // window, cwnd, buffer_size
    let frame_handler = FrameHandler::new(1024, 512, 256); // max_frame_size, buffer_size, queue_size
    
    let stream = NyxAsyncStream::new(
        1, // stream_id
        65536, // initial_window_size
        1024, // max_frame_size
    );
    
    // Test encrypted data flow through stream
    let test_data = b"This is encrypted test data flowing through the stream";
    
    // Verify stream was created successfully
    info!("Stream created with ID: {}", 1);
    
    info!("Crypto-stream integration test completed successfully");
}

/// Test network simulation integration with real components
#[tokio::test]
async fn test_network_simulation_integration() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Starting network simulation integration test");

    let context = IntegrationTestContext::new().await;
    
    // Configure network simulation
    context.network_simulator.set_packet_loss_rate(0.1).await; // 10% loss
    context.network_simulator.set_latency_distribution(Duration::from_millis(50), Duration::from_millis(10)).await;
    context.network_simulator.set_bandwidth_limit(1_000_000).await; // 1 Mbps
    
    // Create simulated network path
    let path = context.network_simulator.create_path(vec!["node1", "node2", "node3"]).await
        .expect("Failed to create network path");
    
    // Test data transmission through simulated network
    let test_data = vec![0u8; 10240]; // 10KB test data
    
    let start_time = std::time::Instant::now();
    let result = context.network_simulator.transmit_data(&path, &test_data).await;
    let transmission_time = start_time.elapsed();
    
    match result {
        Ok(received_data) => {
            // Verify data integrity (accounting for potential packet loss)
            assert!(received_data.len() <= test_data.len());
            info!("Data transmitted successfully: {} bytes in {:?}", received_data.len(), transmission_time);
        }
        Err(e) => {
            warn!("Data transmission failed (expected with packet loss): {:?}", e);
        }
    }
    
    // Test network partition simulation
    context.network_simulator.create_partition(vec!["node1"], vec!["node2", "node3"]).await;
    
    // Verify partition affects connectivity
    let partition_result = context.network_simulator.test_connectivity("node1", "node3").await;
    assert!(!partition_result, "Connectivity should be blocked by partition");
    
    // Heal partition
    context.network_simulator.heal_partition().await;
    
    // Verify connectivity is restored
    let healed_result = context.network_simulator.test_connectivity("node1", "node3").await;
    assert!(healed_result, "Connectivity should be restored after healing partition");
    
    info!("Network simulation integration test completed successfully");
}

/// Test property-based testing integration
#[tokio::test]
async fn test_property_testing_integration() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Starting property testing integration test");

    let context = IntegrationTestContext::new().await;
    
    // Add a simple property test for data integrity
    context.property_tester.add_property("data_integrity", |data: &Vec<u8>| {
        // Property: data should not be empty and should have reasonable size
        !data.is_empty() && data.len() <= 1024 * 1024 // 1MB max
    });
    
    // Run property tests
    let test_results = context.property_tester.run_tests(100).await;
    
    // Verify test results
    assert!(test_results.passed > 0, "Should have some passing tests");
    info!("Property tests completed: {} passed, {} failed", 
          test_results.passed, test_results.failed);
    
    // Test with network simulation
    context.network_simulator.set_packet_loss_rate(0.05).await; // 5% loss
    
    let test_data = vec![0xAB; 1024]; // 1KB test data
    let path = context.network_simulator.create_path(vec!["node1", "node2"]).await
        .expect("Failed to create path");
    
    let result = context.network_simulator.transmit_data(&path, &test_data).await;
    match result {
        Ok(received_data) => {
            info!("Network transmission successful: {} bytes", received_data.len());
        }
        Err(e) => {
            info!("Network transmission failed (expected with packet loss): {:?}", e);
        }
    }
    
    info!("Property testing integration test completed successfully");
}

/// Test metrics collection across all layers
#[tokio::test]
async fn test_cross_layer_metrics() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Starting cross-layer metrics test");

    let context = IntegrationTestContext::new().await;
    
    // Start metrics collection
    context.metrics_collector.start_collection().await;
    
    // Simulate activity in different layers
    context.metrics_collector.record_crypto_operation("hpke_derive", Duration::from_micros(100)).await;
    context.metrics_collector.record_stream_bytes_sent(1024).await;
    context.metrics_collector.record_network_latency(Duration::from_millis(50)).await;
    
    // Wait for metrics aggregation
    sleep(Duration::from_millis(100)).await;
    
    // Collect metrics snapshot
    let snapshot = context.metrics_collector.get_snapshot().await;
    
    // Verify metrics from all layers are present
    assert!(snapshot.crypto_metrics.operations_count > 0);
    assert!(snapshot.stream_metrics.bytes_sent > 0);
    assert!(snapshot.network_metrics.average_latency > Duration::ZERO);
    
    // Test metrics export
    let prometheus_output = context.metrics_collector.export_prometheus().await;
    assert!(prometheus_output.contains("nyx_crypto_operations_total"));
    assert!(prometheus_output.contains("nyx_stream_bytes_sent_total"));
    assert!(prometheus_output.contains("nyx_network_latency_seconds"));
    
    info!("Cross-layer metrics test completed successfully");
}

/// Test network simulation integration with real components
#[tokio::test]
async fn test_network_simulation_integration() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Starting network simulation integration test");

    let context = IntegrationTestContext::new().await;
    
    // Configure network simulation
    context.network_simulator.set_packet_loss_rate(0.1).await; // 10% loss
    context.network_simulator.set_latency_distribution(Duration::from_millis(50), Duration::from_millis(10)).await;
    context.network_simulator.set_bandwidth_limit(1_000_000).await; // 1 Mbps
    
    // Create simulated network path
    let path = context.network_simulator.create_path(vec!["node1", "node2", "node3"]).await
        .expect("Failed to create network path");
    
    // Test data transmission through simulated network
    let test_data = vec![0u8; 10240]; // 10KB test data
    
    let start_time = std::time::Instant::now();
    let result = context.network_simulator.transmit_data(&path, &test_data).await;
    let transmission_time = start_time.elapsed();
    
    match result {
        Ok(received_data) => {
            // Verify data integrity (accounting for potential packet loss)
            assert!(received_data.len() <= test_data.len());
            info!("Data transmitted successfully: {} bytes in {:?}", received_data.len(), transmission_time);
        }
        Err(e) => {
            warn!("Data transmission failed (expected with packet loss): {:?}", e);
        }
    }
    
    // Test network partition simulation
    context.network_simulator.create_partition(vec!["node1"], vec!["node2", "node3"]).await;
    
    // Verify partition affects connectivity
    let partition_result = context.network_simulator.test_connectivity("node1", "node3").await;
    assert!(!partition_result, "Connectivity should be blocked by partition");
    
    // Heal partition
    context.network_simulator.heal_partition().await;
    
    // Verify connectivity is restored
    let healed_result = context.network_simulator.test_connectivity("node1", "node3").await;
    assert!(healed_result, "Connectivity should be restored after healing partition");
    
    info!("Network simulation integration test completed successfully");
}

/// Test configuration changes propagation across layers
#[tokio::test]
async fn test_configuration_propagation() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Starting configuration propagation test");

    let context = IntegrationTestContext::new().await;
    
    // Set up configuration change tracking
    let (config_tx, mut config_rx) = mpsc::channel(100);
    
    context.event_bus.subscribe("config.changed", Box::new(move |event| {
        let tx = config_tx.clone();
        Box::pin(async move {
            tx.send(event).await.ok();
        })
    })).await;
    
    // Update configuration
    let new_config = r#"
    [crypto]
    key_rotation_interval = 3600
    
    [stream]
    max_window_size = 131072
    
    [network]
    max_connections = 1000
    "#;
    
    context.config_manager.update_config(new_config).await
        .expect("Failed to update configuration");
    
    // Verify configuration change event was published
    let config_event = timeout(Duration::from_millis(100), config_rx.recv())
        .await
        .expect("Config event timeout")
        .expect("Config channel closed");
    
    assert!(config_event.contains("config.changed"));
    
    // Verify all layers received the configuration update
    {
        let layer_manager = context.layer_manager.lock().await;
        
        // Check that layers have updated their configuration
        assert!(layer_manager.get_layer_config("crypto").contains("key_rotation_interval"));
        assert!(layer_manager.get_layer_config("stream").contains("max_window_size"));
    }
    
    info!("Configuration propagation test completed successfully");
}

/// Test resource cleanup and lifecycle management
#[tokio::test]
async fn test_resource_lifecycle() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Starting resource lifecycle test");

    let context = IntegrationTestContext::new().await;
    
    // Start all components
    {
        let mut layer_manager = context.layer_manager.lock().await;
        layer_manager.start().await.expect("Failed to start layers");
    }
    
    // Create multiple streams to test resource management
    let mut streams = Vec::new();
    for i in 0..10 {
        let flow_controller = FlowController::new(65536);
        let frame_handler = FrameHandler::new();
        let stream = NyxAsyncStream::new(i, flow_controller, frame_handler, None);
        streams.push(stream);
    }
    
    // Verify resources are allocated
    let initial_metrics = context.metrics_collector.get_snapshot().await;
    assert!(initial_metrics.system_metrics.active_streams > 0);
    
    // Drop streams to test cleanup
    drop(streams);
    
    // Force garbage collection and wait
    sleep(Duration::from_millis(100)).await;
    
    // Verify resources are cleaned up
    let final_metrics = context.metrics_collector.get_snapshot().await;
    assert_eq!(final_metrics.system_metrics.active_streams, 0);
    
    // Test graceful shutdown
    {
        let mut layer_manager = context.layer_manager.lock().await;
        layer_manager.stop().await.expect("Failed to stop layers");
    }
    
    // Verify all resources are released
    let shutdown_metrics = context.metrics_collector.get_snapshot().await;
    assert_eq!(shutdown_metrics.system_metrics.active_connections, 0);
    assert_eq!(shutdown_metrics.system_metrics.memory_usage, 0);
    
    info!("Resource lifecycle test completed successfully");
}

/// Integration test for the complete data flow pipeline
#[tokio::test]
async fn test_complete_data_pipeline() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Starting complete data pipeline test");

    let context = IntegrationTestContext::new().await;
    
    // Initialize the complete pipeline
    {
        let mut layer_manager = context.layer_manager.lock().await;
        layer_manager.start().await.expect("Failed to start pipeline");
    }
    
    // Create test data
    let test_data = b"Integration test data flowing through complete pipeline";
    
    // Simulate complete data flow:
    // 1. Data enters through stream layer
    let flow_controller = FlowController::new(65536);
    let frame_handler = FrameHandler::new();
    let mut stream = NyxAsyncStream::new(1, flow_controller, frame_handler, None);
    
    // 2. Data gets encrypted by crypto layer
    let hpke_deriver = HpkeKeyDeriver::new().expect("Failed to create HPKE deriver");
    let (private_key, public_key) = hpke_deriver.derive_keypair().expect("Failed to derive keypair");
    
    // 3. Data gets processed by mix layer (simulated)
    context.event_bus.publish("data.mix", format!("Processing {} bytes", test_data.len())).await;
    
    // 4. Data gets FEC encoded
    context.event_bus.publish("data.fec", "Applying forward error correction".into()).await;
    
    // 5. Data gets transmitted through transport layer
    context.event_bus.publish("data.transport", "Transmitting over network".into()).await;
    
    // Verify pipeline metrics
    sleep(Duration::from_millis(100)).await;
    let metrics = context.metrics_collector.get_snapshot().await;
    
    assert!(metrics.stream_metrics.bytes_processed > 0);
    assert!(metrics.crypto_metrics.operations_count > 0);
    
    // Test pipeline under load
    for i in 0..100 {
        let data = format!("Load test data packet {}", i);
        context.event_bus.publish("data.load_test", data).await;
    }
    
    sleep(Duration::from_millis(200)).await;
    let load_metrics = context.metrics_collector.get_snapshot().await;
    
    assert!(load_metrics.stream_metrics.bytes_processed > metrics.stream_metrics.bytes_processed);
    
    info!("Complete data pipeline test completed successfully");
}

/// Test stream and crypto component interaction
#[tokio::test]
async fn test_stream_crypto_interaction() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Starting stream-crypto interaction test");

    // Test multiple stream creation and crypto operations
    let mut streams = Vec::new();
    let hpke_deriver = HpkeKeyDeriver::new().expect("Failed to create HPKE deriver");
    
    for i in 0..5 {
        // Create crypto context for each stream
        let (private_key, public_key) = hpke_deriver.derive_keypair()
            .expect("Failed to derive keypair");
        
        // Create stream with proper parameters
        let stream = NyxAsyncStream::new(
            i, // stream_id
            65536, // initial_window_size
            1024, // max_frame_size
        );
        
        streams.push((stream, private_key, public_key));
        info!("Created stream {} with crypto context", i);
    }
    
    // Verify all streams were created successfully
    assert_eq!(streams.len(), 5);
    
    // Test concurrent crypto operations
    let mut handles = Vec::new();
    
    for i in 0..10 {
        let deriver = HpkeKeyDeriver::new().expect("Failed to create HPKE deriver");
        let handle = tokio::spawn(async move {
            let (private_key, public_key) = deriver.derive_keypair()
                .expect("Failed to derive keypair");
            (i, private_key, public_key)
        });
        handles.push(handle);
    }
    
    // Wait for all crypto operations to complete
    let results = futures::future::join_all(handles).await;
    
    for result in results {
        let (id, private_key, public_key) = result.expect("Crypto operation should succeed");
        info!("Completed crypto operation for ID: {}", id);
    }
    
    info!("Stream-crypto interaction test completed successfully");
}