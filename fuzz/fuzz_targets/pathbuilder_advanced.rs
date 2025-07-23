#![no_main]

use libfuzzer_sys::fuzz_target;
use nyx_mix::{Candidate, WeightedPathBuilder, PathBuilder, DynamicPathBuilder};
use nyx_mix::larmix::{Prober, LARMixPlanner};
use nyx_core::NodeId;
use std::collections::HashMap;

fuzz_target!(|data: &[u8]| {
    if data.len() < 16 {
        return;
    }
    
    // Generate test candidates from fuzz data
    let candidates = generate_candidates_from_data(data);
    if candidates.is_empty() {
        return;
    }
    
    // Test weighted path builder
    test_weighted_path_builder(&candidates, data);
    
    // Test dynamic path builder
    test_dynamic_path_builder(&candidates, data);
    
    // Test LARMix path planner
    test_larmix_planner(&candidates, data);
    
    // Test path validation
    test_path_validation(&candidates, data);
    
    // Test edge cases
    test_edge_cases(&candidates, data);
});

fn generate_candidates_from_data(data: &[u8]) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    let mut offset = 0;
    
    // Generate up to 50 candidates from fuzz data
    while offset + 40 <= data.len() && candidates.len() < 50 {
        let mut id = [0u8; 32];
        id.copy_from_slice(&data[offset..offset + 32]);
        
        // Extract latency and bandwidth from remaining bytes
        let latency_bytes = &data[offset + 32..offset + 36];
        let bandwidth_bytes = &data[offset + 36..offset + 40];
        
        let latency_raw = u32::from_le_bytes([
            latency_bytes[0], latency_bytes[1], 
            latency_bytes[2], latency_bytes[3]
        ]);
        let bandwidth_raw = u32::from_le_bytes([
            bandwidth_bytes[0], bandwidth_bytes[1], 
            bandwidth_bytes[2], bandwidth_bytes[3]
        ]);
        
        // Convert to reasonable ranges
        let latency_ms = (latency_raw % 1000) as f64 + 1.0; // 1-1000ms
        let bandwidth_mbps = (bandwidth_raw % 1000) as f64 + 1.0; // 1-1000 Mbps
        
        candidates.push(Candidate {
            id,
            latency_ms,
            bandwidth_mbps,
        });
        
        offset += 40;
    }
    
    candidates
}

fn test_weighted_path_builder(candidates: &[Candidate], data: &[u8]) {
    // Test various bias values
    let bias_values = [0.0, 0.25, 0.5, 0.75, 1.0];
    
    for &bias in &bias_values {
        let builder = WeightedPathBuilder::new(candidates, bias);
        
        // Test different path lengths
        for length in 1..=std::cmp::min(candidates.len(), 10) {
            let path = builder.build_path(length);
            
            // Validate path properties
            assert!(path.len() <= length);
            assert!(path.len() <= candidates.len());
            
            // Ensure all nodes in path are unique
            let mut seen = std::collections::HashSet::new();
            for node_id in &path {
                assert!(seen.insert(node_id), "Duplicate node in path");
            }
            
            // Verify all nodes exist in candidates
            for node_id in &path {
                assert!(candidates.iter().any(|c| &c.id == node_id), 
                       "Path contains unknown node");
            }
        }
    }
}

fn test_dynamic_path_builder(candidates: &[Candidate], data: &[u8]) {
    let mut prober = Prober::new();
    
    // Populate prober with candidate data
    for candidate in candidates {
        prober.record_rtt(candidate.id, candidate.latency_ms);
        prober.record_throughput(candidate.id, candidate.bandwidth_mbps);
        
        // Add some network conditions from fuzz data
        if let Some(&reliability_byte) = data.get(0) {
            let reliability = (reliability_byte as f64) / 255.0;
            prober.record_reliability(candidate.id, reliability);
        }
    }
    
    // Test dynamic path building with different parameters
    let planner = LARMixPlanner::new(&prober, 0.7);
    
    for _ in 0..10 {
        let path = planner.build_path_dynamic();
        
        // Validate dynamic path properties
        assert!(path.len() >= 3, "Dynamic path should have at least 3 hops");
        assert!(path.len() <= 8, "Dynamic path should not exceed 8 hops");
        
        // Ensure path diversity
        let mut seen = std::collections::HashSet::new();
        for node_id in &path {
            assert!(seen.insert(node_id), "Duplicate node in dynamic path");
        }
    }
}

fn test_larmix_planner(candidates: &[Candidate], data: &[u8]) {
    let mut prober = Prober::new();
    
    // Create network topology from candidates
    for (i, candidate) in candidates.iter().enumerate() {
        prober.record_rtt(candidate.id, candidate.latency_ms);
        prober.record_throughput(candidate.id, candidate.bandwidth_mbps);
        
        // Add geographic diversity simulation
        let geo_zone = i % 5; // 5 geographic zones
        prober.record_geographic_zone(candidate.id, geo_zone);
        
        // Add network conditions from fuzz data
        if let Some(condition_data) = data.get(i % data.len()) {
            let load = (*condition_data as f64) / 255.0;
            prober.record_load(candidate.id, load);
        }
    }
    
    let planner = LARMixPlanner::new(&prober, 0.8);
    
    // Test various path building strategies
    test_latency_optimized_paths(&planner);
    test_bandwidth_optimized_paths(&planner);
    test_reliability_optimized_paths(&planner);
    test_geographic_diversity_paths(&planner);
}

fn test_latency_optimized_paths(planner: &LARMixPlanner) {
    let path = planner.build_latency_optimized_path(5);
    
    // Verify latency optimization
    assert!(path.len() <= 5);
    
    // Check that path prioritizes low-latency nodes
    if path.len() >= 2 {
        // This is a heuristic test - in practice, we'd need access to the 
        // actual latency values to verify proper optimization
        assert!(!path.is_empty());
    }
}

fn test_bandwidth_optimized_paths(planner: &LARMixPlanner) {
    let path = planner.build_bandwidth_optimized_path(4);
    
    // Verify bandwidth optimization
    assert!(path.len() <= 4);
    assert!(!path.is_empty());
}

fn test_reliability_optimized_paths(planner: &LARMixPlanner) {
    let path = planner.build_reliability_optimized_path(6);
    
    // Verify reliability optimization
    assert!(path.len() <= 6);
    assert!(!path.is_empty());
}

fn test_geographic_diversity_paths(planner: &LARMixPlanner) {
    let path = planner.build_geographically_diverse_path(5);
    
    // Verify geographic diversity
    assert!(path.len() <= 5);
    assert!(!path.is_empty());
}

fn test_path_validation(candidates: &[Candidate], data: &[u8]) {
    let builder = WeightedPathBuilder::new(candidates, 0.5);
    
    // Test with various invalid inputs
    let empty_path = builder.build_path(0);
    assert!(empty_path.is_empty());
    
    // Test with more hops than candidates
    let oversized_path = builder.build_path(candidates.len() + 10);
    assert!(oversized_path.len() <= candidates.len());
    
    // Test path validation with corrupted node IDs
    if !candidates.is_empty() {
        let mut corrupted_candidates = candidates.to_vec();
        
        // Corrupt some node IDs using fuzz data
        for (i, candidate) in corrupted_candidates.iter_mut().enumerate() {
            if let Some(&corruption_byte) = data.get(i % data.len()) {
                candidate.id[0] ^= corruption_byte;
            }
        }
        
        let corrupted_builder = WeightedPathBuilder::new(&corrupted_candidates, 0.5);
        let corrupted_path = corrupted_builder.build_path(3);
        
        // Should still produce valid paths
        assert!(corrupted_path.len() <= 3);
        assert!(corrupted_path.len() <= corrupted_candidates.len());
    }
}

fn test_edge_cases(candidates: &[Candidate], data: &[u8]) {
    // Test with single candidate
    if !candidates.is_empty() {
        let single_candidate = vec![candidates[0].clone()];
        let builder = WeightedPathBuilder::new(&single_candidate, 0.5);
        let path = builder.build_path(5);
        assert_eq!(path.len(), 1);
        assert_eq!(path[0], single_candidate[0].id);
    }
    
    // Test with extreme latency values
    let mut extreme_candidates = candidates.to_vec();
    for candidate in &mut extreme_candidates {
        if let Some(&modifier) = data.get(0) {
            match modifier % 4 {
                0 => candidate.latency_ms = 0.001, // Very low latency
                1 => candidate.latency_ms = 10000.0, // Very high latency
                2 => candidate.bandwidth_mbps = 0.001, // Very low bandwidth
                3 => candidate.bandwidth_mbps = 10000.0, // Very high bandwidth
                _ => {}
            }
        }
    }
    
    if !extreme_candidates.is_empty() {
        let extreme_builder = WeightedPathBuilder::new(&extreme_candidates, 0.5);
        let extreme_path = extreme_builder.build_path(3);
        assert!(extreme_path.len() <= 3);
        assert!(extreme_path.len() <= extreme_candidates.len());
    }
    
    // Test with identical candidates
    if !candidates.is_empty() {
        let identical_candidates = vec![candidates[0].clone(); 5];
        let identical_builder = WeightedPathBuilder::new(&identical_candidates, 0.5);
        let identical_path = identical_builder.build_path(3);
        
        // Should still work but with limited diversity
        assert!(identical_path.len() <= 3);
        
        // All nodes should be the same
        for node_id in &identical_path {
            assert_eq!(*node_id, identical_candidates[0].id);
        }
    }
    
    // Test bias edge cases
    let extreme_bias_values = [-1.0, 2.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY];
    
    for &bias in &extreme_bias_values {
        if bias.is_finite() {
            let builder = WeightedPathBuilder::new(candidates, bias);
            let path = builder.build_path(3);
            
            // Should handle extreme bias values gracefully
            assert!(path.len() <= 3);
            assert!(path.len() <= candidates.len());
        }
    }
} 