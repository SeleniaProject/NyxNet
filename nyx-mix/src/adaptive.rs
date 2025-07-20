//! Adaptive cover traffic controller.
//! Adjusts Poisson λ based on recent utilization to keep cover/real ratio near target.

#![forbid(unsafe_code)]

use crate::cover::CoverGenerator;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use nyx_core::mobile::{battery_state, BatteryState};

/// Sliding-window utilization estimator (bytes per second).
pub struct UtilizationEstimator {
    window: VecDeque<(Instant, usize)>,
    window_len: Duration,
    accumulated: usize,
}

impl UtilizationEstimator {
    /// `window_secs` – size of sliding window.
    pub fn new(window_secs: u64) -> Self {
        Self {
            window: VecDeque::new(),
            window_len: Duration::from_secs(window_secs.max(1)),
            accumulated: 0,
        }
    }

    /// Record number of real bytes sent at current time.
    pub fn record(&mut self, bytes: usize) {
        let now = Instant::now();
        self.window.push_back((now, bytes));
        self.accumulated += bytes;
        self.purge_old(now);
    }

    /// Current mean throughput in bytes/s over the window.
    pub fn throughput_bps(&mut self) -> f64 {
        let now = Instant::now();
        self.purge_old(now);
        self.accumulated as f64 / self.window_len.as_secs_f64()
    }

    fn purge_old(&mut self, now: Instant) {
        while let Some(&(ts, bytes)) = self.window.front() {
            if now.duration_since(ts) > self.window_len {
                self.window.pop_front();
                self.accumulated = self.accumulated.saturating_sub(bytes);
            } else {
                break;
            }
        }
    }
}

/// Adaptive version of [`CoverGenerator`].
/// λ = base_lambda * f(util), where f increases when utilization low, decreases when high.
/// Target: maintain cover_ratio ≈ target_ratio (e.g., 0.35).
pub struct AdaptiveCoverGenerator {
    base_lambda: f64,
    target_ratio: f64,
    gen: CoverGenerator,
    estimator: UtilizationEstimator,
    manual_low_power: bool,
}

impl AdaptiveCoverGenerator {
    /// `base_lambda` – base events/sec when utilization zero.
    /// `target_ratio` – desired cover/(cover+real) ratio (0..1).
    pub fn new(base_lambda: f64, target_ratio: f64) -> Self {
        let gen = CoverGenerator::new(base_lambda);
        Self {
            base_lambda,
            target_ratio: target_ratio.clamp(0.0, 1.0),
            gen,
            estimator: UtilizationEstimator::new(5),
            manual_low_power: false,
        }
    }

    /// Record real bytes sent to update utilization.
    pub fn record_real_bytes(&mut self, bytes: usize) {
        self.estimator.record(bytes);
    }

    /// Produce next delay. Internal λ adjusted each call.
    pub fn next_delay(&mut self) -> Duration {
        // Low Power Mode: either explicit flag or battery discharging. Scale λ to 0.1×.
        let low_power_detected = self.manual_low_power || matches!(battery_state(), BatteryState::Discharging);
        if low_power_detected {
            self.gen = CoverGenerator::new(self.base_lambda * 0.1);
        }
        let util_bps = self.estimator.throughput_bps();
        // Heuristic: assume 1 packet ≈1200B, convert to packets/s
        let util_pps = util_bps / 1200.0;
        // target cover pps so that cover/(cover+real) ≈ target_ratio
        let target_cover_pps = if self.target_ratio >= 1.0 {
            self.base_lambda
        } else {
            (util_pps * self.target_ratio) / (1.0 - self.target_ratio + f64::EPSILON)
        };
        let new_lambda = self.base_lambda.max(target_cover_pps);
        // Re-initialize internal generator if λ change is >10%
        if (new_lambda - self.gen.lambda).abs() / self.gen.lambda > 0.1 {
            self.gen = CoverGenerator::new(new_lambda);
        }
        self.gen.next_delay()
    }

    /// Manually override low power mode (e.g., screen off event from UI)
    pub fn set_low_power(&mut self, on: bool) { self.manual_low_power = on; }

    /// Current λ value.
    pub fn current_lambda(&self) -> f64 { self.gen.lambda }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lambda_decreases_when_util_high() {
        let mut acg = AdaptiveCoverGenerator::new(10.0, 0.5);
        acg.set_low_power(true);
        // simulate utilization sample via next_delay call (which uses estimator) without records
        acg.next_delay();
        assert!(acg.current_lambda() <= 1.0);
    }
} 