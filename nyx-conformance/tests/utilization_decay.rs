use nyx_mix::adaptive::UtilizationEstimator;
use std::thread::sleep;
use std::time::Duration;

#[test]
fn utilization_decay_over_time() {
    let mut est = UtilizationEstimator::new(2); // 2s window
    for _ in 0..100 { est.record(1200); }
    let high = est.throughput_bps();
    assert!(high > 50000.0);
    sleep(Duration::from_secs(3));
    let low = est.throughput_bps();
    assert!(low < 1000.0);
} 