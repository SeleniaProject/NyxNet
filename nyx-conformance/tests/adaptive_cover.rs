use nyx_mix::adaptive::{AdaptiveCoverGenerator, UtilizationEstimator};
use std::time::Duration;

#[test]
fn utilization_estimator_basic_throughput() {
    let mut est = UtilizationEstimator::new(1); // 1-second window
    est.record(1200); // one typical packet
    let thr = est.throughput_bps();
    // Basic sanity: throughput must be positive and roughly packet size per second.
    assert!(thr > 0.0);
    assert!(thr <= 1200.0);
}

#[test]
fn adaptive_cover_increases_lambda_on_low_util() {
    let mut gen = AdaptiveCoverGenerator::new(5.0, 0.5); // base λ 5 events/sec
    // Capture initial delay distribution sample.
    let d_initial = gen.next_delay();
    // Simulate zero real traffic for a while – utilization remains low.
    for _ in 0..20 {
        let _ = gen.next_delay();
    }
    let d_low_util = gen.next_delay();
    // With low utilization, λ should remain at least base, delay similar or shorter than initial.
    assert!(d_low_util <= d_initial);
}

#[test]
fn adaptive_cover_decreases_delay_after_high_util() {
    let mut gen = AdaptiveCoverGenerator::new(10.0, 0.4);
    // First sample delay with no utilization.
    let d_before = gen.next_delay();
    // Record high real utilisation: 200 packets (approx) in short span.
    for _ in 0..200 {
        gen.record_real_bytes(1200);
    }
    // Next delay should become significantly shorter (higher λ) than initial.
    let d_after = gen.next_delay();
    assert!(d_after < d_before);
} 