use nyx_stream::WeightedRrScheduler;
use proptest::prelude::*;

proptest! {
    #[test]
    fn swrr_distribution_proportional(rtts in proptest::collection::vec(1u32..200u32, 2..6)) {
        // Assign sequential path IDs.
        let mut sched = WeightedRrScheduler::new();
        for (i, rtt) in rtts.iter().enumerate() {
            sched.update_path(i as u8, *rtt as f64);
        }
        // Generate selections
        let iterations = 10_000;
        let mut counts = vec![0u32; rtts.len()];
        for _ in 0..iterations {
            let pid = sched.next().unwrap();
            counts[pid as usize] += 1;
        }
        // Expected weight = 1/rtt. Compare ratio between any two paths.
        for i in 0..rtts.len() {
            for j in (i+1)..rtts.len() {
                let expected_ratio = (1.0 / rtts[i] as f64) / (1.0 / rtts[j] as f64);
                let observed_ratio = counts[i] as f64 / counts[j] as f64;
                // Allow 15% tolerance.
                prop_assert!((observed_ratio / expected_ratio - 1.0).abs() < 0.15);
            }
        }
    }
} 