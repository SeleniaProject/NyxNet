use nyx_mix::adaptive::UtilizationEstimator;
use std::thread::sleep;
use std::time::Duration;

#[test]
fn utilization_window_purge() {
    let mut est = UtilizationEstimator::new(1); // 1s window
    est.record(1200);
    let thr_before = est.throughput_bps();
    assert!(thr_before > 1000.0);
    // sleep 1100ms to exceed window
    sleep(Duration::from_millis(1100));
    let thr_after = est.throughput_bps();
    assert!(thr_after < 10.0);
} 