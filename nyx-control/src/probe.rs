#![forbid(unsafe_code)]

//! RTT & Bandwidth prober for Nyx Control Plane.
//!
//! This component periodically sends PING frames (Type=0x31) to peer nodes and
//! records the round-trip-time when a corresponding PONG (Type=0x32) is
//! observed.  It also offers helper methods to feed throughput measurements
//! (bytes transferred over time) so that higher-level routing logic such as
//! LARMix++ can obtain smoothed latency/bandwidth estimates for every node.
//!
//! The implementation is protocol-agnostic and **does not** perform any actual
//! network I/O.  Integrators are expected to call [`build_ping`] to obtain a
//! [`PingFrame`], dispatch it over the transport, and later invoke [`on_pong`]
//! once the matching `PongFrame` is delivered.  Bandwidth samples can be
//! recorded via [`record_bytes`].
//!
//! All internal state is kept lock-free; callers are responsible for ensuring
//! thread-safety (e.g., wrap in `Mutex`) when the same instance is used from
//! multiple tasks.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use nyx_core::NodeId;
use nyx_stream::management::{PingFrame, PongFrame};
use rand::{rngs::ThreadRng, Rng, thread_rng};

/// Smoothing factors (exponential moving average) for RTT / bandwidth.
const RTT_ALPHA: f64 = 0.2;
const BW_ALPHA: f64 = 0.2;

/// Per-node smoothed metrics.
#[derive(Debug, Clone, Copy)]
pub struct NodeMetrics {
    /// Smoothed RTT in **milliseconds**.
    pub rtt_ms: f64,
    /// Smoothed upstream throughput in **megabits per second**.
    pub bw_mbps: f64,
}

impl NodeMetrics {
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
}

/// Pending outbound PING information (used to match PONGs).
struct PendingPing {
    node: NodeId,
    sent: Instant,
}

/// Runtime prober maintaining RTT & bandwidth estimates for every known node.
#[derive(Default)]
pub struct RttBwProber {
    outstanding: HashMap<u64, PendingPing>,
    metrics: HashMap<NodeId, NodeMetrics>,
    rng: ThreadRng,
}

impl RttBwProber {
    /// Create a new empty prober.
    #[must_use]
    pub fn new() -> Self {
        Self {
            outstanding: HashMap::new(),
            metrics: HashMap::new(),
            rng: thread_rng(),
        }
    }

    /// Build a [`PingFrame`] destined for `node` and start RTT timer.
    ///
    /// The returned frame must be dispatched by the caller.  When its matching
    /// `PongFrame` is later received, call [`on_pong`] with that frame to
    /// finalize the RTT measurement.
    #[must_use]
    pub fn build_ping(&mut self, node: NodeId) -> PingFrame {
        // Generate a nonce unique among currently outstanding probes.
        let nonce: u64 = loop {
            let n: u64 = self.rng.gen();
            if !self.outstanding.contains_key(&n) {
                break n;
            }
        };
        self.outstanding.insert(nonce, PendingPing { node, sent: Instant::now() });
        PingFrame { nonce }
    }

    /// Handle an incoming `PongFrame`.  If a matching outstanding probe exists,
    /// the RTT is computed and the corresponding node metrics are updated.
    pub fn on_pong(&mut self, frame: &PongFrame) {
        if let Some(ping) = self.outstanding.remove(&frame.nonce) {
            let rtt_ms = ping.sent.elapsed().as_secs_f64() * 1_000.0;
            let entry = self
                .metrics
                .entry(ping.node)
                .or_insert(NodeMetrics { rtt_ms: 0.0, bw_mbps: 0.0 });
            entry.update_rtt(rtt_ms);
        }
    }

    /// Record `bytes` transferred over `elapsed` duration for `node`.
    ///
    /// Throughput is converted to **Mbps** and fed into a smoothed estimator.
    pub fn record_bytes(&mut self, node: NodeId, bytes: usize, elapsed: Duration) {
        let mbps = (bytes as f64 * 8.0) / (elapsed.as_secs_f64() * 1_000_000.0);
        let entry = self
            .metrics
            .entry(node)
            .or_insert(NodeMetrics { rtt_ms: 0.0, bw_mbps: 0.0 });
        entry.update_bw(mbps);
    }

    /// Get current metrics for `node`, if any.
    #[must_use]
    pub fn metrics(&self, node: &NodeId) -> Option<NodeMetrics> { self.metrics.get(node).copied() }

    /// Export all known metrics as `(NodeId, NodeMetrics)` vector.
    #[must_use]
    pub fn export_all(&self) -> Vec<(NodeId, NodeMetrics)> {
        self.metrics.iter().map(|(id, m)| (*id, *m)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtt_measurement_updates_metric() {
        let mut prober = RttBwProber::new();
        let node: NodeId = [1u8; 32];

        let ping = prober.build_ping(node);
        // Simulate 50ms RTT by manipulating the stored timestamp.
        if let Some(p) = prober.outstanding.get_mut(&ping.nonce) {
            p.sent -= Duration::from_millis(50);
        }
        prober.on_pong(&PongFrame { nonce: ping.nonce });

        let m = prober.metrics(&node).expect("metrics");
        assert!(m.rtt_ms >= 49.0 && m.rtt_ms <= 51.0, "rtt={}ms", m.rtt_ms);
    }

    #[test]
    fn bandwidth_smoothing() {
        let mut prober = RttBwProber::new();
        let node: NodeId = [9u8; 32];

        prober.record_bytes(node, 1_280_000, Duration::from_millis(100)); // ≈102.4Mbps
        let first = prober.metrics(&node).unwrap().bw_mbps;
        assert!(first > 100.0 && first < 110.0);

        // Second sample lower → smoothed value should decrease but not abruptly.
        prober.record_bytes(node, 128_000, Duration::from_millis(100)); // ≈10Mbps
        let second = prober.metrics(&node).unwrap().bw_mbps;
        assert!(second < first);
        assert!(second > 10.0); // smoothed
    }
} 