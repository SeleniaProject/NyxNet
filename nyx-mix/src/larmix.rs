//! LARMix++ Latency & Bandwidth Probing and Path Optimizer
//!
//! This module provides two main components:
//!
//! * [`Prober`] – continuously measures RTT (via PING/PONG frames) and throughput for each
//!   candidate node, keeping smoothed estimates.
//! * [`LARMixPlanner`] – builds an end-to-end mix path biased toward low-latency and high-bandwidth
//!   nodes using the weighted-random strategy described in the Nyx Design Document §14.
//!
//! The weight formula matches the one defined in the spec:
//!
//! ```text
//! w = (1/RTT_ms) * ALPHA  +  (bandwidth_Mbps / MAX_BW) * (1 - ALPHA)
//! ```
//!
//! where `MAX_BW` is an arbitrary reference (1 Gbps by default).
//!
//! The planner internally reuses [`WeightedPathBuilder`] from `lib.rs`, which already implements
//! weighted sampling **without replacement**.
//!
//! # Example
//! ```rust
//! use nyx_mix::{larmix::{Prober, LARMixPlanner}, Candidate};
//! use nyx_core::NodeId;
//!
//! let mut prober = Prober::new();
//! let node_a: NodeId = [1u8;32];
//! prober.record_rtt(node_a, 12.5);
//! prober.record_throughput(node_a, 10.0);
//!
//! let planner = LARMixPlanner::new(&prober, 0.7);
//! let path = planner.build_path(5);
//! println!("chosen path: {:?}", path);
//! ```

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::time::Duration;
use crate::{Candidate, WeightedPathBuilder};
use nyx_core::NodeId;

/// Exponential moving-average smoothing factor for RTT and bandwidth.
const RTT_ALPHA: f64 = 0.2; // lower → smoother
const BW_ALPHA: f64 = 0.2;

/// Maximum bandwidth reference in Mbps. Values above will be clamped.
const MAX_BW: f64 = 1000.0; // 1 Gbps

#[derive(Debug, Clone, Copy)]
struct NodeStats {
    rtt_ms: f64,        // Smoothed RTT in milliseconds.
    bw_mbps: f64,       // Smoothed upstream bandwidth in Mbps.
}

impl NodeStats {
    fn update_rtt(&mut self, sample_ms: f64) {
        if self.rtt_ms == 0.0 {
            self.rtt_ms = sample_ms;
        } else {
            self.rtt_ms = (1.0 - RTT_ALPHA) * self.rtt_ms + RTT_ALPHA * sample_ms;
        }
    }

    fn update_bw(&mut self, sample_mbps: f64) {
        if self.bw_mbps == 0.0 {
            self.bw_mbps = sample_mbps;
        } else {
            self.bw_mbps = (1.0 - BW_ALPHA) * self.bw_mbps + BW_ALPHA * sample_mbps;
        }
    }

    fn candidate(&self, id: NodeId) -> Candidate {
        Candidate { id, latency_ms: self.rtt_ms.max(1.0), bandwidth_mbps: self.bw_mbps.min(MAX_BW) }
    }
}

/// Runtime prober storing smoothed RTT & bandwidth per node.
#[derive(Default)]
pub struct Prober {
    stats: HashMap<NodeId, NodeStats>,
}

impl Prober {
    /// Create a new empty prober.
    #[must_use]
    pub fn new() -> Self { Self { stats: HashMap::new() } }

    /// Record a new RTT sample (milliseconds) for `node`.
    pub fn record_rtt(&mut self, node: NodeId, rtt_ms: f64) {
        let s = self.stats.entry(node).or_insert(NodeStats { rtt_ms: 0.0, bw_mbps: 0.0 });
        s.update_rtt(rtt_ms);
    }

    /// Record a throughput sample (megabits per second) for `node`.
    pub fn record_throughput(&mut self, node: NodeId, mbps: f64) {
        let s = self.stats.entry(node).or_insert(NodeStats { rtt_ms: 0.0, bw_mbps: 0.0 });
        s.update_bw(mbps);
    }

    /// Convenience helper: record bytes transferred over `elapsed` duration.
    pub fn record_bytes(&mut self, node: NodeId, bytes: usize, elapsed: Duration) {
        let mbps = (bytes as f64 * 8.0) / (elapsed.as_secs_f64() * 1_000_000.0);
        self.record_throughput(node, mbps);
    }

    /// Export stats as `Candidate` list for path building.
    fn candidates(&self) -> Vec<Candidate> {
        self.stats.iter().map(|(id, s)| s.candidate(*id)).collect()
    }
}

/// LARMix++ path planner utilizing [`Prober`] metrics and [`WeightedPathBuilder`].
pub struct LARMixPlanner<'a> {
    prober: &'a Prober,
    alpha: f64, // latency vs bandwidth weight (0..=1)
}

impl<'a> LARMixPlanner<'a> {
    /// `alpha` Controls bias toward latency (1.0 = latency-only, 0 = bandwidth-only).
    #[must_use]
    pub fn new(prober: &'a Prober, alpha: f64) -> Self {
        Self { prober, alpha: alpha.clamp(0.0, 1.0) }
    }

    /// Build a mix path of length `hops` using weighted sampling.
    #[must_use]
    pub fn build_path(&self, hops: usize) -> Vec<NodeId> {
        let cands = self.prober.candidates();
        if cands.is_empty() { return Vec::new(); }
        let builder = WeightedPathBuilder::new(&cands, self.alpha);
        builder.build_path(hops)
    }

    /// Build a mix path with **adaptive hop count** (3–7) based on latency statistics.
    ///
    /// The heuristic follows Nyx Design Document §4.4 “LARMix++”: lower median RTT
    /// permits longer paths without violating the overall latency budget.
    ///
    /// Mapping (median RTT → hops):
    /// * ≤20 ms ⇒ 7 hops  (very fast network)
    /// * ≤50 ms ⇒ 6 hops
    /// * ≤100 ms ⇒ 5 hops  (default)
    /// * ≤150 ms ⇒ 4 hops
    /// * >150 ms ⇒ 3 hops  (high latency)
    #[must_use]
    pub fn build_path_dynamic(&self) -> Vec<NodeId> {
        let cands = self.prober.candidates();
        if cands.is_empty() {
            return Vec::new();
        }

        // Compute median RTT among candidates.
        let mut rtts: Vec<f64> = cands.iter().map(|c| c.latency_ms).collect();
        rtts.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = rtts[rtts.len() / 2];

        let hops = if median <= 20.0 {
            7
        } else if median <= 50.0 {
            6
        } else if median <= 100.0 {
            5
        } else if median <= 150.0 {
            4
        } else {
            3
        };

        let builder = WeightedPathBuilder::new(&cands, self.alpha);
        builder.build_path(hops)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_rtt_high_bw_preferred() {
        let mut prober = Prober::new();
        let fast: NodeId = [1u8; 32];
        let slow: NodeId = [2u8; 32];
        // Fast node: RTT 10ms, 500Mbps
        prober.record_rtt(fast, 10.0);
        prober.record_throughput(fast, 500.0);
        // Slow node: RTT 100ms, 50Mbps
        prober.record_rtt(slow, 100.0);
        prober.record_throughput(slow, 50.0);

        let planner = LARMixPlanner::new(&prober, 0.7); // latency-biased
        let mut fast_cnt = 0;
        let mut slow_cnt = 0;
        for _ in 0..200 {
            let path = planner.build_path(1);
            if path[0] == fast { fast_cnt += 1; } else { slow_cnt += 1; }
        }
        assert!(fast_cnt > slow_cnt, "fast_cnt={fast_cnt}, slow_cnt={slow_cnt}");
    }
} 