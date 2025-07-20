use nyx_mix::{Candidate, WeightedPathBuilder};
use nyx_core::NodeId;

#[test]
fn weighted_builder_bandwidth_bias() {
    let low_bw = Candidate { id: [1u8;32], latency_ms: 50.0, bandwidth_mbps: 10.0 };
    let high_bw = Candidate { id: [2u8;32], latency_ms: 50.0, bandwidth_mbps: 500.0 };
    let cands = [low_bw, high_bw];
    let builder = WeightedPathBuilder::new(&cands, 0.0); // alpha 0 = bandwidth only
    let mut high_count = 0;
    for _ in 0..500 {
        let p = builder.build_path(1);
        if p[0] == high_bw.id { high_count += 1; }
    }
    assert!(high_count > 350, "High bandwidth node should dominate selection");
} 