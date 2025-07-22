//! PathBuilder dynamic hop count conformance test
#![allow(clippy::unwrap_used)]

use nyx_mix::{larmix::{Prober, LARMixPlanner}};
use rand::{thread_rng, Rng};
use nyx_core::NodeId;

#[test]
fn adaptive_hop_count_within_expected_range() {
    let mut rng = thread_rng();
    let mut prober = Prober::new();
    // create 50 synthetic nodes with random RTT 5–200 ms and bandwidth 10–500 Mbps
    for i in 0u8..50 {
        let id: NodeId = [i; 32];
        let rtt = rng.gen_range(5.0..200.0);
        let bw = rng.gen_range(10.0..500.0);
        prober.record_rtt(id, rtt);
        prober.record_throughput(id, bw);
    }
    let planner = LARMixPlanner::new(&prober, 0.7);
    let mut total_hops = 0;
    let samples = 200;
    for _ in 0..samples {
        let path = planner.build_path_dynamic();
        total_hops += path.len();
    }
    let avg = total_hops as f64 / samples as f64;
    assert!(avg >= 4.0 && avg <= 6.0, "average hop count {avg}");
} 