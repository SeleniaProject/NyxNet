//! Weighted Round Robin Path Scheduler (inverse RTT weighting)
//!
//! Implements Smooth Weighted Round-Robin (SWRR) as described by Google research.
//! Each active path is assigned a *weight* proportional to `1/RTT` (lower RTT → higher weight).
//! The scheduler deterministically selects paths so that, over time, the number of packets
//! dispatched to each path approximates the ratio of their weights.
//!
//! References:
//!  * A. O. Partridge, "Weighted Round-Robin Scheduling for High-Speed Networks", 1994.
//!  * Nginx Smooth Weighted RR algorithm (https://www.nginx.com/resources/wiki/extending/examples/smoothweightedroundrobin/)
//!
//! This implementation is lock-free **if** you confine a single instance to one task. Wrap in
//! `Mutex`/`RwLock` for shared mutable access across tasks.

#![forbid(unsafe_code)]

use std::collections::HashMap;

/// Internal per-path state used by the SWRR algorithm.
#[derive(Debug, Clone)]
struct PathState {
    weight: i32,
    current: i32,
}

/// Weighted Round-Robin scheduler for selecting `PathID`s.
///
/// Example:
/// ```rust
/// use nyx_stream::WeightedRrScheduler;
/// let mut sched = WeightedRrScheduler::new();
/// sched.update_path(1, 10.0); // RTT 10ms
/// sched.update_path(2, 50.0); // RTT 50ms
/// let pid = sched.next();
/// println!("selected path {pid}");
/// ```
pub struct WeightedRrScheduler {
    // path_id → state
    paths: HashMap<u8, PathState>,
    total_weight: i32,
}

impl WeightedRrScheduler {
    /// Create an empty scheduler. Add paths via [`update_path`].
    #[must_use]
    pub fn new() -> Self {
        Self { paths: HashMap::new(), total_weight: 0 }
    }

    /// Insert or update a path with the latest RTT sample (milliseconds).
    /// Weight is computed as `(SCALE / rtt_ms)` then clamped to at least 1.
    /// Lower RTT therefore yields larger weight.
    pub fn update_path(&mut self, path_id: u8, rtt_ms: f64) {
        const SCALE: f64 = 1000.0; // Chosen so that RTT 1ms → weight 1000
        let new_weight = ((SCALE / rtt_ms.max(1.0)).round() as i32).max(1);
        match self.paths.get_mut(&path_id) {
            Some(state) => {
                // Adjust total weight accounting for delta.
                self.total_weight += new_weight - state.weight;
                state.weight = new_weight;
            }
            None => {
                self.paths.insert(path_id, PathState { weight: new_weight, current: 0 });
                self.total_weight += new_weight;
            }
        }
    }

    /// Remove a path from consideration (e.g., on failure).
    pub fn remove_path(&mut self, path_id: u8) {
        if let Some(state) = self.paths.remove(&path_id) {
            self.total_weight -= state.weight;
        }
    }

    /// Smooth Weighted Round-Robin selection of next path.
    ///
    /// Returns `None` if no paths are available.
    #[must_use]
    pub fn next(&mut self) -> Option<u8> {
        if self.paths.is_empty() { return None; }
        // 1. Increase `current` by each weight.
        for st in self.paths.values_mut() {
            st.current += st.weight;
        }
        // 2. Pick path with highest `current`.
        let (best_pid, best_state) = self.paths.iter_mut().max_by_key(|(_, st)| st.current)?;
        let pid = *best_pid;
        // 3. Reduce its `current` by total weight.
        best_state.current -= self.total_weight;
        Some(pid)
    }

    /// Returns current number of active paths.
    #[must_use]
    pub fn len(&self) -> usize { self.paths.len() }
    /// Whether no paths are present.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.paths.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_respects_weights() {
        let mut sched = WeightedRrScheduler::new();
        sched.update_path(1, 10.0); // RTT 10ms → weight ~100
        sched.update_path(2, 50.0); // RTT 50ms → weight ~20

        let mut count1 = 0;
        let mut count2 = 0;
        for _ in 0..600 {
            match sched.next() {
                Some(1) => count1 += 1,
                Some(2) => count2 += 1,
                _ => unreachable!(),
            }
        }
        // Expected ratio ≈ 5:1 → count1 ~500, count2 ~100 (within margin).
        let ratio = count1 as f64 / count2.max(1) as f64;
        assert!(ratio > 4.0 && ratio < 6.0, "ratio {ratio}");
    }
} 