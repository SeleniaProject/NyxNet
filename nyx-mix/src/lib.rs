#![forbid(unsafe_code)]

//! Nyx Mix Routing layer (initial skeleton).
//!
//! PathBuilder selects a sequence of NodeId representing mix hops.

use nyx_core::NodeId;

pub mod cmix;
pub use cmix::{CmixController, verify_batch};

pub mod cover;
pub mod cover_adaptive;
pub use cover::CoverGenerator;
pub use cover_adaptive::AdaptiveCoverGenerator as CoverTrafficGenerator;
pub mod adaptive;
pub use adaptive::{AdaptiveCoverGenerator, UtilizationEstimator};
pub mod larmix;
pub use larmix::{Prober, LARMixPlanner};
pub mod accumulator;
pub mod vdf;
pub mod vdf_calib;
pub use vdf_calib::calibrate_t;
pub mod anonymity;
pub use anonymity::AnonymityEvaluator as AnonymitySet;

/// Candidate node with runtime metrics for weighted selection.
#[derive(Debug, Clone, Copy)]
pub struct Candidate {
    pub id: NodeId,
    /// Smoothed RTT latency (milliseconds).
    pub latency_ms: f64,
    /// Available upstream bandwidth (Mbps).
    pub bandwidth_mbps: f64,
}

/// WeightedPathBuilder selects nodes preferring lower latency & higher bandwidth.
/// Weight formula (heuristic):
///   w = (1 / latency_ms) * ALPHA  +  (bandwidth_mbps / MAX_BW) * (1-ALPHA)
/// where `ALPHA` balances latency vs bandwidth importance (default 0.7).
pub struct WeightedPathBuilder<'a> {
    candidates: &'a [Candidate],
    alpha: f64,
}

impl<'a> WeightedPathBuilder<'a> {
    /// Create new builder.
    /// `alpha` in 0.0..=1.0, higher = latency-biased.
    pub fn new(candidates: &'a [Candidate], alpha: f64) -> Self {
        Self { candidates, alpha: alpha.clamp(0.0, 1.0) }
    }

    /// Build a path with weighted random sampling **without replacement**.
    /// This prevents the same node appearing multiple times.
    pub fn build_path(&self, hops: usize) -> Vec<NodeId> {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        // Compute weights.
        const MAX_BW: f64 = 1000.0; // 1 Gbps reference
        let mut pool: Vec<(usize, f64)> = self
            .candidates
            .iter()
            .enumerate()
            .map(|(idx, cand)| {
                let lat_score = (1.0 / cand.latency_ms.max(1.0)) * self.alpha;
                let bw_score = (cand.bandwidth_mbps / MAX_BW).min(1.0) * (1.0 - self.alpha);
                (idx, lat_score + bw_score)
            })
            .collect();

        let mut path = Vec::with_capacity(hops);
        for _ in 0..hops {
            if pool.is_empty() {
                break;
            }
            // Total weight.
            let total: f64 = pool.iter().map(|(_, w)| *w).sum();
            let mut pick = rng.gen::<f64>() * total;
            let pos = pool
                .iter()
                .position(|(_, w)| {
                    if pick <= *w {
                        true
                    } else {
                        pick -= *w;
                        false
                    }
                })
                .unwrap();
            let (idx, _) = pool.swap_remove(pos);
            path.push(self.candidates[idx].id);
        }
        path
    }
}

// PathBuilder removed in favor of WeightedPathBuilder which accounts for latency/bandwidth metrics.

#[cfg(test)]
mod weighted_tests {
    use super::*;

    #[test]
    fn weighted_selection_prefers_low_latency() {
        // Two nodes: one low-latency, one high-latency.
        let a = Candidate { id: [1u8;32], latency_ms: 20.0, bandwidth_mbps: 100.0 };
        let b = Candidate { id: [2u8;32], latency_ms: 200.0, bandwidth_mbps: 100.0 };
        let cands = [a, b];
        let builder = WeightedPathBuilder::new(&cands, 0.7);
        let mut low_latency_count = 0;
        for _ in 0..1000 {
            let path = builder.build_path(1);
            if path[0] == a.id { low_latency_count += 1; }
        }
        // Expect low-latency node picked majority of the time.
        assert!(low_latency_count > 600);
    }
}
