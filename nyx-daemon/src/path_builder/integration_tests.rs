//! Additional integration tests for DHT path builder implementation

#[cfg(test)]
mod integration_tests {
    use super::super::*;
    use tokio::time::{timeout, Duration};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Test concurrent path building requests
    #[tokio::test]
    async fn test_concurrent_path_building() {
        let bootstrap_peers = vec![
            "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWConcurrency1".to_string(),
        ];
        
        let config = PathBuilderConfig::default();
        let mut path_builder = PathBuilder::new(bootstrap_peers, config).await.unwrap();
        path_builder.start().await.unwrap();
        let path_builder = Arc::new(path_builder);
        
        let success_count = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        
        // Launch 10 concurrent path building requests
        for i in 0..10 {
            let pb = Arc::clone(&path_builder);
            let sc = Arc::clone(&success_count);
            
            let handle = tokio::spawn(async move {
                let request = PathRequest {
                    target: format!("target-{}", i),
                    hops: 3,
                };
                
                match timeout(Duration::from_secs(15), pb.build_path(&request)).await {
                    Ok(Ok(_response)) => {
                        sc.fetch_add(1, Ordering::SeqCst);
                    }
                    Ok(Err(e)) => {
                        eprintln!("Path building error: {}", e);
                    }
                    Err(_) => {
                        eprintln!("Path building timeout");
                    }
                }
            });
            
            handles.push(handle);
        }
        
        // Wait for all requests to complete
        for handle in handles {
            handle.await.unwrap();
        }
        
        let successes = success_count.load(Ordering::SeqCst);
        assert!(successes > 0, "At least some concurrent requests should succeed");
        println!("Concurrent test: {}/10 requests succeeded", successes);
    }
    
    /// Test DHT operations under network stress
    #[tokio::test]
    async fn test_dht_stress_conditions() {
        let bootstrap_peers = vec![
            "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWStress1".to_string(),
            "/ip4/127.0.0.1/tcp/4002/p2p/12D3KooWStress2".to_string(),
        ];
        
        let discovery = DhtPeerDiscovery::new(bootstrap_peers).await.unwrap();
        
        // Simulate rapid successive discovery requests
        for i in 0..20 {
            let criteria = match i % 4 {
                0 => DiscoveryCriteria::All,
                1 => DiscoveryCriteria::ByLatency(100.0),
                2 => DiscoveryCriteria::ByBandwidth(10.0),
                _ => DiscoveryCriteria::Random(5),
            };
            
            let result = timeout(Duration::from_secs(5), discovery.discover_peers(criteria)).await;
            match result {
                Ok(Ok(peers)) => {
                    assert!(peers.len() <= 50, "Should respect peer limit under stress");
                }
                Ok(Err(e)) => {
                    // Acceptable under stress conditions
                    println!("Discovery error under stress (expected): {}", e);
                }
                Err(_) => {
                    // Timeout acceptable under stress
                    println!("Discovery timeout under stress (expected)");
                }
            }
        }
    }
    
    /// Test peer cache eviction and memory management
    #[tokio::test] 
    async fn test_peer_cache_memory_management() {
        let bootstrap_peers = vec![
            "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWMemory1".to_string(),
        ];
        
        let discovery = DhtPeerDiscovery::new(bootstrap_peers).await.unwrap();
        
        // Create many synthetic peers to test cache eviction
        let synthetic_peers = (0..1500).map(|i| {
            crate::proto::PeerInfo {
                node_id: format!("synthetic_peer_{}", i),
                address: format!("/ip4/127.0.0.{}/tcp/4001", i % 255 + 1),
                latency_ms: 50.0 + (i as f64 % 100.0),
                bandwidth_mbps: 100.0,
                status: "connected".to_string(),
                last_seen: Some(crate::proto::Timestamp {
                    seconds: chrono::Utc::now().timestamp(),
                    nanos: 0,
                }),
                connection_count: i % 10,
                region: format!("region_{}", i % 5),
            }
        }).collect::<Vec<_>>();
        
        // Update cache with synthetic peers (should trigger LRU eviction)
        let update_result = discovery.update_peer_cache(&synthetic_peers).await;
        assert!(update_result.is_ok(), "Should handle large peer updates");
        
        // Verify cache size is bounded
        let cache = discovery.peer_cache.lock().unwrap();
        assert!(cache.len() <= 1000, "Cache should be bounded by capacity");
        println!("Cache size after bulk update: {}", cache.len());
    }
    
    /// Test persistent storage with corrupted data recovery
    #[tokio::test]
    async fn test_persistent_storage_corruption_recovery() {
        use std::env;
        
        let temp_dir = env::temp_dir().join("nyx-corruption-test");
        let store_path = temp_dir.join("corrupted_peers.json");
        let store = PersistentPeerStore::new(store_path.clone());
        
        // Create corrupted JSON file
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();
        tokio::fs::write(&store_path, "{ invalid json content }").await.unwrap();
        
        // Should handle corruption gracefully
        let load_result = store.load_peers().await;
        assert!(load_result.is_err(), "Should detect corruption");
        
        // Should recover by clearing and starting fresh
        let clear_result = store.clear().await;
        assert!(clear_result.is_ok(), "Should clear corrupted storage");
        
        let reload_result = store.load_peers().await;
        assert!(reload_result.is_ok(), "Should work after clearing corruption");
        assert!(reload_result.unwrap().is_empty(), "Should start with empty cache");
    }
    
    /// Test geographic diversity path selection
    #[tokio::test]
    async fn test_geographic_diversity_path_selection() {
        let bootstrap_peers = vec![
            "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWGeo1".to_string(),
        ];
        
        let mut config = PathBuilderConfig::default();
        config.prefer_geographic_diversity = true;
        
        let path_builder = PathBuilder::new(bootstrap_peers, config).await.unwrap();
        
        // Create peers from different regions
        let diverse_peers = vec![
            crate::proto::PeerInfo {
                node_id: "peer_us_west".to_string(),
                address: "/ip4/127.0.0.1/tcp/4001".to_string(),
                latency_ms: 50.0,
                bandwidth_mbps: 100.0,
                status: "connected".to_string(),
                last_seen: None,
                connection_count: 10,
                region: "us-west".to_string(),
            },
            crate::proto::PeerInfo {
                node_id: "peer_us_east".to_string(),
                address: "/ip4/127.0.0.1/tcp/4002".to_string(),
                latency_ms: 55.0,
                bandwidth_mbps: 90.0,
                status: "connected".to_string(),
                last_seen: None,
                connection_count: 8,
                region: "us-east".to_string(),
            },
            crate::proto::PeerInfo {
                node_id: "peer_eu_central".to_string(),
                address: "/ip4/127.0.0.1/tcp/4003".to_string(),
                latency_ms: 75.0,
                bandwidth_mbps: 85.0,
                status: "connected".to_string(),
                last_seen: None,
                connection_count: 12,
                region: "eu-central".to_string(),
            },
        ];
        
        let path = path_builder.construct_optimal_path(&diverse_peers, "target", 3).await.unwrap();
        
        // Verify geographic diversity
        let regions: std::collections::HashSet<String> = path.iter()
            .map(|peer| peer.region.clone())
            .collect();
        
        assert!(regions.len() >= 2, "Path should use peers from multiple regions");
        println!("Selected regions: {:?}", regions);
    }
    
    /// Test edge cases in path quality calculation
    #[tokio::test]
    async fn test_path_quality_edge_cases() {
        let bootstrap_peers = vec![
            "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWQuality1".to_string(),
        ];
        
        let config = PathBuilderConfig::default();
        let path_builder = PathBuilder::new(bootstrap_peers, config).await.unwrap();
        
        // Test with empty path
        let empty_path = vec![];
        let result = path_builder.calculate_path_quality(&empty_path).await;
        assert!(result.is_err(), "Should fail for empty path");
        
        // Test with single peer
        let single_peer = vec![
            crate::proto::PeerInfo {
                node_id: "single_peer".to_string(),
                address: "/ip4/127.0.0.1/tcp/4001".to_string(),
                latency_ms: 50.0,
                bandwidth_mbps: 100.0,
                status: "connected".to_string(),
                last_seen: None,
                connection_count: 5,
                region: "test-region".to_string(),
            },
        ];
        
        let quality = path_builder.calculate_path_quality(&single_peer).await.unwrap();
        assert_eq!(quality.latency_ms, 50.0);
        assert_eq!(quality.bandwidth_mbps, 100.0);
        assert_eq!(quality.geographic_diversity, 1.0); // 1 region out of 1 peer
        
        // Test with extreme values
        let extreme_peers = vec![
            crate::proto::PeerInfo {
                node_id: "slow_peer".to_string(),
                address: "/ip4/127.0.0.1/tcp/4001".to_string(),
                latency_ms: 2000.0, // Very high latency
                bandwidth_mbps: 0.1, // Very low bandwidth
                status: "connected".to_string(),
                last_seen: None,
                connection_count: 1000, // Very high connections
                region: "slow-region".to_string(),
            },
        ];
        
        let extreme_quality = path_builder.calculate_path_quality(&extreme_peers).await.unwrap();
        assert!(extreme_quality.overall_score >= 0.0 && extreme_quality.overall_score <= 1.0,
                "Quality score should be normalized even for extreme values");
    }
}
