use nyx_stream::WeightedRrScheduler;

/// Simulate large number of path selections and verify that
/// Smooth Weighted Round-Robin approximates the expected traffic
/// split proportional to inverse RTT weights.
#[test]
fn weighted_rr_load_balances_by_weight() {
    let mut sched = WeightedRrScheduler::new();
    // RTT (ms): 10, 50, 100 → weights ≈ 100, 20, 10
    sched.update_path(1, 10.0);
    sched.update_path(2, 50.0);
    sched.update_path(3, 100.0);

    let mut counts = [0usize; 4]; // allow indexing by path_id (1..3)
    const ITER: usize = 30_000;
    for _ in 0..ITER {
        let pid = sched.next().expect("no path");
        counts[pid as usize] += 1;
    }

    let total = counts[1] + counts[2] + counts[3];
    let ratio1 = counts[1] as f64 / total as f64;
    let ratio2 = counts[2] as f64 / total as f64;
    let ratio3 = counts[3] as f64 / total as f64;

    // Expected ratios from weights 100:20:10 ≈ 0.769, 0.154, 0.077
    assert!((ratio1 - 0.77).abs() < 0.05, "ratio1={ratio1}");
    assert!((ratio2 - 0.15).abs() < 0.04, "ratio2={ratio2}");
    assert!((ratio3 - 0.08).abs() < 0.03, "ratio3={ratio3}");
} 