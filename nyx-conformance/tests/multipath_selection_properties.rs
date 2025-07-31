use nyx_core::NodeId;
use nyx_mix::{Candidate, WeightedPathBuilder};
use proptest::prelude::*;
use std::collections::HashSet;

/// Property-based tests for multipath selection algorithms
/// These tests verify that the Rust implementation matches the TLA+ model behavior
/// defined in formal/nyx_multipath_plugin.tla

/// Generate a valid NodeId for testing
fn node_id_strategy() -> impl Strategy<Value = NodeId> {
    any::<[u8; 32]>()
}

/// Generate a candidate node with realistic metrics
fn candidate_strategy() -> impl Strategy<Value = Candidate> {
    (
        node_id_strategy(),
        1.0f64..500.0f64,  // latency_ms: 1-500ms
        1.0f64..1000.0f64, // bandwidth_mbps: 1-1000 Mbps
    ).prop_map(|(id, latency_ms, bandwidth_mbps)| Candidate {
        id,
        latency_ms,
        bandwidth_mbps,
    })
}

/// Generate a collection of unique candidate nodes
fn candidates_strategy(min_count: usize, max_count: usize) -> impl Strategy<Value = Vec<Candidate>> {
    prop::collection::vec(candidate_strategy(), min_count..=max_count)
        .prop_map(|mut candidates| {
            // Ensure all NodeIds are unique to match TLA+ model constraints
            let mut seen = HashSet::new();
            candidates.retain(|c| seen.insert(c.id));
            candidates
        })
        .prop_filter("Need at least minimum candidates", move |candidates| {
            candidates.len() >= min_count
        })
}

proptest! {
    /// Test that path length constraints match TLA+ model: Inv_PathLen
    /// TLA+: Len(path) \in 3..7
    #[test]
    fn path_length_constraints_match_tla_model(
        candidates in candidates_strategy(8, 20),
        path_length in 3usize..=7usize,
        alpha in 0.0f64..=1.0f64
    ) {
        prop_assume!(candidates.len() >= path_length);
        
        let builder = WeightedPathBuilder::new(&candidates, alpha);
        let path = builder.build_path(path_length);
        
        // Verify path length matches TLA+ constraint: Len(path) \in 3..7
        prop_assert!(path.len() == path_length);
        prop_assert!(path.len() >= 3);
        prop_assert!(path.len() <= 7);
    }

    /// Test that paths have no duplicate nodes: Inv_NoDup
    /// TLA+: \A i, j \in 1..Len(path): i # j => path[i] # path[j]
    #[test]
    fn path_uniqueness_matches_tla_model(
        candidates in candidates_strategy(8, 20),
        path_length in 3usize..=7usize,
        alpha in 0.0f64..=1.0f64
    ) {
        prop_assume!(candidates.len() >= path_length);
        
        let builder = WeightedPathBuilder::new(&candidates, alpha);
        let path = builder.build_path(path_length);
        
        // Verify no duplicates match TLA+ constraint: Inv_NoDup
        let unique_nodes: HashSet<NodeId> = path.iter().cloned().collect();
        prop_assert_eq!(unique_nodes.len(), path.len());
        
        // Explicit check matching TLA+ formulation
        for i in 0..path.len() {
            for j in 0..path.len() {
                if i != j {
                    prop_assert_ne!(path[i], path[j]);
                }
            }
        }
    }

    /// Test that selected nodes are from the candidate set
    /// This validates node selection against the available node universe
    #[test]
    fn node_selection_validation_matches_tla_model(
        candidates in candidates_strategy(8, 20),
        path_length in 3usize..=7usize,
        alpha in 0.0f64..=1.0f64
    ) {
        prop_assume!(candidates.len() >= path_length);
        
        let builder = WeightedPathBuilder::new(&candidates, alpha);
        let path = builder.build_path(path_length);
        
        let candidate_ids: HashSet<NodeId> = candidates.iter().map(|c| c.id).collect();
        
        // Verify all path nodes are from candidate set (matches TLA+ NodeCount constraint)
        for node_id in &path {
            prop_assert!(candidate_ids.contains(node_id));
        }
    }

    /// Test weighted selection bias properties
    /// Verifies that the weighted selection algorithm behaves predictably
    #[test]
    fn weighted_selection_bias_properties(
        alpha in 0.8f64..1.0f64  // Very strongly latency-biased
    ) {
        // Create two candidates with extreme latency difference but same bandwidth
        let low_latency = Candidate {
            id: [1u8; 32],
            latency_ms: 10.0,  // Very low latency
            bandwidth_mbps: 100.0,
        };
        let high_latency = Candidate {
            id: [2u8; 32],
            latency_ms: 500.0,  // Very high latency
            bandwidth_mbps: 100.0,  // Same bandwidth
        };
        
        let candidates = vec![low_latency, high_latency];
        let builder = WeightedPathBuilder::new(&candidates, alpha);
        
        // Calculate expected weights to understand the bias
        const MAX_BW: f64 = 1000.0;
        let low_lat_score = (1.0 / low_latency.latency_ms.max(1.0)) * alpha;
        let low_bw_score = (low_latency.bandwidth_mbps / MAX_BW).min(1.0) * (1.0 - alpha);
        let low_weight = low_lat_score + low_bw_score;
        
        let high_lat_score = (1.0 / high_latency.latency_ms.max(1.0)) * alpha;
        let high_bw_score = (high_latency.bandwidth_mbps / MAX_BW).min(1.0) * (1.0 - alpha);
        let high_weight = high_lat_score + high_bw_score;
        
        // Only test if there's a significant weight difference
        prop_assume!(low_weight > high_weight * 1.5);
        
        let mut low_latency_selections = 0;
        let trials = 500;  // More trials for statistical significance
        
        for _ in 0..trials {
            let path = builder.build_path(1);
            if path[0] == low_latency.id {
                low_latency_selections += 1;
            }
        }
        
        let selection_ratio = low_latency_selections as f64 / trials as f64;
        let expected_ratio = low_weight / (low_weight + high_weight);
        
        // Allow for statistical variance - should be within reasonable bounds of expected ratio
        prop_assert!(selection_ratio > expected_ratio * 0.7, 
            "Selection ratio {} should be closer to expected {} with alpha = {}", 
            selection_ratio, expected_ratio, alpha);
    }

    /// Test path generation with minimal candidate set
    /// Edge case testing with exactly the minimum required candidates
    #[test]
    fn minimal_candidate_set_path_generation(
        path_length in 3usize..=7usize,
        alpha in 0.0f64..=1.0f64
    ) {
        // Create exactly path_length candidates
        let candidates: Vec<Candidate> = (0..path_length)
            .map(|i| Candidate {
                id: {
                    let mut id = [0u8; 32];
                    id[0] = i as u8;
                    id
                },
                latency_ms: 50.0 + i as f64 * 10.0,
                bandwidth_mbps: 100.0,
            })
            .collect();
        
        let builder = WeightedPathBuilder::new(&candidates, alpha);
        let path = builder.build_path(path_length);
        
        // Should use all available candidates
        prop_assert_eq!(path.len(), path_length);
        
        // All nodes should be unique
        let unique_nodes: HashSet<NodeId> = path.iter().cloned().collect();
        prop_assert_eq!(unique_nodes.len(), path_length);
    }

    /// Test path generation with excessive candidate set
    /// Verifies behavior when many more candidates are available than needed
    #[test]
    fn excessive_candidate_set_path_generation(
        path_length in 3usize..=7usize,
        extra_candidates in 10usize..50usize,
        alpha in 0.0f64..=1.0f64
    ) {
        let total_candidates = path_length + extra_candidates;
        
        let candidates: Vec<Candidate> = (0..total_candidates)
            .map(|i| Candidate {
                id: {
                    let mut id = [0u8; 32];
                    // Use multiple bytes to ensure uniqueness
                    id[0] = (i / 256) as u8;
                    id[1] = (i % 256) as u8;
                    id
                },
                latency_ms: 20.0 + (i as f64 * 5.0) % 200.0,
                bandwidth_mbps: 50.0 + (i as f64 * 10.0) % 500.0,
            })
            .collect();
        
        let builder = WeightedPathBuilder::new(&candidates, alpha);
        let path = builder.build_path(path_length);
        
        // Should generate exactly requested path length
        prop_assert_eq!(path.len(), path_length);
        
        // All nodes should be unique
        let unique_nodes: HashSet<NodeId> = path.iter().cloned().collect();
        prop_assert_eq!(unique_nodes.len(), path_length);
        
        // All selected nodes should be from candidate set
        let candidate_ids: HashSet<NodeId> = candidates.iter().map(|c| c.id).collect();
        for node_id in &path {
            prop_assert!(candidate_ids.contains(node_id));
        }
    }
}

/// Additional deterministic tests for specific edge cases
#[cfg(test)]
mod deterministic_tests {
    use super::*;

    #[test]
    fn empty_candidate_set_returns_empty_path() {
        let candidates: Vec<Candidate> = vec![];
        let builder = WeightedPathBuilder::new(&candidates, 0.5);
        let path = builder.build_path(3);
        assert!(path.is_empty());
    }

    #[test]
    fn insufficient_candidates_returns_partial_path() {
        let candidates = vec![
            Candidate {
                id: [1u8; 32],
                latency_ms: 50.0,
                bandwidth_mbps: 100.0,
            },
            Candidate {
                id: [2u8; 32],
                latency_ms: 60.0,
                bandwidth_mbps: 120.0,
            },
        ];
        
        let builder = WeightedPathBuilder::new(&candidates, 0.5);
        let path = builder.build_path(5); // Request more than available
        
        // Should return all available candidates
        assert_eq!(path.len(), 2);
        
        // Should be unique
        let unique_nodes: HashSet<NodeId> = path.iter().cloned().collect();
        assert_eq!(unique_nodes.len(), 2);
    }

    #[test]
    fn single_candidate_single_hop_path() {
        let candidates = vec![
            Candidate {
                id: [42u8; 32],
                latency_ms: 25.0,
                bandwidth_mbps: 200.0,
            },
        ];
        
        let builder = WeightedPathBuilder::new(&candidates, 0.7);
        let path = builder.build_path(1);
        
        assert_eq!(path.len(), 1);
        assert_eq!(path[0], [42u8; 32]);
    }

    #[test]
    fn alpha_boundary_values() {
        let candidates = vec![
            Candidate { id: [1u8; 32], latency_ms: 10.0, bandwidth_mbps: 50.0 },
            Candidate { id: [2u8; 32], latency_ms: 100.0, bandwidth_mbps: 500.0 },
        ];
        
        // Test alpha = 0.0 (bandwidth-only)
        let builder_bw = WeightedPathBuilder::new(&candidates, 0.0);
        let path_bw = builder_bw.build_path(1);
        assert!(path_bw.len() == 1);
        
        // Test alpha = 1.0 (latency-only)
        let builder_lat = WeightedPathBuilder::new(&candidates, 1.0);
        let path_lat = builder_lat.build_path(1);
        assert!(path_lat.len() == 1);
    }

    #[test]
    fn path_length_boundary_conditions() {
        let candidates: Vec<Candidate> = (0..10)
            .map(|i| Candidate {
                id: {
                    let mut id = [0u8; 32];
                    id[0] = i as u8;
                    id
                },
                latency_ms: 50.0,
                bandwidth_mbps: 100.0,
            })
            .collect();
        
        let builder = WeightedPathBuilder::new(&candidates, 0.5);
        
        // Test minimum path length (3) - matches TLA+ constraint
        let path_min = builder.build_path(3);
        assert_eq!(path_min.len(), 3);
        let unique_min: HashSet<NodeId> = path_min.iter().cloned().collect();
        assert_eq!(unique_min.len(), 3);
        
        // Test maximum path length (7) - matches TLA+ constraint
        let path_max = builder.build_path(7);
        assert_eq!(path_max.len(), 7);
        let unique_max: HashSet<NodeId> = path_max.iter().cloned().collect();
        assert_eq!(unique_max.len(), 7);
    }

    #[test]
    fn node_id_uniqueness_stress_test() {
        // Create many candidates with similar metrics to stress uniqueness
        let candidates: Vec<Candidate> = (0..20)
            .map(|i| Candidate {
                id: {
                    let mut id = [0u8; 32];
                    // Ensure each NodeId is unique
                    id[0] = (i / 256) as u8;
                    id[1] = (i % 256) as u8;
                    id[2] = ((i * 7) % 256) as u8; // Additional uniqueness
                    id
                },
                latency_ms: 50.0 + (i as f64 % 10.0), // Similar latencies
                bandwidth_mbps: 100.0 + (i as f64 % 20.0), // Similar bandwidths
            })
            .collect();
        
        let builder = WeightedPathBuilder::new(&candidates, 0.5);
        
        // Test various path lengths
        for path_length in 3..=7 {
            let path = builder.build_path(path_length);
            assert_eq!(path.len(), path_length);
            
            // Verify uniqueness (TLA+ Inv_NoDup)
            let unique_nodes: HashSet<NodeId> = path.iter().cloned().collect();
            assert_eq!(unique_nodes.len(), path_length, 
                "Path length {} should have {} unique nodes", path_length, path_length);
        }
    }
}