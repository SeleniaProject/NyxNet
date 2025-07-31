#![cfg(test)]

//! Comprehensive tests for DHT peer discovery and path building functionality.
//! 
//! This test suite validates:
//! - Real DHT integration with libp2p Kademlia
//! - Peer discovery with various criteria
//! - Peer information caching and persistence
//! - Path building quality and reliability
//! - Error handling and edge cases

use super::*;
use tokio::time::timeout;
use std::time::Duration;

/// Test DHT peer discovery initialization
#[tokio::test]
async fn test_dht_peer_discovery_initialization() {
    let bootstrap_peers = vec![
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWGrfVU2HUd65qGM9jdnvYTfL5e4DjX1FgZm8R3S4X5K6Z".to_string(),
        "/ip4/127.0.0.1/tcp/4002/p2p/12D3KooWFD6n2tMVCDWKZJF3qPzMSK7nDHn1XSqWb8R3S4X5K7A".to_string(),
    ];
    
    let result = DhtPeerDiscovery::new(bootstrap_peers).await;
    assert!(result.is_ok(), "DHT initialization should succeed with valid bootstrap peers");
    
    let discovery = result.unwrap();
    assert!(!discovery.bootstrap_peers.is_empty(), "Bootstrap peers should be configured");
}

/// Test DHT peer discovery with invalid bootstrap peers
#[tokio::test]
async fn test_dht_peer_discovery_invalid_bootstrap() {
    let invalid_bootstrap_peers = vec![];
    
    let result = DhtPeerDiscovery::new(invalid_bootstrap_peers).await;
    assert!(result.is_err(), "DHT initialization should fail with no bootstrap peers");
    
    match result.unwrap_err() {
        DhtError::BootstrapFailed => {}, // Expected error
        other => panic!("Expected BootstrapFailed error, got: {:?}", other),
    }
}

/// Test peer discovery by region
#[tokio::test]
async fn test_peer_discovery_by_region() {
    let bootstrap_peers = vec![
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWGrfVU2HUd65qGM9jdnvYTfL5e4DjX1FgZm8R3S4X5K6Z".to_string(),
    ];
    
    let mut discovery = DhtPeerDiscovery::new(bootstrap_peers).await
        .expect("DHT initialization should succeed");
    
    // Test region-based discovery
    let peers = discovery.discover_peers(DiscoveryCriteria::ByRegion("us-east".to_string())).await;
    assert!(peers.is_ok(), "Regional peer discovery should succeed");
    
    let peers = peers.unwrap();
    assert!(!peers.is_empty(), "Should discover peers in us-east region");
    
    // Verify all peers are from the correct region
    for peer in &peers {
        assert_eq!(peer.region, "us-east", "All peers should be from the requested region");
    }
}

/// Test peer discovery by capability
#[tokio::test]
async fn test_peer_discovery_by_capability() {
    let bootstrap_peers = vec![
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWGrfVU2HUd65qGM9jdnvYTfL5e4DjX1FgZm8R3S4X5K6Z".to_string(),
    ];
    
    let mut discovery = DhtPeerDiscovery::new(bootstrap_peers).await
        .expect("DHT initialization should succeed");
    
    // Test capability-based discovery
    let peers = discovery.discover_peers(DiscoveryCriteria::ByCapability("mix-node".to_string())).await;
    assert!(peers.is_ok(), "Capability-based peer discovery should succeed");
    
    let peers = peers.unwrap();
    assert!(!peers.is_empty(), "Should discover peers with mix-node capability");
}

/// Test peer discovery by latency threshold
#[tokio::test]
async fn test_peer_discovery_by_latency() {
    let bootstrap_peers = vec![
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWGrfVU2HUd65qGM9jdnvYTfL5e4DjX1FgZm8R3S4X5K6Z".to_string(),
    ];
    
    let mut discovery = DhtPeerDiscovery::new(bootstrap_peers).await
        .expect("DHT initialization should succeed");
    
    // Test latency-based discovery
    let max_latency = 100.0; // 100ms
    let peers = discovery.discover_peers(DiscoveryCriteria::ByLatency(max_latency)).await;
    assert!(peers.is_ok(), "Latency-based peer discovery should succeed");
    
    let peers = peers.unwrap();
    // Verify all peers meet latency requirements
    for peer in &peers {
        assert!(peer.latency_ms <= max_latency, 
               "Peer latency {}ms should be <= {}ms", peer.latency_ms, max_latency);
    }
}

/// Test random peer discovery
#[tokio::test]
async fn test_peer_discovery_random() {
    let bootstrap_peers = vec![
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWGrfVU2HUd65qGM9jdnvYTfL5e4DjX1FgZm8R3S4X5K6Z".to_string(),
    ];
    
    let mut discovery = DhtPeerDiscovery::new(bootstrap_peers).await
        .expect("DHT initialization should succeed");
    
    // Test random peer discovery
    let requested_count = 5;
    let peers = discovery.discover_peers(DiscoveryCriteria::Random(requested_count)).await;
    assert!(peers.is_ok(), "Random peer discovery should succeed");
    
    let peers = peers.unwrap();
    assert!(!peers.is_empty(), "Should discover some random peers");
    assert!(peers.len() <= requested_count, "Should not exceed requested count");
}

/// Test peer discovery caching
#[tokio::test]
async fn test_peer_discovery_caching() {
    let bootstrap_peers = vec![
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWGrfVU2HUd65qGM9jdnvYTfL5e4DjX1FgZm8R3S4X5K6Z".to_string(),
    ];
    
    let mut discovery = DhtPeerDiscovery::new(bootstrap_peers).await
        .expect("DHT initialization should succeed");
    
    let criteria = DiscoveryCriteria::ByRegion("us-west".to_string());
    
    // First discovery - should hit DHT
    let start_time = std::time::Instant::now();
    let peers1 = discovery.discover_peers(criteria.clone()).await
        .expect("First discovery should succeed");
    let first_duration = start_time.elapsed();
    
    // Second discovery - should hit cache
    let start_time = std::time::Instant::now();
    let peers2 = discovery.discover_peers(criteria).await
        .expect("Second discovery should succeed");
    let second_duration = start_time.elapsed();
    
    // Cache should be faster and return same results
    assert!(second_duration < first_duration, "Cached discovery should be faster");
    assert_eq!(peers1.len(), peers2.len(), "Cached results should match original");
}

/// Test peer information serialization and deserialization
#[tokio::test]
async fn test_peer_serialization() {
    let bootstrap_peers = vec![
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWGrfVU2HUd65qGM9jdnvYTfL5e4DjX1FgZm8R3S4X5K6Z".to_string(),
    ];
    
    let discovery = DhtPeerDiscovery::new(bootstrap_peers).await
        .expect("DHT initialization should succeed");
    
    // Create test peer info
    let peer_info = crate::proto::PeerInfo {
        node_id: "test-node-123".to_string(),
        address: "test.example.com:4330".to_string(),
        latency_ms: 42.5,
        bandwidth_mbps: 150.0,
        status: "active".to_string(),
        last_seen: Some(system_time_to_proto_timestamp(SystemTime::now())),
        connection_count: 10,
        region: "test-region".to_string(),
    };
    
    // Test serialization
    let serialized = discovery.serialize_peer_data(&peer_info)
        .expect("Serialization should succeed");
    assert!(!serialized.is_empty(), "Serialized data should not be empty");
    
    // Test deserialization
    let deserialized = discovery.deserialize_peer_record(&serialized)
        .expect("Deserialization should succeed");
    
    // Verify data integrity
    assert_eq!(deserialized.node_id, peer_info.node_id);
    assert_eq!(deserialized.address, peer_info.address);
    assert_eq!(deserialized.latency_ms, peer_info.latency_ms);
    assert_eq!(deserialized.bandwidth_mbps, peer_info.bandwidth_mbps);
    assert_eq!(deserialized.status, peer_info.status);
    assert_eq!(deserialized.connection_count, peer_info.connection_count);
    assert_eq!(deserialized.region, peer_info.region);
}

/// Test path builder initialization with DHT integration
#[tokio::test]
async fn test_path_builder_initialization() {
    let bootstrap_peers = vec![
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWGrfVU2HUd65qGM9jdnvYTfL5e4DjX1FgZm8R3S4X5K6Z".to_string(),
    ];
    
    let result = PathBuilder::new(bootstrap_peers).await;
    assert!(result.is_ok(), "PathBuilder initialization should succeed");
    
    let mut path_builder = result.unwrap();
    
    // Test starting DHT services
    let start_result = path_builder.start().await;
    assert!(start_result.is_ok(), "PathBuilder start should succeed");
}

/// Test path building with real peer discovery
#[tokio::test]
async fn test_path_building_with_dht() {
    let bootstrap_peers = vec![
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWGrfVU2HUd65qGM9jdnvYTfL5e4DjX1FgZm8R3S4X5K6Z".to_string(),
    ];
    
    let mut path_builder = PathBuilder::new(bootstrap_peers).await
        .expect("PathBuilder initialization should succeed");
    
    // Start DHT services
    path_builder.start().await
        .expect("PathBuilder start should succeed");
    
    // Create path request
    let request = PathRequest {
        target: "target-node-123".to_string(),
        hops: 3,
    };
    
    // Build path with timeout to prevent hanging
    let result = timeout(Duration::from_secs(10), path_builder.build_path(request)).await;
    assert!(result.is_ok(), "Path building should complete within timeout");
    
    let path_response = result.unwrap().expect("Path building should succeed");
    assert!(!path_response.path.is_empty(), "Path should not be empty");
    assert!(path_response.estimated_latency_ms > 0.0, "Path should have estimated latency");
    assert!(path_response.estimated_bandwidth_mbps > 0.0, "Path should have estimated bandwidth");
    assert!(path_response.reliability_score > 0.0, "Path should have reliability score");
}

/// Test error handling for DHT communication failures
#[tokio::test]
async fn test_dht_error_handling() {
    // Test with unreachable bootstrap peers
    let unreachable_peers = vec![
        "/ip4/192.0.2.1/tcp/1234/p2p/12D3KooWInvalidPeerID".to_string(),
    ];
    
    // DHT initialization might succeed even with unreachable peers
    // but discovery operations should handle failures gracefully
    if let Ok(mut discovery) = DhtPeerDiscovery::new(unreachable_peers).await {
        // Test that discovery operations handle failures gracefully
        let result = timeout(
            Duration::from_secs(5), 
            discovery.discover_peers(DiscoveryCriteria::All)
        ).await;
        
        // Should either succeed with fallback peers or fail gracefully
        match result {
            Ok(Ok(peers)) => {
                // Fallback mechanism worked
                assert!(!peers.is_empty(), "Fallback peers should be available");
            }
            Ok(Err(e)) => {
                // Graceful error handling
                println!("Expected error: {:?}", e);
            }
            Err(_) => {
                // Timeout occurred - acceptable for unreachable peers
            }
        }
    }
}

/// Test peer discovery with mixed criteria
#[tokio::test]
async fn test_mixed_discovery_criteria() {
    let bootstrap_peers = vec![
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWGrfVU2HUd65qGM9jdnvYTfL5e4DjX1FgZm8R3S4X5K6Z".to_string(),
    ];
    
    let mut discovery = DhtPeerDiscovery::new(bootstrap_peers).await
        .expect("DHT initialization should succeed");
    
    // Test all discovery criteria types
    let criteria_tests = vec![
        DiscoveryCriteria::ByRegion("eu-central".to_string()),
        DiscoveryCriteria::ByCapability("relay".to_string()),
        DiscoveryCriteria::ByLatency(200.0),
        DiscoveryCriteria::Random(3),
        DiscoveryCriteria::All,
    ];
    
    for criteria in criteria_tests {
        let result = discovery.discover_peers(criteria.clone()).await;
        assert!(result.is_ok(), "Discovery should succeed for criteria: {:?}", criteria);
        
        let peers = result.unwrap();
        assert!(!peers.is_empty(), "Should discover some peers for criteria: {:?}", criteria);
        
        // Verify peer information completeness
        for peer in &peers {
            assert!(!peer.node_id.is_empty(), "Peer should have valid node ID");
            assert!(!peer.address.is_empty(), "Peer should have valid address");
            assert!(peer.latency_ms >= 0.0, "Peer latency should be non-negative");
            assert!(peer.bandwidth_mbps >= 0.0, "Peer bandwidth should be non-negative");
            assert!(!peer.status.is_empty(), "Peer should have status");
            assert!(!peer.region.is_empty(), "Peer should have region");
        }
    }
}

/// Test concurrent peer discovery operations
#[tokio::test]
async fn test_concurrent_peer_discovery() {
    let bootstrap_peers = vec![
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWGrfVU2HUd65qGM9jdnvYTfL5e4DjX1FgZm8R3S4X5K6Z".to_string(),
    ];
    
    let discovery = Arc::new(Mutex::new(
        DhtPeerDiscovery::new(bootstrap_peers).await
            .expect("DHT initialization should succeed")
    ));
    
    // Launch multiple concurrent discovery operations
    let mut handles = Vec::new();
    for i in 0..5 {
        let discovery_clone = Arc::clone(&discovery);
        let handle = tokio::spawn(async move {
            let mut discovery = discovery_clone.lock().await;
            discovery.discover_peers(DiscoveryCriteria::Random(i + 1)).await
        });
        handles.push(handle);
    }
    
    // Wait for all operations to complete
    let mut successful_discoveries = 0;
    for handle in handles {
        if let Ok(Ok(_)) = handle.await {
            successful_discoveries += 1;
        }
    }
    
    assert!(successful_discoveries > 0, "At least some concurrent discoveries should succeed");
}
