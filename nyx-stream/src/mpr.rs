//! Multipath Redundant (MPR) Experimental Implementation
//!
//! This module implements a simple redundant multipath strategy: every outgoing
//! packet is *duplicated* across `k` distinct paths to improve reliability and
//! combat path-specific loss. The path subset is selected via
//! [`WeightedRrScheduler`], ensuring the load is proportionally balanced by RTT.
//!
//! Usage:
//! ```rust
//! use nyx_stream::{MprDispatcher, WeightedRrScheduler};
//! let mut sched = WeightedRrScheduler::new();
//! sched.update_path(1, 10.0);
//! sched.update_path(2, 30.0);
//! let mut mpr = MprDispatcher::new(sched, 2); // duplicate to 2 paths
//! let paths = mpr.choose_paths();
//! ```
//!
//! Limitation: This is an *experimental* feature and does not yet integrate
//! congestion control feedback. Upper layers should enable it only in test
//! scenarios (`cfg(feature="mpr_experimental")`).

#![forbid(unsafe_code)]

use super::WeightedRrScheduler;

/// Dispatcher that picks `k` distinct paths for redundant transmission.
pub struct MprDispatcher {
    scheduler: WeightedRrScheduler,
    k: usize,
    max_k: usize,
    window: std::collections::VecDeque<bool>, // success history
    window_cap: usize,
}

impl MprDispatcher {
    /// Create with an existing scheduler and redundancy degree `k` (>=1).
    #[must_use]
    pub fn new(scheduler: WeightedRrScheduler, initial_k: usize) -> Self {
        Self {
            scheduler,
            k: initial_k.max(1),
            max_k: 4,
            window: std::collections::VecDeque::with_capacity(100),
            window_cap: 100,
        }
    }

    /// Update RTT for a path; simply forwarded to internal scheduler.
    pub fn update_rtt(&mut self, path_id: u8, rtt_ms: f64) {
        self.scheduler.update_path(path_id, rtt_ms);
    }

    /// Remove a failed path.
    pub fn remove_path(&mut self, path_id: u8) { self.scheduler.remove_path(path_id); }

    /// Record whether the last packet replica set was delivered successfully (at least one success).
    /// Adaptive logic: if recent loss ratio >20% increase k (up to max_k); if <5% decrease toward 1.
    pub fn record_feedback(&mut self, success: bool) {
        if self.window.len() == self.window_cap {
            self.window.pop_front();
        }
        self.window.push_back(success);
        let losses = self.window.iter().filter(|s| !**s).count();
        let ratio = losses as f64 / self.window.len() as f64;
        if ratio > 0.2 && self.k < self.max_k {
            self.k += 1;
        } else if ratio < 0.05 && self.k > 1 {
            self.k -= 1;
        }
    }

    /// Choose up to `k` distinct path IDs for the next packet.
    #[must_use]
    pub fn choose_paths(&mut self) -> Vec<u8> {
        let mut paths = Vec::with_capacity(self.k);
        for _ in 0..self.k {
            if let Some(pid) = self.scheduler.next() {
                // ensure uniqueness within this batch
                if !paths.contains(&pid) { paths.push(pid); }
            }
        }
        paths
    }

    /// Current redundancy degree.
    #[must_use]
    pub fn redundancy(&self) -> usize { self.k }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn picks_distinct_paths() {
        let mut sched = WeightedRrScheduler::new();
        sched.update_path(1, 10.0);
        sched.update_path(2, 20.0);
        let mut mpr = MprDispatcher::new(sched, 2);
        let v = mpr.choose_paths();
        assert_eq!(v.len(), 2);
        assert!(v[0] != v[1]);
    }

    #[test]
    fn adapts_redundancy_to_loss() {
        let mut sched = WeightedRrScheduler::new();
        sched.update_path(1, 10.0);
        let mut mpr = MprDispatcher::new(sched, 1);
        // simulate high loss
        for _ in 0..30 {
            mpr.record_feedback(false);
        }
        assert!(mpr.redundancy() > 1);
        // simulate successes
        for _ in 0..100 {
            mpr.record_feedback(true);
        }
        assert_eq!(mpr.redundancy(), 1);
    }
} 