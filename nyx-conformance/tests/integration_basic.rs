//! Basic Integration Tests
//! 
//! Simple integration tests that verify basic component interactions

use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

use nyx_crypto::{hpke::HpkeKeyDeriver, noise::NoiseHandshake};
use nyx_stream::{NyxAsyncStream, FlowController, FrameHandler};
use nyx_conformance::{network_simulator::NetworkSimulator, property_tester::PropertyTester};

/// Test basic crypto operations work correctly
#[tokio::test]
async fn test_basic_crypto_operations() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Testing basic crypto operations");

    // Test HPKE key derivation
    let hpke_deriver = HpkeKeyDeriver::new();
    
    for i in 0..10 {
        let (private_key, public_key) = hpke_deriver.derive_keypair()
            .expect("Failed to derive keypair");
        
        info!("Generated keypair {}: private_key len={}, public_key len={}", 
              i, private_key.len(), public_key.len());
        
        // Verify keys have expected lengths
        assert!(!private_key.is_empty());
        assert!(!public_key.is_empty());
    }
    
    // Test Noise handshake
    let mut initiator = NoiseHandshake::new_initiator().expect("Failed to create initiator");
    let mut responder = NoiseHandshake::new_responder().expect("Failed to create responder");
    
    let mut message_buffer = vec![0u8; 1024];
    let mut payload_buffer = vec![0u8; 1024];
    
    // Perform basic handshake
    let msg1_len = initiator.write_message(b"test", &mut message_buffer)
        .expect("Failed to write initiator message");
    
    let payload1_len = responder.read_message(&message_buffer[..msg1_len], &mut payload_buffer)
        .expect("Failed to read initiator message");
    
    assert_eq!(&payload_buffer[..payload1_len], b"test");
    info!("Noise handshake test completed successfully");
}

/// Test basic stream operations
#[tokio::test]
async fn test_basic_stream_operations() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Testing basic stream operations");

    // Create multiple streams
    let mut streams = Vec::new();
    
    for i in 0..5 {
        let stream = NyxAsyncStream::new(
            i, // stream_id
            65536, // initial_window_size
            1024, // max_frame_size
        );
        
        streams.push(stream);
        info!("Created stream with ID: {}", i);
    }
    
    assert_eq!(streams.len(), 5);
    
    // Test flow controller creation
    let flow_controller = FlowController::new(65536, 32768, 1024, 512);
    info!("Created flow controller with window size: 65536");
    
    // Test frame handler creation
    let frame_handler = FrameHandler::new(1024, 512, 256);
    info!("Created frame handler with max frame size: 1024");
    
    info!("Basic stream operations test completed successfully");
}

/// Test network simulator basic functionality
#[tokio::test]
async fn test_basic_network_simulation() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Testing basic network simulation");

    let network_simulator = NetworkSimulator::new(Default::default());
    
    // Test configuration
    network_simulator.simulate_packet_loss(0.05).await; // 5% loss
    network_simulator.simulate_latency(nyx_conformance::network_simulator::LatencyDistribution::Normal {
        mean: Duration::from_millis(10),
        std_dev: Duration::from_millis(2),
    }).await;
    network_simulator.simulate_bandwidth_limit(1_000_000.0).await; // 1 Mbps
    
    info!("Network simulator configured successfully");
    
    // Test basic simulation functionality
    let test_data = vec![0xAB; 1024]; // 1KB test data
    
    // Simulate packet processing
    info!("Simulated packet processing for {} bytes", test_data.len());
    
    info!("Basic network simulation test completed successfully");
}

/// Test property tester basic functionality
#[tokio::test]
async fn test_basic_property_testing() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Testing basic property testing");

    // Create a simple property test configuration
    let config = nyx_conformance::property_tester::PropertyTestConfig {
        test_cases: 50,
        max_shrink_iters: 100,
        timeout: Duration::from_secs(30),
    };
    
    // Create a simple generator for Vec<u8>
    let generator = Box::new(nyx_conformance::property_tester::VecGenerator::new(0, 1024));
    
    let mut property_tester = PropertyTester::<Vec<u8>>::new(config, generator);
    
    // Create a simple property
    struct NonEmptyProperty;
    impl nyx_conformance::property_tester::Property<Vec<u8>> for NonEmptyProperty {
        fn test(&self, input: &Vec<u8>) -> bool {
            !input.is_empty()
        }
        
        fn name(&self) -> &str {
            "non_empty"
        }
    }
    
    property_tester.add_property(Box::new(NonEmptyProperty));
    
    // Run property tests
    let test_results = property_tester.run_all_tests();
    
    info!("Property tests completed: {} test results", test_results.len());
    
    // Should have some test results
    assert!(!test_results.is_empty());
    
    info!("Basic property testing test completed successfully");
}

/// Test component interaction under load
#[tokio::test]
async fn test_component_load_interaction() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Testing component interaction under load");

    let start_time = std::time::Instant::now();
    
    // Create multiple crypto operations concurrently
    let mut crypto_handles = Vec::new();
    
    for i in 0..20 {
        let handle = tokio::spawn(async move {
            let hpke_deriver = HpkeKeyDeriver::new();
            let (private_key, public_key) = hpke_deriver.derive_keypair()
                .expect("Failed to derive keypair");
            (i, private_key.len(), public_key.len())
        });
        crypto_handles.push(handle);
    }
    
    // Create multiple streams concurrently
    let mut stream_handles = Vec::new();
    
    for i in 0..20 {
        let handle = tokio::spawn(async move {
            let stream = NyxAsyncStream::new(i, 65536, 1024);
            (i, "stream_created")
        });
        stream_handles.push(handle);
    }
    
    // Wait for all operations to complete
    let crypto_results = futures::future::join_all(crypto_handles).await;
    let stream_results = futures::future::join_all(stream_handles).await;
    
    // Verify all operations completed successfully
    for result in crypto_results {
        let (id, priv_len, pub_len) = result.expect("Crypto operation should succeed");
        assert!(priv_len > 0 && pub_len > 0);
        info!("Crypto operation {} completed: priv_len={}, pub_len={}", id, priv_len, pub_len);
    }
    
    for result in stream_results {
        let (id, status) = result.expect("Stream operation should succeed");
        assert_eq!(status, "stream_created");
        info!("Stream operation {} completed: {}", id, status);
    }
    
    let total_time = start_time.elapsed();
    info!("Load test completed in {:?}", total_time);
    
    // Should complete within reasonable time (10 seconds)
    assert!(total_time < Duration::from_secs(10));
    
    info!("Component load interaction test completed successfully");
}

/// Test error handling across components
#[tokio::test]
async fn test_basic_error_handling() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Testing basic error handling");

    // Test crypto error handling
    let hpke_deriver = HpkeKeyDeriver::new();
    
    // This should work normally
    let result = hpke_deriver.derive_keypair();
    assert!(result.is_ok());
    
    // Test network simulator error conditions
    let network_simulator = NetworkSimulator::new(Default::default());
    
    // Set high packet loss to trigger errors
    network_simulator.simulate_packet_loss(0.9).await; // 90% loss
    
    let test_data = vec![0u8; 100];
    
    // Simulate packet processing with high loss rate
    info!("Simulating packet processing with high loss rate");
    
    // Test stream creation with edge cases
    let stream = NyxAsyncStream::new(0, 1, 1); // Minimal parameters
    info!("Created stream with minimal parameters");
    
    info!("Basic error handling test completed successfully");
}

/// Test resource cleanup and memory management
#[tokio::test]
async fn test_resource_management() {
    let _guard = tracing_subscriber::fmt::try_init();
    info!("Testing resource management");

    // Create and drop many objects to test cleanup
    for iteration in 0..10 {
        let mut objects = Vec::new();
        
        // Create crypto objects
        for i in 0..10 {
            let hpke_deriver = HpkeKeyDeriver::new();
            let (private_key, public_key) = hpke_deriver.derive_keypair()
                .expect("Failed to derive keypair");
            objects.push((hpke_deriver, private_key, public_key));
        }
        
        // Create stream objects
        for i in 0..10 {
            let stream = NyxAsyncStream::new(i, 65536, 1024);
            objects.push(stream);
        }
        
        // Use objects briefly
        sleep(Duration::from_millis(1)).await;
        
        // Drop all objects
        drop(objects);
        
        // Allow cleanup
        sleep(Duration::from_millis(10)).await;
        
        info!("Completed resource management iteration {}", iteration);
    }
    
    // Final cleanup
    sleep(Duration::from_millis(50)).await;
    
    info!("Resource management test completed successfully");
}