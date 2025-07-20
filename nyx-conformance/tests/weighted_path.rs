use nyx_mix::{Candidate, WeightedPathBuilder};

#[test]
fn weighted_builder_unique_nodes() {
    let cands: Vec<Candidate> = (0..10)
        .map(|i| Candidate { id: [i as u8; 32], latency_ms: 50.0 + i as f64, bandwidth_mbps: 100.0 })
        .collect();
    let builder = WeightedPathBuilder::new(&cands, 0.5);
    let path = builder.build_path(5);
    // Ensure no duplicates in path
    let mut seen = std::collections::HashSet::new();
    for id in &path { assert!(seen.insert(id)); }
    assert_eq!(path.len(), 5);
}

#[test]
fn weighted_builder_bias_latency() {
    // candidate A low latency, B high latency, both bandwidth
    let a = Candidate { id: [1u8; 32], latency_ms: 10.0, bandwidth_mbps: 100.0 };
    let b = Candidate { id: [2u8; 32], latency_ms: 200.0, bandwidth_mbps: 100.0 };
    let cands = [a, b];
    let builder = WeightedPathBuilder::new(&cands, 0.8);
    let mut a_count = 0;
    for _ in 0..500 {
        let p = builder.build_path(1);
        if p[0] == a.id { a_count += 1; }
    }
    assert!(a_count > 350, "Low latency node should dominate");
} 