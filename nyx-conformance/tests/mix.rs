use nyx_mix::{Candidate, WeightedPathBuilder, CoverGenerator};
use nyx_core::NodeId;

#[test]
fn weighted_path_builder_caps_length() {
    let cands: Vec<Candidate> = (0..5)
        .map(|i| Candidate { id: [i as u8; 32], latency_ms: 50.0, bandwidth_mbps: 100.0 })
        .collect();
    let builder = WeightedPathBuilder::new(&cands, 0.5);
    // Ask for more hops than candidates.
    let path = builder.build_path(10);
    assert_eq!(path.len(), 5); // should cap at number of unique candidates
}

#[test]
fn cover_generator_mean_delay() {
    let gen = CoverGenerator::new(20.0); // λ=20 events/s, mean 0.05s
    let samples = 5000;
    let mut acc = 0.0;
    for _ in 0..samples {
        acc += gen.next_delay().as_secs_f64();
    }
    let mean = acc / samples as f64;
    assert!((mean - 0.05).abs() < 0.01);
} 