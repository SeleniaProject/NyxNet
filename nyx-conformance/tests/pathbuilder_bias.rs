use nyx_mix::{Candidate, WeightedPathBuilder};

#[test]
fn weighted_pathbuilder_latency_bias() {
    let low_latency = Candidate { id: [1u8;32], latency_ms: 10.0, bandwidth_mbps: 50.0 };
    let high_latency = Candidate { id: [2u8;32], latency_ms: 200.0, bandwidth_mbps: 50.0 };
    let candidates = [low_latency, high_latency];

    let builder_latency = WeightedPathBuilder::new(&candidates, 0.7); // latency-biased
    let mut low_count = 0;
    for _ in 0..500 {
        let path = builder_latency.build_path(1);
        if path[0] == low_latency.id { low_count += 1; }
    }
    assert!(low_count > 350, "Latency-biased builder should prefer low latency node");
} 