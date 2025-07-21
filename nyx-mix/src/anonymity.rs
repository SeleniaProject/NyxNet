//! Statistical anonymity evaluation utilities.
//! Computes a simple entropy-based anonymity score over observed inter-packet delays.
#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::time::Duration;

/// Default sliding-window size in seconds used for anonymity evaluation.
pub const DEFAULT_WINDOW_SEC: u64 = 5;

/// Evaluates entropy of delay distribution to approximate anonymity (0–1).
/// This is a coarse metric: bucketises delays into 10 bins and computes
/// Shannon entropy H / H_max.
pub struct AnonymityEvaluator {
    window: VecDeque<Duration>,
    window_len: Duration,
}

impl AnonymityEvaluator {
    pub fn new(window_sec: u64) -> Self {
        Self { window: VecDeque::new(), window_len: Duration::from_secs(window_sec.max(1)) }
    }

    /// Record a new observed delay.
    pub fn record_delay(&mut self, d: Duration) {
        let now = std::time::Instant::now();
        // store duration with timestamp via tuple
        self.window.push_back(d);
        // keep size under ~200 samples to bound memory
        if self.window.len() > 256 { self.window.pop_front(); }
    }

    /// Compute anonymity score (0..1). Higher score = higher entropy.
    pub fn score(&self) -> f64 {
        if self.window.is_empty() { return 0.0; }
        // Bucketise delays (0-500ms) into 10 bins; delays >500ms fall into last bin
        let mut buckets = [0usize; 10];
        for d in &self.window {
            let ms = d.as_secs_f64() * 1000.0;
            let idx = (ms / 50.0).floor() as usize; // 0-9
            let idx = idx.min(9);
            buckets[idx] += 1;
        }
        let n = self.window.len() as f64;
        let mut entropy = 0.0;
        for &count in &buckets {
            if count == 0 { continue; }
            let p = count as f64 / n;
            entropy -= p * p.log2();
        }
        let h_max = (10u32).ilog2() as f64; // log2(10) ≈ 3.32
        (entropy / h_max).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entropy_bounds() {
        let mut ev = AnonymityEvaluator::new(5);
        for _ in 0..100 { ev.record_delay(Duration::from_millis(10)); }
        assert!(ev.score() <= 0.5);
        // Mixed delays for higher entropy
        let mut ev2 = AnonymityEvaluator::new(5);
        for i in 0..100 { ev2.record_delay(Duration::from_millis((i % 500) as u64)); }
        assert!(ev2.score() > ev.score());
    }
} 